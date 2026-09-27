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

mod describe;
mod interpret;

use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tinydesktop_bus::agent::{
    AgentError, AgentResponse, AwaitTaskRequest, ContinueTaskRequest, InputField, InputKind,
    PlanTaskRequest, StartTaskRequest, StepView, TaskBudget, TaskConstraints, TaskId, TaskPlan,
    TaskReport, TaskStatus, TaskView,
};
use tinydesktop_bus::{
    DesktopResponse, FLOW_GUIDE, Flow, FlowAction, FlowStep, GroundingHint, JevExchange,
    RunFlowRequest, StepReport,
};
use tinydesktop_core::Facts;
use tokio::sync::watch;

pub use describe::capabilities;

use interpret::{Next, Resume, finished, run_outcome};

/// The future a [`FlowRunner`] returns: the flow runtime's reply envelope.
pub type FlowFuture = Pin<Box<dyn Future<Output = DesktopResponse> + Send>>;

/// The future [`FlowRunner::visible_text`] returns.
pub type TextFuture = Pin<Box<dyn Future<Output = Vec<String>> + Send>>;

/// Runs a task's flows, on surfaces that live as long as the task.
pub trait FlowRunner: Send + Sync + 'static {
    /// Runs `request` for `task` within `constraints`, returning `RunFlow`'s
    /// reply. Runs of one task share its surfaces, so a continuation picks up
    /// on the page or window the last run left.
    fn run(
        &self,
        task: &TaskId,
        constraints: &TaskConstraints,
        request: RunFlowRequest,
    ) -> FlowFuture;

    /// The visible text of the task's surface now, to spot a wall only a
    /// person can pass. Empty by default.
    fn visible_text(&self, _task: &TaskId) -> TextFuture {
        Box::pin(async { Vec::new() })
    }

    /// Lets go of whatever the task held, once it has ended.
    fn release(&self, _task: &TaskId) {}
}

/// How many tasks the controller holds; finished ones are dropped first.
pub const MAX_TASKS: usize = 32;

/// The longest a single `AwaitTask` waits.
pub const MAX_AWAIT_MS: u64 = 60_000;

/// The task controller.
pub struct Tasks {
    runner: Arc<dyn FlowRunner>,
    planner: Option<crate::planner::Planner>,
    cells: Mutex<BTreeMap<u64, Arc<Cell>>>,
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
    /// What every run of this task has spent so far, so an approval or a
    /// human intervention that splits a task into several runs still cannot
    /// exceed its declared budget by starting each run with a fresh one.
    spent: Spent,
}

/// A task's cumulative spend against its [`TaskBudget`], across every run.
#[derive(Debug, Default, Clone, Copy)]
struct Spent {
    actions: u32,
    model_calls: u32,
    elapsed_ms: u64,
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
            planner: None,
            cells: Mutex::new(BTreeMap::new()),
            counter: AtomicU64::new(0),
        }
    }

    /// This controller, turning plain-language tasks into flows with
    /// `planner`.
    #[must_use]
    pub fn with_planner(mut self, planner: crate::planner::Planner) -> Self {
        self.planner = Some(planner);
        self
    }

    /// Whether a planner is configured.
    #[must_use]
    pub fn planner_configured(&self) -> bool {
        self.planner.is_some()
    }

    /// Drafts a flow for a plain-language task without acting.
    pub async fn plan(&self, request: &PlanTaskRequest) -> AgentResponse<TaskPlan> {
        let Some(planner) = &self.planner else {
            return AgentResponse::err(no_planner());
        };
        match planner
            .plan(&request.task, &request.fact_names, &request.surfaces)
            .await
        {
            Ok(plan) => AgentResponse::ok(plan),
            Err(reason) => AgentResponse::err(AgentError::new(
                "PLAN_FAILED",
                reason,
                "reword the task, or write the flow yourself with Describe's guide",
                true,
            )),
        }
    }

    /// Starts a task and returns at once with its first view.
    ///
    /// # Panics
    ///
    /// When called outside a Tokio runtime: the task runs on a spawned worker.
    #[must_use]
    pub fn start(&self, request: &StartTaskRequest) -> AgentResponse<TaskView> {
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
            if let (Some(task), Some(planner)) = (&request.task, &self.planner) {
                return self.start_planned(request, facts, task, planner.clone());
            }
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
        let Some(cell) = self.register(&flow, facts, request) else {
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
            TaskStatus::NeedsHuman { .. } => self.retry(&cell),
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

    /// Stops a task, and lets go of whatever surface it still holds.
    ///
    /// Cancelling one already finished leaves its status unchanged — its
    /// view still reports how it ended — but still releases its workspace.
    /// A payment checkpoint is final without ever running to `Done`, so this
    /// is also its only path to release the browser session it left open for
    /// a person to pay in: without it, every checkout would permanently
    /// consume one of a limited number of session slots.
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
        self.runner.release(id);
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
        let Ok(tasks) = self.cells.lock() else {
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
        if let (true, Some(Resume::Approval { phrase, app, rest })) = (approve, resume) {
            let mut runs = vec![Run {
                flow: Flow {
                    app: app.clone(),
                    vars: vars.clone(),
                    steps: vec![FlowStep::Action(FlowAction::StopBefore(phrase))],
                },
                allow_destructive: true,
            }];
            if !rest.is_empty() {
                runs.push(Run {
                    flow: Flow {
                        app,
                        vars,
                        steps: rest,
                    },
                    allow_destructive: allow,
                });
            }
            self.spawn(cell, runs);
        } else {
            publish(
                cell,
                TaskStatus::Cancelled,
                "The irreversible action was declined, so the task stopped before it.",
            );
            self.runner.release(&cell.view.borrow().id);
        }
        AgentResponse::ok(cell.view.borrow().clone())
    }

    /// Runs the step a person has just got the task past, and the rest.
    fn retry(&self, cell: &Arc<Cell>) -> AgentResponse<TaskView> {
        let Ok(mut state) = cell.state.lock() else {
            return poisoned();
        };
        let resume = state.resume.take();
        let allow = state.constraints.allow_destructive;
        let vars = state.flow.vars.clone();
        drop(state);
        if let Some(Resume::Retry { app, steps }) = resume {
            self.spawn(
                cell,
                vec![Run {
                    flow: Flow { app, vars, steps },
                    allow_destructive: allow,
                }],
            );
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
                spent: Spent::default(),
            }),
            worker: Mutex::new(None),
        });
        let mut tasks = self.cells.lock().ok()?;
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

    fn register_planless(&self, request: &StartTaskRequest) -> AgentResponse<TaskView> {
        let flow = Flow {
            app: String::new(),
            vars: BTreeMap::new(),
            steps: Vec::new(),
        };
        let Some(cell) = self.register(&flow, Facts::default(), request) else {
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

    fn start_planned(
        &self,
        request: &StartTaskRequest,
        facts: Facts,
        task: &str,
        planner: crate::planner::Planner,
    ) -> AgentResponse<TaskView> {
        let placeholder = Flow {
            app: String::new(),
            vars: BTreeMap::new(),
            steps: Vec::new(),
        };
        let Some(cell) = self.register(&placeholder, facts, request) else {
            return too_many();
        };
        publish(&cell, TaskStatus::Running, "Planning the task.");
        let worker = tokio::spawn(plan_then_drive(
            cell.clone(),
            self.runner.clone(),
            planner,
            task.to_owned(),
            request.constraints.surfaces.clone(),
        ));
        if let Ok(mut slot) = cell.worker.lock() {
            *slot = Some(worker.abort_handle());
        }
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
        self.cells.lock().ok()?.get(&number).cloned()
    }
}

/// Plans the task, then runs the plan — or asks for what it needs first.
async fn plan_then_drive(
    cell: Arc<Cell>,
    runner: Arc<dyn FlowRunner>,
    planner: crate::planner::Planner,
    task: String,
    surfaces: Vec<tinydesktop_bus::agent::SurfaceKind>,
) {
    let names = cell.state.lock().map_or_else(
        |_| Vec::new(),
        |state| {
            state
                .facts
                .names()
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        },
    );
    let plan = match planner.plan(&task, &names, &surfaces).await {
        Ok(plan) => plan,
        Err(reason) => {
            publish(
                &cell,
                TaskStatus::Failed {
                    step: None,
                    reason: reason.clone(),
                    hint: "reword the task, or pass a flow written with Describe's guide"
                        .to_owned(),
                    recoverable: true,
                },
                &format!("Planning failed: {reason}"),
            );
            return;
        }
    };
    let allow = {
        let Ok(mut state) = cell.state.lock() else {
            return;
        };
        state.flow = plan.flow.clone();
        state.constraints.allow_destructive
    };
    if plan.questions.is_empty() {
        drive(
            cell,
            runner,
            vec![Run {
                flow: plan.flow,
                allow_destructive: allow,
            }],
        )
        .await;
    } else {
        publish(
            &cell,
            TaskStatus::NeedsInput {
                fields: plan.questions,
            },
            "The plan needs values before it can start.",
        );
    }
}

/// Runs a task's flows in order until one stops it or all finish.
///
/// A task's [`TaskBudget`] bounds the whole task, not one run of it: an
/// approval or a human intervention splits a task into several runs, and
/// each is given only what the task has not already spent, so resuming can
/// never reset the budget back to full. `max_elapsed_ms` has no equivalent in
/// [`RunFlowRequest`] — a run cannot police its own wall-clock time from the
/// inside — so it is enforced here instead, by timing out a run that would
/// otherwise run past what remains of it.
async fn drive(cell: Arc<Cell>, runner: Arc<dyn FlowRunner>, runs: Vec<Run>) {
    for run in runs {
        let Some((request, constraints, time_left)) = (|| {
            let Ok(state) = cell.state.lock() else {
                return None;
            };
            let mut vars = run.flow.vars.clone();
            for name in state.facts.names() {
                if let Some(value) = state.facts.get(name) {
                    vars.insert(name.to_owned(), value.to_owned());
                }
            }
            let max_actions = state
                .budget
                .max_actions
                .unwrap_or(120)
                .saturating_sub(state.spent.actions);
            let max_model_calls = state
                .budget
                .max_model_calls
                .unwrap_or(300)
                .saturating_sub(state.spent.model_calls);
            let time_left = state
                .budget
                .max_elapsed_ms
                .map(|max| max.saturating_sub(state.spent.elapsed_ms));
            Some((
                RunFlowRequest {
                    flow: run.flow.clone(),
                    vars,
                    allow_destructive: run.allow_destructive,
                    include_values: false,
                    max_actions,
                    max_model_calls,
                    memory: state.memory.clone(),
                    trace: state.trace,
                    ..RunFlowRequest::default()
                },
                state.constraints.clone(),
                time_left,
            ))
        })() else {
            return;
        };
        if time_left == Some(0) {
            stop_task(&cell, runner.as_ref(), elapsed_budget_failed());
            return;
        }
        let id = cell.view.borrow().id.clone();
        let started = Instant::now();
        let run_call = runner.run(&id, &constraints, request);
        let reply = match time_left {
            Some(ms) => match tokio::time::timeout(Duration::from_millis(ms), run_call).await {
                Ok(reply) => reply,
                Err(_) => {
                    stop_task(&cell, runner.as_ref(), elapsed_budget_failed());
                    return;
                }
            },
            None => run_call.await,
        };
        let spent_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let (next, result) = run_outcome(&run.flow, &reply);
        let redacted = {
            let Ok(mut state) = cell.state.lock() else {
                return;
            };
            state.spent.elapsed_ms = state.spent.elapsed_ms.saturating_add(spent_ms);
            if let Some(result) = result {
                state.spent.actions = state.spent.actions.saturating_add(result.actions);
                state.spent.model_calls =
                    state.spent.model_calls.saturating_add(result.metrics.calls);
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
            let status = human_wall(&cell, runner.as_ref(), *status).await;
            let summary = redacted.redact(&stopped_summary(&status));
            let ended = matches!(
                status,
                TaskStatus::Done { .. } | TaskStatus::Failed { .. } | TaskStatus::Cancelled
            );
            publish(&cell, status, &summary);
            if ended {
                runner.release(&cell.view.borrow().id);
            }
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
    runner.release(&cell.view.borrow().id);
}

/// A recoverable failure in front of something only a person can pass — a
/// captcha, a one-time code, a login wall — becomes `needs_human`, and the
/// failed step runs again once they have. Anything else stays a failure.
async fn human_wall(cell: &Cell, runner: &dyn FlowRunner, status: TaskStatus) -> TaskStatus {
    let retryable = matches!(
        status,
        TaskStatus::Failed {
            recoverable: true,
            ..
        }
    ) && cell
        .state
        .lock()
        .is_ok_and(|state| matches!(state.resume, Some(Resume::Retry { .. })));
    let wall = if retryable {
        let id = cell.view.borrow().id.clone();
        tinydesktop_core::human_needed(&runner.visible_text(&id).await)
    } else {
        None
    };
    if let Some(action) = wall {
        return TaskStatus::NeedsHuman {
            reason: format!("{action}, then continue the task"),
            screenshot: None,
        };
    }
    // No person can help, so there is nothing to retry.
    if let Ok(mut state) = cell.state.lock()
        && matches!(state.resume, Some(Resume::Retry { .. }))
    {
        state.resume = None;
    }
    status
}

/// Ends a task outright with `status`, without a flow run to interpret: its
/// time budget ran out before a run of it could even start, or a run of it
/// had to be cut off mid-flight to keep from spending past what remains.
fn stop_task(cell: &Cell, runner: &dyn FlowRunner, status: TaskStatus) {
    let summary = stopped_summary(&status);
    publish(cell, status, &summary);
    runner.release(&cell.view.borrow().id);
}

/// The task-level failure `stop_task` reports when `budget.max_elapsed_ms`
/// is spent: never recoverable by a resume, the same as an action or model
/// budget running out inside a run.
fn elapsed_budget_failed() -> TaskStatus {
    TaskStatus::Failed {
        step: None,
        reason: "the task's time budget ran out".to_owned(),
        hint: "raise budget.max_elapsed_ms".to_owned(),
        recoverable: true,
    }
}

fn stopped_summary(status: &TaskStatus) -> String {
    match status {
        TaskStatus::NeedsHuman { reason, .. } => format!("A person is needed: {reason}."),
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
pub(crate) fn input_kind(name: &str) -> InputKind {
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
            // An `extract` stores JSON rows of text; anything else is one value.
            let rows = serde_json::from_str::<Vec<Vec<String>>>(value).map_or_else(
                |_| vec![BTreeMap::from([("value".to_owned(), value.clone())])],
                |rows| {
                    rows.into_iter()
                        .map(|fields| {
                            fields
                                .into_iter()
                                .enumerate()
                                .map(|(index, field)| (format!("field {}", index + 1), field))
                                .collect()
                        })
                        .collect()
                },
            );
            (name.clone(), rows)
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
        // A final checkpoint's workspace is only ever released by
        // `CancelTask`, so it must stay offered even though the task is done.
        TaskStatus::Checkpoint { .. } => &["CancelTask", "TaskReport"],
        TaskStatus::Done { .. } | TaskStatus::Cancelled => &["TaskReport"],
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

fn no_planner() -> AgentError {
    AgentError::new(
        "PLANNER_NOT_CONFIGURED",
        "no planner is configured in this module",
        "write a flow with Describe's guide and pass it as StartTask.flow",
        false,
    )
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
