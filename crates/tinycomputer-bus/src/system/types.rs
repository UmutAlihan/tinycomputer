//! The payload for `Permissions`.

use serde::{Deserialize, Serialize};

/// The argument to [`crate::names::methods::PERMISSIONS`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PermissionsRequest {
    /// Whether to prompt for anything not yet granted. `false`, the default,
    /// reports the current state and puts no dialog on screen.
    pub request: bool,
}
