//! Tests for the task controller over scripted flow runs.
//!
//! The runner hands back queued `RunFlow` replies and records every request,
//! so each pause, resume, and failure path is exercised without Jev or a
//! surface.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};

use serde_json::json;
use tinydesktop_bus::agent::{
    AgentResponse, AwaitTaskRequest, ContinueTaskRequest, InputKind, StartTaskRequest, TaskId,
    TaskStatus, TaskView,
};
use tinydesktop_bus::{
    DesktopError, DesktopResponse, Flow, FlowAction, FlowRunResult, FlowStep, FlowStopReason,
    JevMetrics, JevTarget, RunFlowRequest, StepOutcome, StepReport, TaskConstraintsAlias,
};

use super::interpret::app_at;
use super::{FlowFuture, FlowRunner, MAX_TASKS, Tasks, input_kind, next_calls};
