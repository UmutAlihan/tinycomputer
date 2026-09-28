//! `tinycomputer-cursor-overlay`: draws the agent's cursor above everything
//! on screen.
//!
//! The module starts this helper the first time the cursor is shown and
//! writes one [`OverlayCommand`] per line to its standard input. The helper
//! shows a small click-through window that never takes focus and never
//! appears in the Dock, taskbar, or window switcher, and moves it along each
//! glide. It exits when its input closes, so it never outlives the module.
//!
//! Only putting pixels on screen is platform code (`platform/`): what the
//! cursor looks like and how it moves come from `tinycomputer-cursor`, so it
//! is the same cursor on macOS and Windows. Elsewhere the helper reads its
//! input and draws nothing.
//!
//! [`OverlayCommand`]: tinycomputer_cursor::OverlayCommand

mod driver;
mod platform;

use std::io::{BufRead, BufReader, Read};
use std::sync::mpsc::{self, Receiver, Sender};

use crate::driver::Driver;
use tinycomputer_cursor::OverlayCommand;

#[cfg(not(test))]
fn main() {
    run(std::io::stdin(), platform::run);
}

fn run(input: impl Read + Send + 'static, launch: impl FnOnce(Driver)) {
    let (commands, received): (_, Receiver<OverlayCommand>) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        forward_lines(input, &commands);
        // Dropping the sender tells the driver the module has gone.
    });
    launch(driver::Driver::new(received));
    let _ = reader.join();
}

fn forward_lines(input: impl Read, commands: &Sender<OverlayCommand>) {
    for line in BufReader::new(input).lines() {
        let Ok(line) = line else { break };
        if let Some(command) = OverlayCommand::from_line(&line)
            && commands.send(command).is_err()
        {
            break;
        }
    }
}

#[cfg(test)]
mod main_tests;
