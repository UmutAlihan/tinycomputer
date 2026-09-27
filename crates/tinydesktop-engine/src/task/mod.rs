//! The task controller behind the Agent interface.
//!
//! [`Tasks`] takes a task — a flow and the facts it may type — runs it in
//! the background, and reports it as a [`TaskView`] a model can act on. It
//! pauses only for what the caller must decide:
//!
//! - **missing values** — a `${name}` the flow uses and no fact supplies
//!   becomes `needs_input` before anything runs;
//! - **irreversible actions** — a `stop_before` the task may not perform
//!   becomes `needs_approval`, and approving it performs that action and
//!   carries on with the steps after it;
//! - **payment** — reaching a payment control is always a final checkpoint.
//!
//! Facts reach the flow as variables, so they are typed locally; the flow
//! runs with `include_values` off, and every summary is redacted of them.
//!
//! How a flow actually runs is behind [`FlowRunner`], so this controller is
//! tested with scripted runs and the module plugs in the real flow runtime.

mod interpret;

use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tinydesktop_bus::agent::{
    AgentError, AgentResponse, AwaitTaskRequest, ContinueTaskRequest, InputField, InputKind,
    StartTaskRequest, StepView, TaskBudget, TaskConstraints, TaskId, TaskReport, TaskStatus,
    TaskView,
};
use tinydesktop_bus::{
    DesktopResponse, FLOW_GUIDE, Flow, FlowAction, FlowStep, GroundingHint, JevExchange,
    RunFlowRequest, StepReport,
};
use tinydesktop_core::Facts;
use tokio::sync::watch;

use interpret::{Next, Resume, finished, run_outcome};

/// The future a [`FlowRunner`] returns: the flow runtime's reply envelope.
pub type FlowFuture = Pin<Box<dyn Future<Output = DesktopResponse> + Send>>;

/// Runs one flow for a task.
pub trait FlowRunner: Send + Sync + 'static {
    /// Runs `request` within `constraints`, returning `RunFlow`'s reply.
    fn run(&self, constraints: &TaskConstraints, request: RunFlowRequest) -> FlowFuture;
}

/// How many tasks the controller holds; finished ones are dropped first.
pub const MAX_TASKS: usize = 32;

/// The longest a single `AwaitTask` waits.
pub const MAX_AWAIT_MS: u64 = 60_000;

/// The task controller.
pub struct Tasks {
    runner: Arc<dyn FlowRunner>,
    tasks: Mutex<BTreeMap<u64, Arc<Cell>>>,
    counter: AtomicU64,
}

impl std::fmt::Debug for Tasks {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Tasks").finish_non_exhaustive()
    }
}

/// One task: its view, published to waiters, and its working state.
struct Cell {
    view: watch::Sender<TaskView>,
    state: Mutex<State>,
    worker: Mutex<Option<tokio::task::AbortHandle>>,
}

struct State {
    flow: Flow,
    facts: Facts,
    constraints: TaskConstraints,
    budget: TaskBudget,
    memory: Vec<GroundingHint>,
    trace: bool,
    steps: Vec<StepReport>,
    exchanges: Vec<JevExchange>,
    learned: Vec<GroundingHint>,
    reads: BTreeMap<String, String>,
    finished: usize,
    resume: Option<Resume>,
}

/// One flow run in a task's sequence.
struct Run {
    flow: Flow,
    allow_destructive: bool,
}

impl Tasks {
    /// A controller that runs flows with `runner`.
    #[must_use]
    pub fn new(runner: Arc<dyn FlowRunner>) -> Self {
        Self {
            runner,
            tasks: Mutex::new(BTreeMap::new()),
            counter: AtomicU64::new(0),
        }
    }

    /// Starts a task and returns at once with its first view.
    ///
    /// # Panics
    ///
    /// When called outside a Tokio runtime: the task runs on a spawned worker.
    #[must_use]
    pub fn start(&self, request: StartTaskRequest) -> AgentResponse<TaskView> {
        let facts = match Facts::new(request.facts.clone()) {
            Ok(facts) => facts,
            Err(error) => {
                return AgentResponse::err(AgentError::new(
                    "CARD_DATA_REFUSED",
                    error.to_string(),
                    "remove payment card details; the task stops at payment for you to finish",
                    true,
                ));
            }
        };
        let Some(flow) = request.flow.clone() else {
            if request.task.is_some() {
                return self.register_planless(request);
            }
            return AgentResponse::err(AgentError::new(
                "INVALID_REQUEST",
                "a task needs `flow`, or `task` with a planner configured",
                "write a flow with Describe's guide and pass it as `flow`",
                true,
            ));
        };
        let known = known_names(&flow, &facts);
        let validation = crate::agentic::check_flow(&flow, &known);
        let problems = validation
            .errors
            .iter()
            .filter(|error| !is_undefined(error))
            .cloned()
            .collect::<Vec<_>>();
        if !problems.is_empty() {
            return AgentResponse::err(AgentError::new(
                "INVALID_FLOW",
                problems.join("; "),
                "fix the flow; Describe returns the guide",
                true,
            ));
        }
        let Some(cell) = self.register(&flow, facts, &request) else {
            return too_many();
        };
        let missing = crate::agentic::missing_inputs(&flow, &known);
        if missing.is_empty() {
            self.spawn(
                &cell,
                vec![Run {
                    allow_destructive: request.constraints.allow_destructive,
                    flow,
                }],
            );
        } else {
            publish(
                &cell,
                needs_input(&missing),
                "The task needs values before it can start.",
            );
        }
        AgentResponse::ok(cell.view.borrow().clone())
    }

    /// Waits until the task is no longer running, or `timeout_ms` passes.
    pub async fn await_task(&self, request: AwaitTaskRequest) -> AgentResponse<TaskView> {
        let Some(cell) = self.find(&request.id) else {
            return no_such_task(&request.id);
        };
        let mut changes = cell.view.subscribe();
        let wait = Duration::from_millis(request.timeout_ms.min(MAX_AWAIT_MS));
        let _waited = tokio::time::timeout(wait, async {
            while changes.borrow().status == TaskStatus::Running {
                if changes.changed().await.is_err() {
                    break;
                }
            }
        })
        .await;
        AgentResponse::ok(changes.borrow().clone())
    }

    /// Answers what a paused task asked for, and resumes it.
    #[must_use]
    pub fn continue_task(&self, request: ContinueTaskRequest) -> AgentResponse<TaskView> {
        let Some(cell) = self.find(&request.id) else {
            return no_such_task(&request.id);
        };
        let status = cell.view.borrow().status.clone();
        match status {
            TaskStatus::NeedsInput { .. } => self.supply(&cell, request),
            TaskStatus::NeedsApproval { .. } => self.decide(&cell, request.approve),
            other => AgentResponse::err(AgentError::new(
                "NOT_WAITING",
                format!(
                    "the task is not waiting for an answer: {}",
                    state_name(&other)
                ),
                "call AwaitTask until the task asks for something",
                true,
            )),
        }
    }

    /// Stops a task. Cancelling one already finished returns it unchanged.
    #[must_use]
    pub fn cancel(&self, id: &TaskId) -> AgentResponse<TaskView> {
        let Some(cell) = self.find(id) else {
            return no_such_task(id);
        };
        if !cell.view.borrow().status.is_final() {
            if let Some(worker) = cell.worker.lock().ok().and_then(|mut worker| worker.take()) {
                worker.abort();
            }
            publish(&cell, TaskStatus::Cancelled, "The task was cancelled.");
        }
        AgentResponse::ok(cell.view.borrow().clone())
    }

    /// Everything the task did.
    #[must_use]
    pub fn report(&self, id: &TaskId) -> AgentResponse<TaskReport> {
        let Some(cell) = self.find(id) else {
            return no_such_task(id);
        };
        let view = cell.view.borrow().clone();
        let Ok(state) = cell.state.lock() else {
            return poisoned();
        };
        AgentResponse::ok(TaskReport {
            view,
            flow: Some(state.flow.clone()),
            steps: state.steps.clone(),
            records: records(&state.reads),
            artifacts: Vec::new(),
            learned: state.learned.clone(),
            trace: state.exchanges.clone(),
        })
    }

    /// Every task held, newest first.
    #[must_use]
    pub fn list(&self) -> AgentResponse<Vec<TaskView>> {
        let Ok(tasks) = self.tasks.lock() else {
            return poisoned();
        };
        AgentResponse::ok(
            tasks
                .values()
                .rev()
                .map(|cell| cell.view.borrow().clone())
                .collect(),
        )
    }

    fn supply(&self, cell: &Arc<Cell>, request: ContinueTaskRequest) -> AgentResponse<TaskView> {
        let Ok(mut state) = cell.state.lock() else {
            return poisoned();
        };
        let mut values = state
            .facts
            .names()
            .into_iter()
            .filter_map(|name| {
                state
                    .facts
                    .get(name)
                    .map(|value| (name.to_owned(), value.to_owned()))
            })
            .collect::<BTreeMap<_, _>>();
        values.extend(request.inputs);
        match Facts::new(values) {
            Ok(facts) => state.facts = facts,
            Err(error) => {
                return AgentResponse::err(AgentError::new(
                    "CARD_DATA_REFUSED",
                    error.to_string(),
                    "remove payment card details; the task stops at payment for you to finish",
                    true,
                ));
            }
        }
        let known = known_names(&state.flow, &state.facts);
        let missing = crate::agentic::missing_inputs(&state.flow, &known);
        if !missing.is_empty() {
            drop(state);
            publish(
                cell,
                needs_input(&missing),
                "The task still needs values before it can start.",
            );
            return AgentResponse::ok(cell.view.borrow().clone());
        }
        let run = Run {
            flow: state.flow.clone(),
            allow_destructive: state.constraints.allow_destructive,
        };
        drop(state);
        self.spawn(cell, vec![run]);
        AgentResponse::ok(cell.view.borrow().clone())
    }

    fn decide(&self, cell: &Arc<Cell>, approve: Option<bool>) -> AgentResponse<TaskView> {
        let Some(approve) = approve else {
            return AgentResponse::err(AgentError::new(
                "APPROVAL_REQUIRED",
                "the task is waiting for an approval",
                "call ContinueTask with approve set to true or false",
                true,
            ));
        };
        let Ok(mut state) = cell.state.lock() else {
            return poisoned();
        };
        let resume = state.resume.take();
        let allow = state.constraints.allow_destructive;
        let vars = state.flow.vars.clone();
        drop(state);
        match (approve, resume) {
            (true, Some(resume)) => {
                let mut runs = vec![Run {
                    flow: Flow {
                        app: resume.app.clone(),
                        vars: vars.clone(),
                        steps: vec![FlowStep::Action(FlowAction::StopBefore(resume.phrase))],
                    },
                    allow_destructive: true,
                }];
                if !resume.rest.is_empty() {
                    runs.push(Run {
                        flow: Flow {
                            app: resume.app,
                            vars,
                            steps: resume.rest,
                        },
                        allow_destructive: allow,
                    });
                }
                self.spawn(cell, runs);
            }
            _ => publish(
                cell,
                TaskStatus::Cancelled,
                "The irreversible action was declined, so the task stopped before it.",
            ),
        }
        AgentResponse::ok(cell.view.borrow().clone())
    }

    fn register(&self, flow: &Flow, facts: Facts, request: &StartTaskRequest) -> Option<Arc<Cell>> {
        let number = self.counter.fetch_add(1, Ordering::Relaxed) + 1;
        let id = TaskId::new(format!("t-{number}"));
        let view = TaskView {
            id,
            status: TaskStatus::Running,
            summary: "The task is starting.".to_owned(),
            step: None,
            progress: 0.0,
            next: next_calls(&TaskStatus::Running),
        };
        let cell = Arc::new(Cell {
            view: watch::Sender::new(view),
            state: Mutex::new(State {
                flow: flow.clone(),
                facts,
                constraints: request.constraints.clone(),
                budget: request.budget,
                memory: request.memory.clone(),
                trace: request.trace,
                steps: Vec::new(),
                exchanges: Vec::new(),
                learned: Vec::new(),
                reads: BTreeMap::new(),
                finished: 0,
                resume: None,
            }),
            worker: Mutex::new(None),
        });
        let mut tasks = self.tasks.lock().ok()?;
        while tasks.len() >= MAX_TASKS {
            let oldest_final = tasks
                .iter()
                .find(|(_, cell)| cell.view.borrow().status.is_final())
                .map(|(number, _)| *number)?;
            tasks.remove(&oldest_final);
        }
        tasks.insert(number, cell.clone());
        Some(cell)
    }

    fn register_planless(&self, request: StartTaskRequest) -> AgentResponse<TaskView> {
        let flow = Flow {
            app: String::new(),
            vars: BTreeMap::new(),
            steps: Vec::new(),
        };
        let Some(cell) = self.register(&flow, Facts::default(), &request) else {
            return too_many();
        };
        publish(
            &cell,
            TaskStatus::NeedsPlan {
                guide: FLOW_GUIDE.to_owned(),
            },
            "No planner is configured: write a flow for this task with the guide and start it again.",
        );
        AgentResponse::ok(cell.view.borrow().clone())
    }

    fn spawn(&self, cell: &Arc<Cell>, runs: Vec<Run>) {
        publish(cell, TaskStatus::Running, "The task is running.");
        let worker = tokio::spawn(drive(cell.clone(), self.runner.clone(), runs));
        if let Ok(mut slot) = cell.worker.lock() {
            *slot = Some(worker.abort_handle());
        }
    }

    fn find(&self, id: &TaskId) -> Option<Arc<Cell>> {
        let number = id.0.strip_prefix("t-")?.parse().ok()?;
        self.tasks.lock().ok()?.get(&number).cloned()
    }
}

/// Runs a task's flows in order until one stops it or all finish.
async fn drive(cell: Arc<Cell>, runner: Arc<dyn FlowRunner>, runs: Vec<Run>) {
    for run in runs {
        let (request, constraints) = {
            let Ok(state) = cell.state.lock() else {
                return;
            };
            let mut vars = run.flow.vars.clone();
            for name in state.facts.names() {
                if let Some(value) = state.facts.get(name) {
                    vars.insert(name.to_owned(), value.to_owned());
                }
            }
            (
                RunFlowRequest {
                    flow: run.flow.clone(),
                    vars,
                    allow_destructive: run.allow_destructive,
                    include_values: false,
                    max_actions: state.budget.max_actions.unwrap_or(120),
                    max_model_calls: state.budget.max_model_calls.unwrap_or(300),
                    memory: state.memory.clone(),
                    trace: state.trace,
                    ..RunFlowRequest::default()
                },
                state.constraints.clone(),
            )
        };
        let reply = runner.run(&constraints, request).await;
        let (next, result) = run_outcome(&run.flow, &reply);
        let redacted = {
            let Ok(mut state) = cell.state.lock() else {
                return;
            };
            if let Some(result) = result {
                state.finished += finished(&result.steps);
                state.steps.extend(result.steps);
                state.exchanges.extend(result.trace);
                state.learned.extend(result.learned);
                for (name, value) in result.vars {
                    if state.facts.get(&name).is_none() && !run.flow.vars.contains_key(&name) {
                        state.reads.insert(name, value);
                    }
                }
            }
            if let Next::Stop { resume, .. } = &next {
                state.resume.clone_from(resume);
            }
            state.facts.clone()
        };
        if let Next::Stop { status, .. } = next {
            let summary = redacted.redact(&stopped_summary(&status));
            publish(&cell, status, &summary);
            return;
        }
    }
    let answer = {
        let Ok(state) = cell.state.lock() else {
            return;
        };
        let reads = state
            .reads
            .iter()
            .map(|(name, value)| format!("{name}: {value}"))
            .collect::<Vec<_>>();
        let answer = if reads.is_empty() {
            format!("Finished all {} steps.", state.flow.steps.len())
        } else {
            format!(
                "Finished all {} steps. {}",
                state.flow.steps.len(),
                reads.join("; ")
            )
        };
        (state.facts.redact(&answer), records(&state.reads))
    };
    publish(
        &cell,
        TaskStatus::Done {
            answer: answer.0.clone(),
            records: answer.1,
        },
        &answer.0,
    );
}

fn stopped_summary(status: &TaskStatus) -> String {
    match status {
        TaskStatus::NeedsApproval { action, target, .. } => {
            format!(
                "Stopped before an irreversible action ({action}: {target}); approve or decline it."
            )
        }
        TaskStatus::Checkpoint { reason, .. } => format!("Stopped: {reason}."),
        TaskStatus::Failed { reason, .. } => format!("The task failed: {reason}"),
        other => format!("The task is {}.", state_name(other)),
    }
}

/// Updates a task's view: status, summary, progress, step, and next calls.
fn publish(cell: &Cell, status: TaskStatus, summary: &str) {
    let (progress, step) = cell.state.lock().map_or((0.0, None), |state| {
        let total = state.flow.steps.len().max(1);
        let fraction = |count: usize| f32::from(u16::try_from(count).unwrap_or(u16::MAX));
        let progress = fraction(state.finished.min(total)) / fraction(total);
        let step = state.steps.last().map(|report| StepView {
            index: interpret::top_index(report.path.split('.').next().unwrap_or("1")).unwrap_or(0),
            total: state.flow.steps.len(),
            kind: report.kind.clone(),
            intent: state.facts.redact(&report.text),
            surface: interpret::app_at(
                &state.flow,
                interpret::top_index(report.path.split('.').next().unwrap_or("1")).unwrap_or(0),
            ),
        });
        (progress, step)
    });
    cell.view.send_modify(|view| {
        view.next = next_calls(&status);
        view.status = status;
        summary.clone_into(&mut view.summary);
        view.progress = progress;
        view.step = step;
    });
}

fn needs_input(missing: &[String]) -> TaskStatus {
    TaskStatus::NeedsInput {
        fields: missing
            .iter()
            .map(|name| InputField {
                name: name.clone(),
                why: format!("the flow uses ${{{name}}} and no fact supplies it"),
                kind: input_kind(name),
                options: Vec::new(),
            })
            .collect(),
    }
}

/// A best guess at a value's kind from its name, for the caller's form.
fn input_kind(name: &str) -> InputKind {
    let name = name.to_ascii_lowercase();
    if name.contains("email") {
        InputKind::Email
    } else if name.contains("phone") || name.contains("mobile") {
        InputKind::Phone
    } else if name.contains("date") || name.contains("birth") || name.contains("dob") {
        InputKind::Date
    } else if name.contains("count") || name.contains("number of") || name.contains("travellers") {
        InputKind::Number
    } else {
        InputKind::Text
    }
}

fn known_names(flow: &Flow, facts: &Facts) -> BTreeSet<String> {
    facts
        .names()
        .into_iter()
        .map(str::to_owned)
        .chain(flow.vars.keys().cloned())
        .collect()
}

fn is_undefined(error: &str) -> bool {
    error.contains("` is not defined in `vars`")
}

fn records(reads: &BTreeMap<String, String>) -> BTreeMap<String, Vec<BTreeMap<String, String>>> {
    reads
        .iter()
        .map(|(name, value)| {
            (
                name.clone(),
                vec![BTreeMap::from([("value".to_owned(), value.clone())])],
            )
        })
        .collect()
}

fn next_calls(status: &TaskStatus) -> Vec<String> {
    let calls: &[&str] = match status {
        TaskStatus::Running => &["AwaitTask", "CancelTask"],
        TaskStatus::NeedsInput { .. } | TaskStatus::NeedsApproval { .. } => {
            &["ContinueTask", "CancelTask"]
        }
        TaskStatus::NeedsHuman { .. } => &["ContinueTask", "CancelTask", "TaskReport"],
        TaskStatus::Checkpoint {
            continuable: true, ..
        } => &["ContinueTask", "TaskReport"],
        TaskStatus::NeedsPlan { .. } => &["StartTask"],
        TaskStatus::Failed { .. } => &["TaskReport", "StartTask"],
        TaskStatus::Checkpoint { .. } | TaskStatus::Done { .. } | TaskStatus::Cancelled => {
            &["TaskReport"]
        }
    };
    calls.iter().map(|call| (*call).to_owned()).collect()
}

fn state_name(status: &TaskStatus) -> &'static str {
    match status {
        TaskStatus::Running => "running",
        TaskStatus::NeedsInput { .. } => "needs_input",
        TaskStatus::NeedsApproval { .. } => "needs_approval",
        TaskStatus::Checkpoint { .. } => "checkpoint",
        TaskStatus::NeedsHuman { .. } => "needs_human",
        TaskStatus::NeedsPlan { .. } => "needs_plan",
        TaskStatus::Done { .. } => "done",
        TaskStatus::Failed { .. } => "failed",
        TaskStatus::Cancelled => "cancelled",
    }
}

fn no_such_task<T>(id: &TaskId) -> AgentResponse<T> {
    AgentResponse::err(AgentError::new(
        "NO_SUCH_TASK",
        format!("task {id} does not exist"),
        "call ListTasks for the tasks this module holds",
        false,
    ))
}

fn too_many() -> AgentResponse<TaskView> {
    AgentResponse::err(AgentError::new(
        "TOO_MANY_TASKS",
        format!("{MAX_TASKS} tasks are already running or waiting"),
        "cancel or finish a task first",
        true,
    ))
}

fn poisoned<T>() -> AgentResponse<T> {
    AgentResponse::err(AgentError::new(
        "INTERNAL",
        "the task store was poisoned by a panic",
        "restart the module",
        false,
    ))
}

#[cfg(test)]
mod test;
