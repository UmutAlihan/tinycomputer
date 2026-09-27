//! Putting the picture on screen, per platform.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub(crate) use macos::run;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub(crate) use windows::run;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod other;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) use other::run;

/// How often the overlay redraws: a display's refresh.
#[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
pub(crate) const FRAME_SECONDS: f64 = 1.0 / 60.0;
