//! Platforms with no overlay yet: read commands until the module goes, and
//! draw nothing.

use crate::driver::{Driver, Tick};

pub(crate) fn run(mut driver: Driver) {
    run_with_sleep(
        &mut driver,
        std::thread::sleep,
        std::time::Duration::from_millis(100),
    );
}

fn run_with_sleep(
    driver: &mut Driver,
    mut sleep: impl FnMut(std::time::Duration),
    interval: std::time::Duration,
) {
    while driver.tick() != Tick::Quit {
        sleep(interval);
    }
}

#[cfg(test)]
#[path = "test.rs"]
mod test;
