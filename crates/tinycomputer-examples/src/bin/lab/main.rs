//! The tinycomputer lab: run and score high-level flows on this desktop.
//!
//! ```text
//! scripts/lab list
//! scripts/lab run mail-compose [--mode flow|goal|authored] [--headed] [--send]
//!                              [--disable moves,undo] [--no-memory] [--flow file.json]
//!                              [--strategy narrow|wide]
//!                              [--deliberation off|standard|deep]
//! scripts/lab eval all [--modes flow,goal] [--trials 3] [--disable ...]
//! scripts/lab report target/lab-runs/<scenario>/<run>
//! scripts/lab call ListWindows '{"app": "TextEdit"}'   # probe any member
//! scripts/lab guide
//! ```
//!
//! Every run drives real applications and spends Jev (and, in `authored`
//! mode, LLM) credit. Nothing is sent, deleted, or submitted unless `--send`
//! is given, and `--send` only ever addresses `TINYCOMPUTER_LAB_SELF_EMAIL`.

use std::{
    io,
    path::{Path, PathBuf},
};

use tinycomputer_bus::{Deliberation, FLOW_GUIDE, FlowLoop, FlowStrategy};
use tinycomputer_examples::lab::{
    host::{Host, HostOptions, LabError, module_path},
    record::{RunRecord, report, scorecard},
    scenario::{SCENARIOS, find},
};

mod authored;
mod run;

use run::run_once;

const RUNS: &str = "target/lab-runs";
const MEMORY: &str = "target/lab-runs/memory.json";

#[derive(Debug, Default)]
struct Options {
    mode: String,
    modes: Vec<String>,
    trials: u32,
    headed: bool,
    send: bool,
    memory: bool,
    disabled: Vec<FlowLoop>,
    flow_file: Option<PathBuf>,
    strategy: FlowStrategy,
    deliberation: Deliberation,
}

#[tokio::main]
async fn main() -> Result<(), LabError> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let command = args.first().map_or("help", String::as_str);
    let target = args.get(1).cloned().unwrap_or_default();
    let options = parse(&args)?;
    match command {
        "list" => {
            for scenario in SCENARIOS {
                println!("{:<22} {}", scenario.name, scenario.app);
            }
            Ok(())
        }
        "guide" => {
            println!("{FLOW_GUIDE}");
            Ok(())
        }
        "report" => {
            println!("{}", report(Path::new(&target))?);
            Ok(())
        }
        "call" => {
            // `lab call <Member> '<json argument>'`: probe any member directly.
            let host = load(&options).await?;
            let argument = args
                .get(2)
                .map(|text| serde_json::from_str(text))
                .transpose()?
                .unwrap_or(serde_json::Value::Null);
            let reply = host.call(&target, argument).await?;
            println!("{}", serde_json::to_string_pretty(&reply)?);
            host.shutdown();
            Ok(())
        }
        "validate" => {
            let host = load(&options).await?;
            let flow = serde_json::from_str(&std::fs::read_to_string(&target)?)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&host.validate(&flow).await?)?
            );
            host.shutdown();
            Ok(())
        }
        "run" => {
            let scenario = find(&target).ok_or_else(|| unknown(&target))?;
            let host = load(&options).await?;
            let record = run_once(&host, scenario, &options.mode, &options).await?;
            host.shutdown();
            println!("{}", scorecard(&[record]));
            Ok(())
        }
        "eval" => {
            let scenarios = if target == "all" {
                SCENARIOS.iter().collect::<Vec<_>>()
            } else {
                target
                    .split(',')
                    .map(|name| find(name).ok_or_else(|| unknown(name)))
                    .collect::<Result<Vec<_>, _>>()?
            };
            let host = load(&options).await?;
            let mut records = Vec::new();
            for scenario in scenarios {
                for mode in &options.modes {
                    for trial in 0..options.trials {
                        println!("\n=== {} / {mode} / trial {}", scenario.name, trial + 1);
                        match run_once(&host, scenario, mode, &options).await {
                            Ok(record) => records.push(record),
                            Err(error) => {
                                println!("run failed: {error}");
                                records.push(failed_run_record(scenario.name, mode, &error));
                            }
                        }
                    }
                }
            }
            host.shutdown();
            let card = scorecard(&records);
            let path = Path::new(RUNS).join(format!(
                "scorecard-{}.md",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|elapsed| elapsed.as_secs())
                    .unwrap_or_default()
            ));
            std::fs::create_dir_all(RUNS)?;
            std::fs::write(&path, &card)?;
            println!("\n{card}\nscorecard written to {}", path.display());
            Ok(())
        }
        _ => {
            println!(
                "usage: lab list | guide | validate <flow.json> | run <scenario> [--mode flow|goal|authored] \
[--headed] [--send] [--disable a,b] [--no-memory] [--flow file] [--strategy narrow|wide] [--deliberation off|standard|deep] | eval <names|all> [--modes flow,goal] \
[--trials N] | report <run-dir>"
            );
            Ok(())
        }
    }
}

/// A scorecard row for a trial that errored before it produced its own
/// record, so `scorecard` divides by every trial rather than only the ones
/// that ran to completion.
fn failed_run_record(scenario: &str, mode: &str, error: &LabError) -> RunRecord {
    RunRecord {
        scenario: scenario.to_owned(),
        mode: mode.to_owned(),
        passed: false,
        detail: format!("run failed: {error}"),
        stop: "Error".to_owned(),
        actions: 0,
        jev_calls: 0,
        llm_calls: 0,
        seconds: 0.0,
        dir: PathBuf::new(),
    }
}

async fn load(options: &Options) -> Result<Host, LabError> {
    Host::load(
        &module_path(),
        &HostOptions {
            headed: options.headed,
            jev_model: std::env::var("TINYCOMPUTER_LAB_JEV_MODEL").ok(),
        },
    )
    .await
}

fn parse(args: &[String]) -> Result<Options, LabError> {
    let mut options = Options {
        mode: "flow".to_owned(),
        modes: vec!["flow".to_owned(), "goal".to_owned()],
        trials: 1,
        memory: true,
        ..Options::default()
    };
    let skip = if args.first().map(String::as_str) == Some("call") {
        3
    } else {
        2
    };
    let mut rest = args.iter().skip(skip);
    while let Some(flag) = rest.next() {
        let mut value = || {
            rest.next()
                .cloned()
                .ok_or_else(|| io::Error::other(format!("{flag} needs a value")))
        };
        match flag.as_str() {
            "--mode" => options.mode = value()?,
            "--modes" => options.modes = value()?.split(',').map(str::to_owned).collect(),
            "--trials" => options.trials = value()?.parse()?,
            "--headed" => options.headed = true,
            "--send" => options.send = true,
            "--no-memory" => options.memory = false,
            "--flow" => options.flow_file = Some(PathBuf::from(value()?)),
            "--strategy" => {
                options.strategy = serde_json::from_value(serde_json::Value::String(value()?))?;
            }
            "--deliberation" => {
                options.deliberation = serde_json::from_value(serde_json::Value::String(value()?))?;
            }
            "--disable" => {
                options.disabled = value()?
                    .split(',')
                    .map(|name| serde_json::from_value(serde_json::Value::String(name.to_owned())))
                    .collect::<Result<_, _>>()?;
            }
            other => return Err(io::Error::other(format!("unknown flag {other}")).into()),
        }
    }
    Ok(options)
}

/// A short id that makes this run's artifacts on screen distinguishable.
fn run_id() -> String {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or_default();
    format!("{:06}", millis % 1_000_000)
}

fn unknown(name: &str) -> LabError {
    io::Error::other(format!("no scenario named {name:?}; see `lab list`")).into()
}
