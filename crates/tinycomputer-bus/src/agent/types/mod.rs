//! Task payloads for the Agent interface.

mod describe;
mod reply;
mod report;
mod request;
mod status;

pub use describe::{Capabilities, Example, MemberDoc, SurfaceAvailability};
pub use reply::{AgentError, AgentResponse};
pub use report::{Rescue, RescueOutcome, TaskReport};
pub use request::{
    AwaitTaskRequest, ContinueTaskRequest, PaymentMode, PlanTaskRequest, StartTaskRequest,
    SurfaceKind, TaskBudget, TaskConstraints, TaskId, TaskOutput, TaskPlan, TaskRef,
};
pub use status::{InputField, InputKind, StepView, TaskStatus, TaskView};
