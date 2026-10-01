//! Unit tests for reading the Jev journal back.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use serde_json::{Value, json};

use super::{
    calibration, events, find, percentile, render, render_calibration, runs, summarize, transcript,
};

fn flow_events() -> Vec<Value> {
    vec![
        json!({"event": "run", "seq": 0, "elapsed_ms": 0, "kind": "flow", "label": "Mail"}),
        json!({"event": "action", "seq": 1, "elapsed_ms": 40, "step": "", "action": "launch", "wall_ms": 30, "settle_ms": 10}),
        json!({"event": "observe", "seq": 2, "elapsed_ms": 60, "step": "1", "wall_ms": 20}),
        json!({"event": "exchange", "seq": 3, "elapsed_ms": 400, "step": "1", "ok": true,
        "latency_ms": 300, "request_bytes": 1000, "input_tokens": 50, "output_tokens": 2,
        "questions": ["done", "move"],
        "answers": {
            "done": {"type": "noul", "noul": 0.2},
            "move": {"type": "choice", "choice": "shortcut", "probabilities": {"shortcut": 0.9}, "confidence": 0.8},
            "progress": {"type": "score", "score": 1.5, "legend": {}, "probabilities": {}, "confidence": 0.5}
        }}),
        json!({"event": "decision", "seq": 4, "elapsed_ms": 410, "step": "1", "wall_ms": 320}),
        json!({"event": "exchange", "seq": 5, "elapsed_ms": 900, "step": "1", "ok": false,
               "latency_ms": 500, "request_bytes": 3000, "questions": ["done"], "error": "timeout"}),
        json!({"event": "decision", "seq": 6, "elapsed_ms": 910, "step": "1", "wall_ms": 500}),
        json!({"event": "action", "seq": 7, "elapsed_ms": 1000, "step": "1", "action": "press cmd+n", "wall_ms": 50, "settle_ms": 40}),
        json!({"event": "turn", "seq": 71, "elapsed_ms": 1001, "step": "1", "turn": 0, "decisions": 2, "wall_ms": 900}),
        json!({"event": "turn", "seq": 72, "elapsed_ms": 1002, "step": "1", "turn": 1, "decisions": 1, "wall_ms": 50}),
        json!({"event": "step", "seq": 8, "elapsed_ms": 1010, "step": "1", "kind": "do", "text": "start a new message",
               "outcome": "done", "note": "the step is complete", "wall_ms": 950}),
        json!({"event": "end", "seq": 9, "elapsed_ms": 1020, "stop": "completed", "wall_ms": 1200}),
    ]
}

#[test]
fn a_summary_splits_wall_time_by_where_it_went() {
    let summary = summarize(&flow_events());
    assert_eq!(summary.runs, ["flow: Mail"]);
    assert_eq!(summary.wall_ms, 1200, "the end event's wall time wins");
    assert_eq!(
        summary.jev_ms, 820,
        "decisions, not calls, are what a flow waited on"
    );
    assert_eq!(summary.observe_ms, 20);
    assert_eq!(summary.act_ms, 80);
    assert_eq!(summary.settle_ms, 50);
    assert_eq!(
        (summary.calls, summary.failed_calls, summary.decisions),
        (2, 1, 2)
    );
    assert_eq!((summary.observations, summary.actions), (1, 2));
    assert_eq!(summary.latency_p50_ms, 300);
    assert_eq!(summary.latency_max_ms, 500);
    assert_eq!(summary.mean_request_bytes, 2000);
    assert_eq!((summary.input_tokens, summary.output_tokens), (50, 2));
    assert_eq!(
        (summary.max_input_tokens, summary.max_window_percent),
        (50, 0)
    );
    assert_eq!(
        (
            summary.turns,
            summary.max_turn_decisions,
            summary.mean_turn_decisions_x100
        ),
        (2, 2, 150)
    );

    assert_eq!(summary.steps.len(), 1, "the launch has no step row");
    let step = &summary.steps[0];
    assert_eq!(step.step, "1");
    assert_eq!(step.text, "do start a new message");
    assert_eq!(step.outcome, "done");
    assert_eq!(
        (
            step.wall_ms,
            step.jev_ms,
            step.observe_ms,
            step.act_ms,
            step.calls
        ),
        (950, 820, 20, 90, 2)
    );

    assert_eq!(summary.slowest[0].seq, 5);
    assert_eq!(summary.slowest[0].questions, ["done"]);
}

#[test]
fn a_run_without_decisions_counts_call_latency_as_jev_time() {
    let summary = summarize(&[
        json!({"event": "run", "kind": "goal", "label": "Calculator: add"}),
        json!({"event": "exchange", "elapsed_ms": 200, "ok": true, "latency_ms": 150}),
        json!({"event": "exchange", "elapsed_ms": 500, "ok": true, "latency_ms": 250}),
    ]);
    assert_eq!(summary.jev_ms, 400);
    assert_eq!(summary.wall_ms, 500);
    assert_eq!(summary.steps, Vec::new());
    assert_eq!(summary.slowest[0].step, "");
}

#[test]
fn an_empty_journal_summarises_to_zeroes() {
    let summary = summarize(&[]);
    assert_eq!(summary, super::Summary::default());
    let text = render(&summary);
    assert!(text.contains("wall"));
    assert!(!text.contains("slowest"));
}

#[test]
fn the_rendered_summary_shows_shares_steps_and_slow_calls() {
    let text = render(&summarize(&flow_events()));
    assert!(text.contains("run      flow: Mail"), "{text}");
    assert!(text.contains("jev         0.8s  68%"), "{text}");
    assert!(text.contains("2 decisions, 2 calls (1 failed)"), "{text}");
    assert!(text.contains("do start a new message"), "{text}");
    assert!(text.contains("slowest Jev calls"), "{text}");
    assert!(text.contains("#5"), "{text}");
    assert!(
        text.contains("turns    2 do turns; decisions in sequence per turn: mean 1.50, most 2"),
        "{text}"
    );
    assert!(
        text.contains("window   largest call 50 tokens, 0% of Jev's 32 K"),
        "{text}"
    );
}

#[test]
fn the_transcript_shows_each_answer_compactly() {
    let text = transcript(&flow_events());
    assert!(text.contains("== flow: Mail"), "{text}");
    assert!(text.contains("#3 step 1 · 300 ms · 1000 B"), "{text}");
    assert!(text.contains("done         yes 0.20"), "{text}");
    assert!(text.contains("move         -> shortcut (0.90)"), "{text}");
    assert!(text.contains("progress     score 1.50"), "{text}");
    assert!(text.contains("failed: timeout"), "{text}");
    assert!(
        text.contains("-- step 1 do \"start a new message\": done"),
        "{text}"
    );
}

#[test]
fn percentiles_use_the_nearest_rank() {
    assert_eq!(percentile(&[], 50), 0);
    assert_eq!(percentile(&[7], 90), 7);
    assert_eq!(percentile(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10], 90), 9);
    assert_eq!(percentile(&[1, 2, 3, 4], 50), 2);
}

#[test]
fn runs_are_found_by_latest_by_part_of_their_id_or_by_path() {
    let root = std::env::temp_dir().join(format!(
        "tinycomputer-journal-reader-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    for id in [
        "20260101T000000Z-flow-aaaaaa",
        "20260102T000000Z-goal-bbbbbb",
    ] {
        std::fs::create_dir_all(root.join(id)).unwrap();
        std::fs::write(
            root.join(id).join(tinycomputer_engine::JOURNAL_FILE),
            "{\"event\":\"run\",\"kind\":\"flow\"}\nnot json\n",
        )
        .unwrap();
    }
    std::fs::create_dir_all(root.join("empty")).unwrap();

    let listed = runs(&root).unwrap();
    assert_eq!(
        listed.len(),
        2,
        "a directory without a journal is not a run"
    );
    assert!(
        find(&root, "latest")
            .unwrap()
            .ends_with("20260102T000000Z-goal-bbbbbb")
    );
    assert!(
        find(&root, "aaaa")
            .unwrap()
            .ends_with("20260101T000000Z-flow-aaaaaa")
    );
    let by_path = root.join("20260101T000000Z-flow-aaaaaa");
    assert_eq!(find(&root, by_path.to_str().unwrap()).unwrap(), by_path);
    assert!(find(&root, "2026").is_err(), "an ambiguous name is refused");
    assert!(find(&root, "missing").is_err());
    assert_eq!(events(&by_path).unwrap().len(), 1, "a torn line is skipped");

    std::fs::remove_dir_all(&root).unwrap();
    assert!(runs(&root).is_err());
    assert!(find(&PathBuf::from("/nonexistent-journal-root"), "latest").is_err());
}

#[test]
fn a_strategy_is_parsed_by_its_wire_name() {
    assert_eq!(
        crate::parse_strategy(" wide "),
        Some(tinycomputer_bus::FlowStrategy::Wide)
    );
    assert_eq!(
        crate::parse_strategy("narrow"),
        Some(tinycomputer_bus::FlowStrategy::Narrow)
    );
    assert_eq!(crate::parse_strategy("fastest"), None);
}

#[test]
fn calibration_tallies_verdicts_against_how_steps_ended() {
    let events = vec![
        json!({"event": "evidence", "step": "1", "site": "target", "p": 0.9, "agreement": 1.0, "verdict": "accept"}),
        json!({"event": "evidence", "step": "2", "site": "target", "p": 0.5, "agreement": 0.6, "verdict": "deliberate"}),
        json!({"event": "evidence", "step": "2", "site": "target", "p": 0.7, "agreement": 0.8, "verdict": "deliberate"}),
        json!({"event": "escalate", "step": "2", "site": "target", "rung": "framings"}),
        json!({"event": "duel", "step": "2", "champion": 1}),
        json!({"event": "duel", "step": "2", "champion": null}),
        json!({"event": "expect", "step": "2", "outcome": "missed: it left the page"}),
        json!({"event": "expect", "step": "1", "outcome": "met"}),
        json!({"event": "restore", "step": "2", "restored": true}),
        json!({"event": "backtrack", "step": "2", "accepted": false}),
        json!({"event": "step", "step": "1", "outcome": "done"}),
        json!({"event": "step", "step": "2", "outcome": "failed"}),
    ];
    let tally = calibration(&events);
    assert_eq!(tally.verdicts.len(), 2);
    let accepted = &tally.verdicts[0];
    assert_eq!(
        (
            accepted.verdict.as_str(),
            accepted.count,
            accepted.step_done
        ),
        ("accept", 1, 1)
    );
    let deliberated = &tally.verdicts[1];
    assert_eq!((deliberated.count, deliberated.step_failed), (2, 2));
    assert!((deliberated.mean_p - 0.6).abs() < 1e-9);
    assert!((deliberated.mean_agreement - 0.7).abs() < 1e-9);
    assert_eq!(tally.rungs["framings"], 1);
    assert_eq!((tally.duels, tally.champions), (2, 1));
    assert_eq!(
        (tally.expectations["missed"], tally.expectations["met"]),
        (1, 1)
    );
    assert_eq!((tally.restores, tally.restored), (1, 1));
    assert_eq!((tally.backtracks, tally.branched), (1, 0));
    let table = render_calibration(&tally);
    assert!(table.contains("deliberate"));
    assert!(table.contains("duels: 2 (1 with a champion)"));
    assert!(render_calibration(&calibration(&[])).contains("undos: 0"));
}
