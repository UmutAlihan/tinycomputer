//! Request payloads for the notification-centre members.
//!
//! A notification is addressed by its position in the list a
//! [`ListNotificationsRequest`] just returned, which is a position that moves
//! the moment another notification arrives. Every mutating member here
//! therefore also takes the app and title the caller believes occupy that
//! position, and the engine verifies them before acting. Acting on index 0
//! without saying what index 0 was is not offered: the failure mode is
//! dismissing someone's message because a build finished first.

mod types;

pub use types::{
    DismissAllNotificationsRequest, DismissNotificationRequest, ListNotificationsRequest,
    NotificationActionRequest,
};

#[cfg(test)]
mod test;
