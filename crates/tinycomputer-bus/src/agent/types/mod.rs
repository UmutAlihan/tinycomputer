//! Task payloads for the Agent interface.




mod reply;
mod request;
mod status;
mod report;
mod describe;

pub use reply::{AgentResponse, AgentError};
pub use request::{TaskId, SurfaceKind, StartTaskRequest, TaskOutput, TaskConstraints, PaymentMode, TaskBudget, AwaitTaskRequest, ContinueTaskRequest, TaskRef, PlanTaskRequest, TaskPlan};
pub use status::{TaskView, StepView, TaskStatus, InputField, InputKind};
pub use report::{TaskReport, Rescue, RescueOutcome};
pub use describe::{Capabilities, SurfaceAvailability, MemberDoc, Example};
