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

use std::io::BufRead;
use std::sync::mpsc;

use tinycomputer_cursor::OverlayCommand;

fn main() {
    let (commands, received) = mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if let Some(command) = OverlayCommand::from_line(&line)
                && commands.send(command).is_err()
            {
                break;
            }
        }
        // Dropping the sender tells the driver the module has gone.
    });
    platform::run(driver::Driver::new(received));
}
