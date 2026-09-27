//! Putting the picture on screen, per platform.

#[cfg(all(target_os = "macos", not(test)))]
mod macos;
#[cfg(all(target_os = "macos", not(test)))]
pub(crate) use macos::run;

#[cfg(all(target_os = "windows", not(test)))]
mod windows;
#[cfg(all(target_os = "windows", not(test)))]
pub(crate) use windows::run;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod other;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
#[cfg(not(test))]
pub(crate) use other::run;
