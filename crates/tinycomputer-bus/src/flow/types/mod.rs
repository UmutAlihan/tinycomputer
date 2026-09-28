//! Wire types for high-level intent flows.

mod step;
mod request;
mod result;

pub use step::{Flow, FlowStep, STEP_KINDS, FlowAction, Slot, Slots, ChooseStep, ReadStep, PickStep, RepeatStep, IfStep};
pub use request::{FlowLoop, Deliberation, FlowStrategy, GroundingHint, RunFlowRequest, FlowBrief, ValidateFlowRequest, FlowValidation};
pub use result::{FlowStopReason, StepOutcome, FlowActionRecord, StepReport, FlowRunResult, JevExchange};

use std::collections::{BTreeMap, BTreeSet};

use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, MapAccess, Visitor},
    ser::SerializeMap,
};
use serde_json::Value;

use crate::{JevMetrics, JevTarget};
