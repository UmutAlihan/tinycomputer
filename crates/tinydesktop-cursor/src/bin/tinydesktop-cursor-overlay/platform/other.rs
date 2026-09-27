//! Platforms with no overlay yet: read commands until the module goes, and
//! draw nothing.

use crate::driver::{Driver, Tick};

pub(crate) fn run(mut driver: Driver) {
    while driver.tick() != Tick::Quit {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}
