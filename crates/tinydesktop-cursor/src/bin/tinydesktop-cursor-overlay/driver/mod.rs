//! [`Driver`]: the platform-independent half of the overlay — commands in,
//! the next picture out.

use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Instant;

use tinydesktop_cursor::animate::{Animator, Frame};
use tinydesktop_cursor::sprite::Sprite;
use tinydesktop_cursor::{OverlayCommand, Point};

/// What the window should show now.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Picture {
    /// The window's top-left corner, in screen points (origin top-left).
    pub(crate) origin: Point,
    /// Which sprite frame to show.
    pub(crate) sprite_frame: usize,
    /// Window opacity, 0 to 1.
    pub(crate) opacity: f64,
}

impl Picture {
    fn of(frame: Frame) -> Self {
        let hotspot = Sprite::hotspot();
        Self {
            origin: Point::new(frame.position.x - hotspot.x, frame.position.y - hotspot.y),
            sprite_frame: Sprite::frame_for(frame.pulse),
            opacity: frame.opacity,
        }
    }
}

/// One tick's result.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Tick {
    /// Show this, and tick again soon.
    Show(Picture),
    /// Show nothing.
    Hidden,
    /// The module has gone; exit.
    Quit,
}

/// Commands in, pictures out, on a clock that starts with the driver.
#[derive(Debug)]
pub(crate) struct Driver {
    commands: Receiver<OverlayCommand>,
    animator: Animator,
    epoch: Instant,
}

impl Driver {
    pub(crate) fn new(commands: Receiver<OverlayCommand>) -> Self {
        Self {
            commands,
            animator: Animator::new(),
            epoch: Instant::now(),
        }
    }

    fn now(&self) -> f64 {
        self.epoch.elapsed().as_secs_f64() * 1_000.0
    }

    /// Takes in every waiting command and says what to show now.
    pub(crate) fn tick(&mut self) -> Tick {
        self.tick_at(self.now())
    }

    pub(crate) fn tick_at(&mut self, now: f64) -> Tick {
        loop {
            match self.commands.try_recv() {
                Ok(command) => self.animator.apply(command, now),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return Tick::Quit,
            }
        }
        self.animator
            .frame(now)
            .map_or(Tick::Hidden, |frame| Tick::Show(Picture::of(frame)))
    }
}

#[cfg(test)]
mod test;
