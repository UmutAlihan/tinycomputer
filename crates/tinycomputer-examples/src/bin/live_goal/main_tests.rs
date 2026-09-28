//! Tests for the `live_goal` binary: the mode is checked before module setup.

use super::validate_mode;
use std::ffi::OsStr;

#[test]
fn mode_is_checked_before_module_setup() {
    assert!(validate_mode(OsStr::new("probe")).is_ok());
    assert!(validate_mode(OsStr::new("run")).is_ok());
    assert!(validate_mode(OsStr::new("typo")).is_err());
}
