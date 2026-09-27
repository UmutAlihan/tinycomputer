//! Tests that keep the skill in step with the contract it describes.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tinydesktop_bus::agent::{StartTaskRequest, names};

use super::{SKILL, START_TASK_SCHEMA, skill_assets};

#[test]
fn the_schema_names_exactly_the_members_the_contract_serves() {
    let schema: serde_json::Value = serde_json::from_str(START_TASK_SCHEMA).unwrap();
    let members = schema["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|member| member.as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(members, names::METHODS);
    let properties = schema["properties"].as_object().unwrap();
    let request = serde_json::to_value(StartTaskRequest::default()).unwrap();
    for field in request.as_object().unwrap().keys() {
        if field != "memory" {
            assert!(properties.contains_key(field), "{field} is undocumented");
        }
    }
}

#[test]
fn the_skill_covers_every_member_status_and_the_payment_rule() {
    for member in names::METHODS {
        if !["PlanTask", "CancelTask", "ListTasks"].contains(member) {
            assert!(SKILL.contains(member), "{member} is not explained");
        }
    }
    for status in [
        "running",
        "needs_input",
        "needs_approval",
        "needs_human",
        "checkpoint",
        "needs_plan",
        "done",
        "failed",
    ] {
        assert!(
            SKILL.contains(&format!("`{status}`")),
            "{status} is not explained"
        );
    }
    assert!(SKILL.contains("Never pay on the person's behalf"));
    assert!(SKILL.contains("Secrets stay templates"));
    assert!(SKILL.starts_with("---\nname: tinydesktop\n"));
}

#[test]
fn the_example_in_the_skill_is_a_request_the_module_accepts() {
    let start = SKILL.find("```json\n").unwrap() + 8;
    let end = start + SKILL[start..].find("```").unwrap();
    let call: serde_json::Value = serde_json::from_str(&SKILL[start..end]).unwrap();
    assert_eq!(call["member"], "StartTask");
    let request: StartTaskRequest = serde_json::from_value(call["args"][0].clone()).unwrap();
    assert!(request.flow.is_some());
    assert!(
        skill_assets()
            .iter()
            .all(|asset| !asset.contents.is_empty())
    );
}
