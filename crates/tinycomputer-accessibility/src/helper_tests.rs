//! Tests for the platform-independent entry points of the Swift helper module.

#[cfg(not(target_os = "macos"))]
#[test]
fn precompile_entry_point_is_a_noop_off_macos() {
    super::precompile_helper_background();
}
