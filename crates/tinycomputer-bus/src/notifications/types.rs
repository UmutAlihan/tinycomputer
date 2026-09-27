//! Payloads for the notification members.

use serde::{Deserialize, Serialize};

/// The argument to [`crate::names::methods::LIST_NOTIFICATIONS`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ListNotificationsRequest {
    /// Return only notifications from this application.
    pub app: Option<String>,
    /// Return only notifications containing this text.
    pub text: Option<String>,
    /// Return at most this many.
    pub limit: Option<usize>,
}

/// The argument to [`crate::names::methods::NOTIFICATION_ACTION`].
///
/// Both `expected_app` and `expected_title` are required by the engine even
/// though they are typed as optional here, because the wire form has to be able
/// to carry their absence in order to report it as `INVALID_ARGS` rather than
/// as a decode failure.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NotificationActionRequest {
    /// The position in the list the action targets.
    pub index: usize,
    /// The action to invoke, by its accessible name.
    pub action: String,
    /// The application the caller believes owns that notification.
    pub expected_app: Option<String>,
    /// The title the caller believes that notification carries.
    pub expected_title: Option<String>,
}

/// The argument to [`crate::names::methods::DISMISS_NOTIFICATION`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DismissNotificationRequest {
    /// The position in the list to dismiss.
    pub index: usize,
    /// Narrow the list to one application before indexing into it.
    pub app: Option<String>,
    /// The application the caller believes owns that notification.
    pub expected_app: Option<String>,
    /// The title the caller believes that notification carries.
    pub expected_title: Option<String>,
}

/// The argument to [`crate::names::methods::DISMISS_ALL_NOTIFICATIONS`].
///
/// The reply reports what was dismissed and what refused to be, so a partial
/// success is visible rather than rounded up to `ok`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DismissAllNotificationsRequest {
    /// Dismiss only this application's notifications. Absent dismisses every
    /// notification.
    pub app: Option<String>,
}
