//! Tours the agent's cursor around the screen, so its look and motion can be
//! judged without running an agent.
//!
//! Build the overlay helper first — the demo finds it next to itself — then
//! run the demo, optionally naming a pace (`brisk`, `natural`, `calm`) and
//! how many laps to make:
//!
//! ```sh
//! cargo build -p tinydesktop-cursor --features overlay
//! cargo run -p tinydesktop-examples --bin cursor_demo -- calm 3
//! ```
//!
//! The cursor visits a dozen made-up targets — buttons, fields, menu items,
//! of different sizes and distances — pausing on each the way a run pauses
//! while an action lands. Nothing is clicked: the cursor is only drawn.

use std::time::Duration;

use tinydesktop_cursor::{CursorPace, ProcessOverlay, Rect, ScreenCursor};

/// Made-up targets across a 1280 × 800 area near the top-left of the main
/// display, from tiny to wide, near and far.
const TOUR: [(&str, Rect); 12] = [
    ("Compose", Rect::new(180.0, 160.0, 96.0, 32.0)),
    ("To field", Rect::new(420.0, 240.0, 520.0, 28.0)),
    ("Subject field", Rect::new(420.0, 290.0, 520.0, 28.0)),
    ("Attach", Rect::new(1180.0, 180.0, 28.0, 28.0)),
    ("Bold", Rect::new(430.0, 350.0, 22.0, 22.0)),
    ("Body", Rect::new(420.0, 400.0, 700.0, 300.0)),
    ("Send", Rect::new(1040.0, 740.0, 90.0, 34.0)),
    ("Search box", Rect::new(640.0, 120.0, 360.0, 30.0)),
    ("Result 3", Rect::new(260.0, 520.0, 300.0, 44.0)),
    ("Menu: File", Rect::new(200.0, 110.0, 40.0, 20.0)),
    ("Menu item: Export…", Rect::new(210.0, 250.0, 180.0, 22.0)),
    ("Close", Rect::new(1300.0, 130.0, 16.0, 16.0)),
];

/// How long the cursor rests on each target after landing.
const REST: Duration = Duration::from_millis(900);

fn main() {
    let mut arguments = std::env::args().skip(1);
    let pace = arguments
        .next()
        .map_or(Ok(CursorPace::Calm), |name| name.parse::<CursorPace>());
    let pace = match pace {
        Ok(pace) if !pace.is_off() => pace,
        Ok(_) => return eprintln!("`off` draws nothing; pick brisk, natural, or calm"),
        Err(error) => return eprintln!("{error}"),
    };
    let laps: u32 = arguments
        .next()
        .and_then(|laps| laps.parse().ok())
        .unwrap_or(3);

    let Some(helper) = ProcessOverlay::locate() else {
        return eprintln!(
            "the overlay helper was not found; build it first with\n  \
             cargo build -p tinydesktop-cursor --features overlay"
        );
    };
    println!(
        "drawing with {} at the {pace} pace, {laps} laps",
        helper.display()
    );

    let cursor = ScreenCursor::new(pace, Some(helper));
    for lap in 1..=laps {
        for (name, target) in TOUR {
            println!("lap {lap}: {name}");
            // The cursor never waits for itself; the demo does, so each
            // glide lands before the next begins.
            let glide = cursor.show(target).unwrap_or_default();
            std::thread::sleep(glide + REST);
        }
    }
    cursor.hide();
    std::thread::sleep(Duration::from_millis(400));
}
