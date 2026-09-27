//! One Jev decision against one observed screen.
//!
//! [`resolve_on_screen`] asks Jev for an operation and a target, reranks a
//! close call, gates the result through the deterministic policy, and
//! optionally executes it. `ResolveIntent` is this function once; `RunGoal` is
//! this function in a loop.

use std::collections::BTreeSet;

use tinydesktop_bus::{DesktopResponse, JevDecision, JevDecisionKind, JevOperation};
use tinyjevclient::EvaluationResult;

use super::{
    JevRuntime,
    backend::{AgentBackend, execute_operation, observe_async},
    invalid_response,
    policy::{
        self, action_space, choice, deterministic_destructive, exact_named_match,
        gate_with_evidence, noul, parse_operation, shortlist, target,
    },
    provider_error, reason,
    screen::{Candidate, Depth, Screen, signature},
    target_payload,
};

pub(super) struct ResolveOutcome {
    pub(super) decision: JevDecision,
    pub(super) evaluations: Vec<EvaluationResult>,
    pub(super) action_failure: Option<DesktopResponse>,
    /// The candidate the decision selected, for history and banning.
    pub(super) selected: Option<Candidate>,
}

/// What one resolution needs beyond the backend, runtime, and screen.
#[derive(Debug, Clone, Copy)]
pub(super) struct Resolution<'a> {
    pub(super) intent: &'a str,
    pub(super) text: Option<&'a str>,
    pub(super) include_values: bool,
    pub(super) execute: bool,
    pub(super) history: &'a [String],
    pub(super) allow_rerank: bool,
    /// Element signatures that already failed and must not be offered again.
    pub(super) banned: &'a BTreeSet<String>,
}

pub(super) async fn resolve<B: AgentBackend>(
    backend: &B,
    runtime: &JevRuntime,
    app: &str,
    root: Option<&str>,
    resolution: Resolution<'_>,
) -> Result<ResolveOutcome, Box<DesktopResponse>> {
    let screen = observe_async(
        backend.clone(),
        app.to_owned(),
        root.map(str::to_owned),
        Depth::Full,
    )
    .await?;
    resolve_on_screen(backend, runtime, &screen, resolution).await
}

pub(super) async fn resolve_on_screen<B: AgentBackend>(
    backend: &B,
    runtime: &JevRuntime,
    screen: &Screen,
    resolution: Resolution<'_>,
) -> Result<ResolveOutcome, Box<DesktopResponse>> {
    let Resolution {
        intent,
        text,
        include_values,
        execute,
        history,
        allow_rerank,
        banned,
    } = resolution;
    let mut offered = screen.clone();
    offered
        .candidates
        .retain(|candidate| !banned.contains(&signature(candidate)));
    let screen = &offered;
    let space = action_space(screen, text.is_some());
    let evaluation = runtime
        .client
        .evaluate(&policy::request(
            &runtime.configuration.model,
            intent,
            screen,
            &space,
            history,
            include_values,
        ))
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
    let first = target(&space, &operation_name, answers.get(&target_answer_name))
        .map(|(candidate, confidence)| (candidate.clone(), confidence));
    let (selected, evaluations) = rerank(RerankInput {
        runtime,
        intent,
        screen,
        space: &space,
        operation: &operation_name,
        target_answer: &target_answer_name,
        selected: first,
        include_values,
        first: evaluation,
        allow: allow_rerank,
    })
    .await?;
    let confidence = selected
        .as_ref()
        .map_or(operation_confidence, |(_, confidence)| *confidence);
    if deterministic_destructive(operation, selected.as_ref().map(|(candidate, _)| candidate)) {
        destructive = 1.0;
    }
    let mut decision = gate_with_evidence(
        operation,
        confidence,
        destructive,
        exact_named_match(intent, selected.as_ref().map(|(candidate, _)| candidate)),
    );
    if space.targets.contains_key(&operation_name) && selected.is_none() {
        decision = JevDecisionKind::Abstain;
    }
    if operation == JevOperation::TypeText && text.is_none() {
        decision = JevDecisionKind::NeedsText;
    }
    let mut out = JevDecision {
        decision,
        operation,
        target: selected
            .as_ref()
            .map(|(candidate, _)| target_payload(candidate)),
        confidence,
        destructive,
        reason: reason(decision, confidence, destructive),
        executed: false,
    };
    let selected = selected.map(|(node, _)| node);
    let action_failure = if execute && decision == JevDecisionKind::Act {
        let response = execute_operation(
            backend.clone(),
            screen.app.clone(),
            operation,
            selected.clone(),
            text.map(str::to_owned),
        )
        .await;
        out.executed = response.ok;
        (!response.ok).then_some(response)
    } else {
        None
    };
    Ok(ResolveOutcome {
        decision: out,
        evaluations,
        action_failure,
        selected,
    })
}

struct RerankInput<'a> {
    runtime: &'a JevRuntime,
    intent: &'a str,
    screen: &'a Screen,
    space: &'a policy::ActionSpace,
    operation: &'a str,
    target_answer: &'a str,
    selected: Option<(Candidate, f64)>,
    include_values: bool,
    first: EvaluationResult,
    allow: bool,
}

/// The selection after an optional rerank, and every evaluation it took.
type Reranked = (Option<(Candidate, f64)>, Vec<EvaluationResult>);

/// Re-asks a close call over the top five, keeping the first answer when
/// the rerank finds nothing better.
async fn rerank(input: RerankInput<'_>) -> Result<Reranked, Box<DesktopResponse>> {
    if !input.allow
        || !input
            .selected
            .as_ref()
            .is_some_and(|(_, confidence)| *confidence < policy::ACT)
    {
        return Ok((input.selected, vec![input.first]));
    }
    let candidates = shortlist(
        input.space,
        input.operation,
        input.first.response.answers.get(input.target_answer),
    );
    if candidates.len() <= 1 {
        return Ok((input.selected, vec![input.first]));
    }
    let evaluation = input
        .runtime
        .client
        .evaluate(&policy::rerank_request(
            &input.runtime.configuration.model,
            input.intent,
            input.screen,
            input.operation,
            &candidates,
            input.include_values,
        ))
        .await
        .map_err(|error| provider_error(&error))?;
    let reranked =
        choice(evaluation.response.answers.get("target")).and_then(|(choice, confidence)| {
            candidates
                .get(choice)
                .cloned()
                .map(|candidate| (candidate, confidence))
        });
    Ok((
        reranked.or(input.selected),
        vec![input.first, evaluation],
    ))
}
