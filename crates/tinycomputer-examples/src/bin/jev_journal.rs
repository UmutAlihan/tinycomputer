//! Reads the Jev debug journal: lists journaled runs, summarises where one
//! run's time went, or prints what Jev was asked and answered.
//!
//! Turn the journal on for any run with `TINYCOMPUTER_JEV_JOURNAL=1`, then:
//!
//! ```sh
//! cargo run -p tinycomputer-examples --bin jev_journal                  # list runs
//! cargo run -p tinycomputer-examples --bin jev_journal -- latest        # summary
//! cargo run -p tinycomputer-examples --bin jev_journal -- <id> --json   # summary as JSON
//! cargo run -p tinycomputer-examples --bin jev_journal -- <id> --transcript
//! cargo run -p tinycomputer-examples --bin jev_journal -- <id> --calibration
//! ```
//!
//! `<id>` is `latest`, any unique part of a run id, or a run directory.
//! See `docs/jev-journal.md`.

use std::process::ExitCode;

use tinycomputer_examples::journal;

fn main() -> ExitCode {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let json = args.iter().any(|arg| arg == "--json");
    let view = if args.iter().any(|arg| arg == "--transcript") {
        View::Transcript
    } else if args.iter().any(|arg| arg == "--calibration") {
        View::Calibration
    } else if json {
        View::Json
    } else {
        View::Summary
    };
    let name = args.iter().find(|arg| !arg.starts_with("--"));
    let root = journal::root();
    let outcome = match name {
        None => list(&root),
        Some(name) => show(&root, name, view, json),
    };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("jev_journal: {error}");
            ExitCode::FAILURE
        }
    }
}

fn list(root: &std::path::Path) -> std::io::Result<()> {
    let runs = journal::runs(root).map_err(|error| {
        std::io::Error::new(
            error.kind(),
            format!("cannot read {}: {error}", root.display()),
        )
    })?;
    for run in runs {
        let summary = journal::summarize(&journal::events(&run)?);
        println!(
            "{:<40} {:>7.1}s {:>4} calls  {}",
            run.file_name().unwrap_or_default().to_string_lossy(),
            std::time::Duration::from_millis(summary.wall_ms).as_secs_f64(),
            summary.calls,
            summary.runs.join(" | ")
        );
    }
    Ok(())
}

/// What `show` prints about one run.
#[derive(Clone, Copy)]
enum View {
    Summary,
    Json,
    Transcript,
    Calibration,
}

fn show(root: &std::path::Path, name: &str, view: View, json: bool) -> std::io::Result<()> {
    let dir = journal::find(root, name)?;
    let events = journal::events(&dir)?;
    match view {
        View::Transcript => print!("{}", journal::transcript(&events)),
        View::Calibration if json => println!(
            "{}",
            serde_json::to_string_pretty(&journal::calibration(&events))
                .map_err(std::io::Error::other)?
        ),
        View::Calibration => print!(
            "{}",
            journal::render_calibration(&journal::calibration(&events))
        ),
        View::Json => println!(
            "{}",
            serde_json::to_string_pretty(&journal::summarize(&events))
                .map_err(std::io::Error::other)?
        ),
        View::Summary => {
            println!("{}", dir.display());
            print!("{}", journal::render(&journal::summarize(&events)));
        }
    }
    Ok(())
}
