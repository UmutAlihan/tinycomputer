//! `ResolveIntent`, and the one Jev decision it and every goal turn make:
//! choose an operation and a target on the observed screen, rerank a close
//! call, and gate the result before anything acts.

use tinycomputer_bus::{
    DesktopResponse, JevDecision, JevDecisionKind, JevOperation, JevTarget, ResolveIntentRequest,
    RunGoalRequest,
};
use tinyinference_decisions::EvaluationResult;

use super::backend::{AgentBackend, execute_operation, observe_async};
use super::goal::{mutates, target_allowed};
use super::policy::{
    self, action_space, choice, deterministic_destructive, exact_named_match, gate_with_evidence,
    noul, parse_operation, playing_goal_satisfied, positional_match, shortlist, target,
};
use super::reply::{invalid_response, provider_error, response};
use super::runtime::JevRuntime;
use super::screen::{Candidate, Screen};
use crate::Desktop;

/// Resolves `request.intent` to one element on the live desktop with Jev, and
/// acts on it when `request.execute` is set.
pub async fn resolve_intent(
    desktop: Desktop,
    runtime: JevRuntime,
    request: ResolveIntentRequest,
) -> DesktopResponse {
    resolve_intent_with(desktop, runtime, request).await
}

pub(super) async fn resolve_intent_with<B: AgentBackend>(
    backend: B,
    runtime: JevRuntime,
    request: ResolveIntentRequest,
) -> DesktopResponse {
    let runtime = runtime.begin_run("intent", &format!("{}: {}", request.app, request.intent));
    let result = resolve(
        &backend,
        &runtime,
        &request.intent,
        &request.app,
        request.root.as_deref(),
        request.text.as_deref(),
        request.include_values,
        request.execute,
        &[],
        true,
    )
    .await;
    match result {
        Ok(outcome) => outcome
            .action_failure
            .unwrap_or_else(|| response("resolve-intent", &outcome.decision)),
        Err(error) => *error,
    }
}

pub(super) struct ResolveOutcome {
    decision: JevDecision,
    evaluations: Vec<EvaluationResult>,
    action_failure: Option<DesktopResponse>,
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn resolve<B: AgentBackend>(
    backend: &B,
    runtime: &JevRuntime,
    intent: &str,
    app: &str,
    root: Option<&str>,
    text: Option<&str>,
    include_values: bool,
    execute: bool,
    history: &[String],
    allow_rerank: bool,
) -> Result<ResolveOutcome, Box<DesktopResponse>> {
    let screen = observe_async(
        backend.clone(),
        app.to_owned(),
        None,
        root.map(str::to_owned),
    )
    .await?;
    resolve_on_screen(
        backend,
        runtime,
        intent,
        &screen,
        text,
        include_values,
        execute,
        history,
        allow_rerank,
        None,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn resolve_on_screen<B: AgentBackend>(
    backend: &B,
    runtime: &JevRuntime,
    intent: &str,
    screen: &Screen,
    text: Option<&str>,
    include_values: bool,
    execute: bool,
    history: &[String],
    allow_rerank: bool,
    scope: Option<&RunGoalRequest>,
) -> Result<ResolveOutcome, Box<DesktopResponse>> {
    if let Some(done) = visible_completion(intent, screen) {
        return Ok(done);
    }
    let space = scoped_action_space(screen, text.is_some(), scope);
    let evaluation = runtime
        .evaluate(
            Some(intent),
            &policy::request(
                &runtime.configuration.model,
                intent,
                screen,
                &space,
                history,
                include_values,
            ),
        )
        .await
        .map_err(|error| provider_error(&error))?;
    let answers = &evaluation.response.answers;
    let (operation_name, operation_confidence) = choice(answers.get("operation"))
        .ok_or_else(|| invalid_response("operation answer was absent"))?;
    let operation_name = operation_name.to_owned();
    let operation = parse_operation(&operation_name)
        .ok_or_else(|| invalid_response("operation answer was unknown"))?;
    let mut destructive = noul(answers.get("destructive"));
    let target_answer_name = format!("{}_target", operation_name.to_ascii_lowercase());
    let mut selected = target(&space, &operation_name, answers.get(&target_answer_name))
        .map(|(candidate, confidence)| (candidate.clone(), confidence));
    let mut evaluations = vec![evaluation];
    if let Some((reranked_target, reranked)) = rerank(RerankInput {
        runtime,
        intent,
        screen,
        space: &space,
        operation: &operation_name,
        target_answer: &target_answer_name,
        selected: selected.as_ref(),
        include_values,
        first: &evaluations[0],
        allow: allow_rerank,
    })
    .await?
    {
        if let Some(reranked_target) = reranked_target {
            selected = Some(reranked_target);
        }
        evaluations.push(reranked);
    }
    let confidence = selected
        .as_ref()
        .map_or(operation_confidence, |(_, confidence)| *confidence);
    destructive = destructive.max(local_destructive_score(
        intent,
        operation,
        selected.as_ref(),
    ));
    let decision = gate_decision(&GateInput {
        intent,
        operation,
        operation_name: &operation_name,
        operation_confidence,
        confidence,
        destructive,
        selected: selected.as_ref(),
        space: &space,
        has_text: text.is_some(),
    });
    let target = selected
        .as_ref()
        .map(|(candidate, _)| target_payload(candidate));
    let mut out = JevDecision {
        decision,
        operation,
        target,
        confidence,
        destructive,
        reason: reason(decision, confidence, destructive),
        executed: false,
    };
    let (executed, action_failure) = execute_if_requested(
        backend,
        execute && decision == JevDecisionKind::Act,
        operation,
        selected.map(|(node, _)| node),
        text,
    )
    .await;
    out.executed = executed;
    Ok(ResolveOutcome {
        decision: out,
        evaluations,
        action_failure,
    })
}

pub(super) fn scoped_action_space(
    screen: &Screen,
    has_text: bool,
    scope: Option<&RunGoalRequest>,
) -> policy::ActionSpace {
    let mut space = action_space(screen, has_text);
    if let Some(scope) = scope {
        space.targets.retain(|operation, candidates| {
            let Some(parsed) = parse_operation(operation) else {
                return false;
            };
            if mutates(parsed)
                && !scope.allowed_operations.is_empty()
                && !scope.allowed_operations.contains(&parsed)
            {
                return false;
            }
            candidates.retain(|_, candidate| target_allowed(scope, candidate));
            !candidates.is_empty()
        });
    }
    space
}

pub(super) struct GateInput<'a> {
    intent: &'a str,
    operation: JevOperation,
    operation_name: &'a str,
    operation_confidence: f64,
    confidence: f64,
    destructive: f64,
    selected: Option<&'a (Candidate, f64)>,
    space: &'a policy::ActionSpace,
    has_text: bool,
}

pub(super) fn gate_decision(input: &GateInput<'_>) -> JevDecisionKind {
    let candidate = input.selected.map(|(candidate, _)| candidate);
    let mut decision = gate_with_evidence(
        input.operation,
        input.confidence,
        input.destructive,
        exact_named_match(input.intent, candidate)
            || positional_match(
                input.intent,
                candidate,
                input.space.targets.get(input.operation_name),
            ),
    );
    if mutates(input.operation) && input.operation_confidence < policy::ACT {
        decision = JevDecisionKind::Abstain;
    }
    if input.space.targets.contains_key(input.operation_name) && input.selected.is_none() {
        decision = JevDecisionKind::Abstain;
    }
    if input.operation == JevOperation::TypeText && !input.has_text {
        decision = JevDecisionKind::NeedsText;
    }
    decision
}

pub(super) fn local_destructive_score(
    intent: &str,
    operation: JevOperation,
    selected: Option<&(Candidate, f64)>,
) -> f64 {
    if deterministic_destructive(intent, operation, selected.map(|(candidate, _)| candidate)) {
        1.0
    } else {
        0.0
    }
}

pub(super) async fn execute_if_requested<B: AgentBackend>(
    backend: &B,
    execute: bool,
    operation: JevOperation,
    target: Option<Candidate>,
    text: Option<&str>,
) -> (bool, Option<DesktopResponse>) {
    if !execute {
        return (false, None);
    }
    let response =
        execute_operation(backend.clone(), operation, target, text.map(str::to_owned)).await;
    if response.ok {
        (true, None)
    } else {
        (false, Some(response))
    }
}

pub(super) fn visible_completion(intent: &str, screen: &Screen) -> Option<ResolveOutcome> {
    playing_goal_satisfied(intent, screen).then(|| ResolveOutcome {
        decision: JevDecision {
            decision: JevDecisionKind::Done,
            operation: JevOperation::Done,
            target: None,
            confidence: 1.0,
            destructive: 0.0,
            reason: "the requested playback state is visibly satisfied".to_owned(),
            executed: false,
        },
        evaluations: Vec::new(),
        action_failure: None,
    })
}

pub(super) struct RerankInput<'a> {
    runtime: &'a JevRuntime,
    intent: &'a str,
    screen: &'a Screen,
    space: &'a policy::ActionSpace,
    operation: &'a str,
    target_answer: &'a str,
    selected: Option<&'a (Candidate, f64)>,
    include_values: bool,
    first: &'a EvaluationResult,
    allow: bool,
}

pub(super) async fn rerank(
    input: RerankInput<'_>,
) -> Result<Option<(Option<(Candidate, f64)>, EvaluationResult)>, Box<DesktopResponse>> {
    if !input.allow
        || !input
            .selected
            .is_some_and(|(_, confidence)| *confidence < policy::ACT)
    {
        return Ok(None);
    }
    let candidates = shortlist(
        input.space,
        input.operation,
        input.first.response.answers.get(input.target_answer),
    );
    if candidates.len() <= 1 {
        return Ok(None);
    }
    let evaluation = input
        .runtime
        .evaluate(
            Some(input.intent),
            &policy::rerank_request(
                &input.runtime.configuration.model,
                input.intent,
                input.screen,
                input.operation,
                &candidates,
                input.include_values,
            ),
        )
        .await
        .map_err(|error| provider_error(&error))?;
    let selected =
        choice(evaluation.response.answers.get("target")).and_then(|(choice, confidence)| {
            candidates
                .get(choice)
                .cloned()
                .map(|candidate| (candidate, confidence))
        });
    Ok(Some((selected, evaluation)))
}

pub(super) fn target_payload(candidate: &Candidate) -> JevTarget {
    JevTarget {
        ref_id: candidate.ref_id.clone(),
        role: candidate.role.clone(),
        name: candidate.label().map(str::to_owned),
    }
}

pub(super) fn reason(decision: JevDecisionKind, confidence: f64, destructive: f64) -> String {
    match decision {
        JevDecisionKind::Act => "the target cleared the safe-action threshold".to_owned(),
        JevDecisionKind::ConfirmationRequired => {
            format!("the action is hard to undo ({destructive:.2})")
        }
        JevDecisionKind::Abstain => format!("target confidence {confidence:.2} is too low"),
        JevDecisionKind::NeedsText => {
            "the selected operation needs caller-supplied text".to_owned()
        }
        JevDecisionKind::Done => "the goal is visibly satisfied".to_owned(),
        JevDecisionKind::Blocked => "no offered operation can advance the goal".to_owned(),
    }
}
