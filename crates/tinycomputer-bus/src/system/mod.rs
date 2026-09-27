//! The request payload for the members that report on the module itself.
//!
//! `Version` and `Status` take no argument. [`PermissionsRequest`] takes one
//! field, and it is the field that matters: whether to *prompt*.
//!
//! Desktop automation needs permissions a person has to grant — accessibility
//! access, screen recording — and granting them puts a system dialog in front
//! of whoever is at the machine. An agent should read the report first and
//! decide, so `request` defaults to `false` and a caller has to ask for the
//! dialog on purpose.

mod types;

pub use types::PermissionsRequest;

#[cfg(test)]
mod test;
