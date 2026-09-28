//! The debug journal: a journaled run records every exchange and what each
//! part took.

use super::*;

#[tokio::test]
async fn a_journaled_run_records_every_exchange_and_what_each_part_took() {
    let app = App::quirky(Quirk::BodyIgnoresSetValue);
    let scratch = std::env::temp_dir().join(format!(
        "tinycomputer-flow-journal-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let runtime = runtime(Oracle {
        app: app.clone(),
        hook: Box::new(|_, _, _| None),
        requests: Mutex::new(Vec::new()),
        fail: false,
    })
    .with_journal(&scratch);
    let request = RunFlowRequest {
        flow: serde_json::from_value(mail_flow()).unwrap(),
        include_values: true,
        votes: 1,
        ..RunFlowRequest::default()
    };
    let reply = super::run_flow(app, runtime, request).await;
    assert!(reply.ok, "flow run failed: {:?}", reply.error);
    let result: FlowRunResult = serde_json::from_value(reply.data.unwrap()).unwrap();

    let runs = std::fs::read_dir(&scratch)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(runs.len(), 1, "one run, one directory");
    let journal = std::fs::read_to_string(runs[0].join(crate::JOURNAL_FILE)).unwrap();
    let _ = std::fs::remove_dir_all(&scratch);
    let events = journal
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    let of = |kind: &str| {
        events
            .iter()
            .filter(|event| event["event"] == kind)
            .collect::<Vec<_>>()
    };

    assert_eq!(events[0]["event"], "run");
    assert_eq!(events[0]["kind"], "flow");
    assert_eq!(events[0]["label"], "Mail");
    let end = events.last().unwrap();
    assert_eq!(end["event"], "end");
    assert_eq!(end["stop"], serde_json::to_value(result.stop).unwrap());
    assert!(end["wall_ms"].is_u64());

    let exchanges = of("exchange");
    assert_eq!(
        exchanges.len(),
        usize::try_from(result.metrics.calls).unwrap(),
        "every Jev call is journaled"
    );
    assert!(exchanges.iter().all(|exchange| {
        exchange["ok"] == true
            && exchange["latency_ms"].is_u64()
            && exchange["request"]["questions"].is_object()
            && exchange["answers"].is_object()
    }));
    assert_eq!(exchanges[0]["step"], "2", "exchanges carry their step");
    assert_eq!(
        of("decision").len(),
        exchanges.len(),
        "one framing per decision"
    );
    assert_eq!(of("action").len(), usize::try_from(result.actions).unwrap());
    assert!(of("action").iter().all(|action| action["wall_ms"].is_u64()));
    assert!(
        of("decision")
            .iter()
            .all(|decision| decision["request_bytes"].as_u64().unwrap() > 0)
    );
    let turns = of("turn");
    assert!(!turns.is_empty(), "every do turn is journaled");
    assert!(turns.iter().all(|turn| {
        turn["step"] == "2"
            && turn["decisions"].is_u64()
            && turn["rounds"].is_u64()
            && turn["wall_ms"].is_u64()
    }));
    assert!(!of("observe").is_empty());
    let steps = of("step");
    assert_eq!(
        steps
            .iter()
            .map(|step| step["step"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["1", "2", "3", "4", "5"]
    );
    assert_eq!(steps[4]["outcome"], "gated");
    let seqs = events
        .iter()
        .map(|event| event["seq"].as_u64().unwrap())
        .collect::<Vec<_>>();
    assert!(seqs.windows(2).all(|pair| pair[1] == pair[0] + 1));
}
