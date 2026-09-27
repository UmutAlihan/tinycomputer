//! `Describe`: everything a model needs to drive the Agent members, in one
//! reply — what is available, the flow guide, each member's input schema,
//! and worked requests to adapt.

use serde_json::{Value, json};
use tinydesktop_bus::agent::names::{CONFIDENTIAL, methods};
use tinydesktop_bus::agent::{Capabilities, Example, MemberDoc, SurfaceAvailability};
use tinydesktop_bus::{CONTRACT_VERSION, FLOW_GUIDE, STEP_KINDS};

/// The capabilities reply for a module with these surfaces and whether Jev
/// is configured.
#[must_use]
pub fn capabilities(surfaces: Vec<SurfaceAvailability>, jev_configured: bool) -> Capabilities {
    Capabilities {
        contract_version: CONTRACT_VERSION,
        surfaces,
        jev_configured,
        planner_configured: false,
        step_kinds: STEP_KINDS.iter().map(|kind| (*kind).to_owned()).collect(),
        guide: FLOW_GUIDE.to_owned(),
        members: members(),
        examples: examples(),
    }
}

fn member(name: &str, summary: &str, input: Value, output: &str) -> MemberDoc {
    MemberDoc {
        name: name.to_owned(),
        summary: summary.to_owned(),
        confidential: CONFIDENTIAL.contains(&name),
        input,
        output: json!({"description": output}),
    }
}

fn task_id() -> Value {
    json!({"type": "object", "required": ["id"], "properties": {
        "id": {"type": "string", "description": "the task id StartTask returned"}
    }})
}

fn members() -> Vec<MemberDoc> {
    let object = |properties: Value, required: &[&str]| {
        json!({"type": "object", "required": required, "properties": properties})
    };
    vec![
        member(
            methods::DESCRIBE,
            "How to use these members: surfaces, the flow guide, schemas, and examples.",
            json!({"type": "null"}),
            "Capabilities",
        ),
        member(
            methods::PLAN_TASK,
            "Drafts a flow for a plain-language task without acting; needs a planner.",
            object(
                json!({
                    "task": {"type": "string"},
                    "fact_names": {"type": "array", "items": {"type": "string"}},
                    "surfaces": {"type": "array", "items": {"enum": ["desktop", "browser"]}}
                }),
                &["task"],
            ),
            "TaskPlan: the flow, and the values to collect first",
        ),
        member(
            methods::START_TASK,
            "Starts a task from a flow (or a plain-language task, with a planner) and returns at once.",
            object(
                json!({
                    "task": {"type": "string", "description": "the goal in plain language"},
                    "flow": {"type": "object", "description": "a flow written from the guide"},
                    "facts": {
                        "type": "object",
                        "additionalProperties": {"type": "string"},
                        "description": "values the flow may type, by name; they never reach a model"
                    },
                    "constraints": {"type": "object", "properties": {
                        "surfaces": {"type": "array", "items": {"enum": ["desktop", "browser"]}},
                        "origins": {"type": "array", "items": {"type": "string"}},
                        "allow_destructive": {"type": "boolean"},
                        "browser_endpoint": {"type": "string"},
                        "headed": {"type": "boolean"}
                    }},
                    "budget": {"type": "object", "properties": {
                        "max_actions": {"type": "integer"},
                        "max_model_calls": {"type": "integer"},
                        "max_elapsed_ms": {"type": "integer"}
                    }},
                    "trace": {"type": "boolean"}
                }),
                &[],
            ),
            "TaskView",
        ),
        member(
            methods::AWAIT_TASK,
            "Waits until the task needs something or finishes, up to timeout_ms.",
            object(
                json!({
                    "id": {"type": "string"},
                    "timeout_ms": {"type": "integer", "default": 30_000, "maximum": 60_000}
                }),
                &["id"],
            ),
            "TaskView",
        ),
        member(
            methods::CONTINUE_TASK,
            "Answers a paused task: inputs for needs_input, approve for needs_approval.",
            object(
                json!({
                    "id": {"type": "string"},
                    "inputs": {"type": "object", "additionalProperties": {"type": "string"}},
                    "approve": {"type": "boolean"},
                    "answer": {"type": "string"}
                }),
                &["id"],
            ),
            "TaskView",
        ),
        member(
            methods::CANCEL_TASK,
            "Stops a task.",
            task_id(),
            "TaskView",
        ),
        member(
            methods::TASK_REPORT,
            "Everything a task did: steps, reads, and learned hints.",
            task_id(),
            "TaskReport",
        ),
        member(
            methods::LIST_TASKS,
            "The tasks this module holds, newest first.",
            json!({"type": "null"}),
            "array of TaskView",
        ),
    ]
}

fn examples() -> Vec<Example> {
    vec![
        Example {
            title: "Find the cheapest flight and fill traveller details up to payment".to_owned(),
            member: methods::START_TASK.to_owned(),
            request: json!({
                "flow": {
                    "app": "browser",
                    "steps": [
                        {"browse": "https://www.google.com/travel/flights"},
                        {"enter": {"where from": "${from}", "where to": "${to}", "departure date": "${date}"}},
                        "search for flights",
                        {"wait_for": "flight results are listed"},
                        "sort the results by price, lowest first",
                        {"read": {"what": "the first flight's airline, times, and price", "into": "cheapest"}},
                        "choose the first flight and continue to booking",
                        {"enter": {"first name": "${first name}", "last name": "${last name}", "email": "${email}", "phone": "${phone}"}},
                        {"stop_before": "paying for the booking"}
                    ]
                },
                "facts": {
                    "from": "Delhi",
                    "to": "Srinagar",
                    "date": "14 October",
                    "first name": "Asha",
                    "last name": "Raina",
                    "email": "asha@example.com"
                },
                "constraints": {"surfaces": ["browser"]}
            }),
        },
        Example {
            title: "Wait for the task to need something".to_owned(),
            member: methods::AWAIT_TASK.to_owned(),
            request: json!({"id": "t-1", "timeout_ms": 30_000}),
        },
        Example {
            title: "Supply a value the task asked for".to_owned(),
            member: methods::CONTINUE_TASK.to_owned(),
            request: json!({"id": "t-1", "inputs": {"phone": "+91 98765 43210"}}),
        },
        Example {
            title: "Approve the irreversible action the task stopped before".to_owned(),
            member: methods::CONTINUE_TASK.to_owned(),
            request: json!({"id": "t-1", "approve": true}),
        },
    ]
}
