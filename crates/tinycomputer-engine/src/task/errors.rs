//! The errors the controller answers a call with.

use tinycomputer_bus::agent::{AgentError, AgentResponse, TaskId, TaskView};

use super::MAX_TASKS;

pub(super) fn no_planner() -> AgentError {
    AgentError::new(
        "PLANNER_NOT_CONFIGURED",
        "no planner is configured in this module",
        "write a flow with Describe's guide and pass it as StartTask.flow",
        false,
    )
}

pub(super) fn no_such_task<T>(id: &TaskId) -> AgentResponse<T> {
    AgentResponse::err(AgentError::new(
        "NO_SUCH_TASK",
        format!("task {id} does not exist"),
        "call ListTasks for the tasks this module holds",
        false,
    ))
}

pub(super) fn too_many() -> AgentResponse<TaskView> {
    AgentResponse::err(AgentError::new(
        "TOO_MANY_TASKS",
        format!("{MAX_TASKS} tasks are already running or waiting"),
        "cancel or finish a task first",
        true,
    ))
}

pub(super) fn poisoned<T>() -> AgentResponse<T> {
    AgentResponse::err(AgentError::new(
        "INTERNAL",
        "the task store was poisoned by a panic",
        "restart the module",
        false,
    ))
}
