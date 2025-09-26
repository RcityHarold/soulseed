use std::sync::Arc;

use crate::{
    api::{AuthzRequest, AuthzResponse},
    cache::PolicyStore,
    errors::AuthzError,
    policy::Policy,
    quota::{QuotaClient, QuotaRequest},
    types::{Action, Decision, Effect, Explain, ExplainStep},
    validate::validate_anchor,
};

pub struct Evaluator {
    policies: Arc<dyn PolicyStore>,
    quota: Arc<dyn QuotaClient>,
    clock: Arc<dyn Fn() -> i64 + Send + Sync>,
}

impl Evaluator {
    pub fn new(policies: Arc<dyn PolicyStore>, quota: Arc<dyn QuotaClient>) -> Self {
        Self {
            policies,
            quota,
            clock: Arc::new(|| time::OffsetDateTime::now_utc().unix_timestamp() * 1000),
        }
    }

    pub fn with_clock(mut self, clock: Arc<dyn Fn() -> i64 + Send + Sync>) -> Self {
        self.clock = clock;
        self
    }

    pub fn authorize(&self, request: AuthzRequest) -> Result<AuthzResponse, AuthzError> {
        validate_anchor(&request.anchor)?;
        let now_ms = (self.clock)();

        let mut policies = self
            .policies
            .policies_for_tenant(request.anchor.tenant_id)
            .into_iter()
            .collect::<Vec<_>>();
        policies.sort_by(|a, b| b.priority.cmp(&a.priority));

        let mut explain = Explain::default();
        let mut matched_policies = Vec::new();
        let mut chosen_effect: Option<Effect> = None;
        let mut chosen_policy: Option<Policy> = None;
        let mut obligations: Vec<String> = Vec::new();
        let mut chosen_priority = i32::MIN;

        for policy in policies.into_iter() {
            let mut step = ExplainStep {
                policy_id: policy.id.clone(),
                matched: false,
                short_circuit: false,
                predicates: Vec::new(),
            };

            if !policy.applies_to(&request.resource, &request.action) {
                explain.chain.push(step);
                continue;
            }
            if !policy.roles_match(&request.roles) {
                explain.chain.push(step);
                continue;
            }

            let (predicates, all_true) =
                policy.evaluate_predicates(&request.anchor, &request.subject, &request.context);
            step.predicates = predicates.clone();
            step.matched = all_true;

            if all_true {
                matched_policies.push((
                    policy.id.clone(),
                    policy.policy_digest.clone(),
                    policy.priority,
                ));
                if policy.effect == Effect::Deny {
                    obligations = policy.obligations.iter().map(|o| o.as_label()).collect();
                    chosen_effect = Some(Effect::Deny);
                    chosen_policy = Some(policy.clone());
                    step.short_circuit = true;
                    explain.chain.push(step);
                    explain
                        .reasoning
                        .push("matched_policy_deny_short_circuit".into());
                    break;
                }
                if policy.priority > chosen_priority {
                    chosen_priority = policy.priority;
                    chosen_effect = Some(policy.effect.clone());
                    obligations = policy.obligations.iter().map(|o| o.as_label()).collect();
                    chosen_policy = Some(policy.clone());
                }
            }
            explain.chain.push(step);
        }

        if chosen_effect.is_none() {
            explain
                .reasoning
                .push("no_policy_matched_default_deny".into());
            let decision = Decision {
                effect: Effect::Deny,
                matched_policies,
                obligations,
                explain,
                anchor: request.anchor,
                subject: request.subject,
                resource: request.resource,
                action: request.action,
                time_ms: now_ms,
                quota: None,
            };
            return Ok(AuthzResponse { decision });
        }

        let mut effect = chosen_effect.unwrap();
        let mut obligations = obligations;
        let mut quota_payload: Option<serde_json::Value> = None;

        // Access ticket requirement for full trace
        if matches!(request.action, Action::ViewTraceFull) && request.want_trace_full {
            let policy_digest = chosen_policy
                .as_ref()
                .map(|p| p.policy_digest.clone())
                .unwrap_or_default();
            let ticket_valid = request
                .access_ticket
                .as_ref()
                .map(|ticket| {
                    ticket.tenant_id == request.anchor.tenant_id
                        && ticket.is_valid(
                            now_ms,
                            &request.subject,
                            &request.resource,
                            &request.action,
                            &policy_digest,
                        )
                })
                .unwrap_or(false);
            if !ticket_valid {
                effect = Effect::Deny;
                obligations.push("reason=TRACE_FORBIDDEN".into());
                explain
                    .reasoning
                    .push("trace_full_requires_access_ticket".into());
            }
        }

        if effect == Effect::Allow {
            if let Some(cost) = request.quota_cost.clone() {
                let quota_request = QuotaRequest {
                    tenant_id: request.anchor.tenant_id,
                    subject: request.subject.clone(),
                    resource: request.resource.clone(),
                    action: request.action.clone(),
                    cost,
                    envelope_id: request.anchor.envelope_id,
                    idem_key: request.idem_key.clone(),
                    time_ms: now_ms,
                    context: request.context.clone(),
                };
                let quota_decision = self.quota.check_and_consume(&quota_request);
                quota_payload = Some(quota_decision.detail.clone());
                match quota_decision.effect {
                    Effect::Allow => {
                        explain
                            .reasoning
                            .push(format!("quota_allow:{}", quota_decision.reason));
                    }
                    Effect::Degrade => {
                        effect = Effect::Degrade;
                        obligations.push(format!("quota:{}", quota_decision.reason));
                        explain.reasoning.push("quota_degrade".into());
                    }
                    Effect::AskConsent => {
                        effect = Effect::AskConsent;
                        explain.reasoning.push("quota_ask_consent".into());
                    }
                    Effect::Deny => {
                        effect = Effect::Deny;
                        obligations.push(format!("quota:{}", quota_decision.reason));
                        explain.reasoning.push("quota_deny".into());
                    }
                }
            }
        }

        if let Some(policy) = chosen_policy {
            if effect != Effect::Deny {
                explain
                    .reasoning
                    .push(format!("matched_policy:{}", policy.id));
            }
        }

        let decision = Decision {
            effect,
            matched_policies,
            obligations,
            explain,
            anchor: request.anchor,
            subject: request.subject,
            resource: request.resource,
            action: request.action,
            time_ms: now_ms,
            quota: quota_payload,
        };

        Ok(AuthzResponse { decision })
    }
}
