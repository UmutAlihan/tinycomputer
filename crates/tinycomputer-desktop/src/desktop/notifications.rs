//! The notification-centre members.

use agent_desktop_core::commands::{
    dismiss_all_notifications, dismiss_notification, list_notifications, notification_action,
};
use tinycomputer_bus::{
    DesktopResponse, DismissAllNotificationsRequest, DismissNotificationRequest,
    ListNotificationsRequest, NotificationActionRequest,
};

use super::{Desktop, Need};

impl Desktop {
    /// Lists notification-centre entries.
    ///
    /// The index of each entry is what the mutating members address, and it
    /// moves as notifications arrive — which is why they also take the identity
    /// the caller expects to find there.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tinycomputer_desktop::{Desktop, ListNotificationsRequest};
    /// let reply = Desktop::new().list_notifications(ListNotificationsRequest::default());
    /// assert_eq!(reply.command, "list-notifications");
    /// ```
    #[must_use]
    pub fn list_notifications(&self, request: ListNotificationsRequest) -> DesktopResponse {
        self.run(
            "list-notifications",
            Need::Accessibility,
            |adapter, context| {
                list_notifications::execute(
                    list_notifications::ListNotificationsArgs {
                        app: request.app,
                        text: request.text,
                        limit: request.limit,
                    },
                    adapter,
                    context,
                )
            },
        )
    }

    /// Invokes an action on a notification.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tinycomputer_desktop::{Desktop, NotificationActionRequest};
    /// let reply = Desktop::new().notification_action(NotificationActionRequest {
    ///     index: 0,
    ///     action: "Reply".to_owned(),
    ///     expected_app: Some("Messages".to_owned()),
    ///     expected_title: Some("Ada".to_owned()),
    /// });
    ///
    /// assert_eq!(reply.command, "notification-action");
    /// ```
    #[must_use]
    pub fn notification_action(&self, request: NotificationActionRequest) -> DesktopResponse {
        self.run(
            "notification-action",
            Need::Accessibility,
            |adapter, context| {
                notification_action::execute(
                    notification_action::NotificationActionArgs {
                        index: request.index,
                        action: request.action,
                        expected_app: request.expected_app,
                        expected_title: request.expected_title,
                    },
                    adapter,
                    context,
                )
            },
        )
    }

    /// Dismisses one notification.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tinycomputer_desktop::{Desktop, DismissNotificationRequest};
    /// let reply = Desktop::new().dismiss_notification(DismissNotificationRequest {
    ///     index: 0,
    ///     expected_app: Some("Mail".to_owned()),
    ///     ..DismissNotificationRequest::default()
    /// });
    ///
    /// assert_eq!(reply.command, "dismiss-notification");
    /// ```
    #[must_use]
    pub fn dismiss_notification(&self, request: DismissNotificationRequest) -> DesktopResponse {
        self.run(
            "dismiss-notification",
            Need::Accessibility,
            |adapter, context| {
                dismiss_notification::execute(
                    dismiss_notification::DismissNotificationArgs {
                        index: request.index,
                        app: request.app,
                        expected_app: request.expected_app,
                        expected_title: request.expected_title,
                    },
                    adapter,
                    context,
                )
            },
        )
    }

    /// Dismisses every notification, optionally scoped to one application.
    ///
    /// The reply reports both what was dismissed and what refused, so a partial
    /// success stays visible.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tinycomputer_desktop::{Desktop, DismissAllNotificationsRequest};
    /// let reply =
    ///     Desktop::new().dismiss_all_notifications(DismissAllNotificationsRequest::default());
    /// assert_eq!(reply.command, "dismiss-all-notifications");
    /// ```
    #[must_use]
    pub fn dismiss_all_notifications(
        &self,
        request: DismissAllNotificationsRequest,
    ) -> DesktopResponse {
        self.run(
            "dismiss-all-notifications",
            Need::Accessibility,
            |adapter, context| {
                dismiss_all_notifications::execute(
                    dismiss_all_notifications::DismissAllNotificationsArgs { app: request.app },
                    adapter,
                    context,
                )
            },
        )
    }
}
