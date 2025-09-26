use crate::errors::DfrError;
use crate::planner::RoutePlanner;
use crate::router::RouterService;
use crate::types::{RoutePlan, RouterCandidate, RouterDecision, RouterInput};
use time::OffsetDateTime;

pub trait AuthzService: Send + Sync {
    fn authorize(&self, input: &RouterInput, plan: &RoutePlan) -> Result<(), DfrError>;
}

pub trait QuotaService: Send + Sync {
    fn consume(&self, input: &RouterInput, plan: &RoutePlan) -> Result<(), DfrError>;
}

pub struct DfrEngine<'a, A: AuthzService, Q: QuotaService> {
    router: &'a RouterService,
    planner: &'a RoutePlanner,
    authz: &'a A,
    quota: &'a Q,
}

impl<'a, A: AuthzService, Q: QuotaService> DfrEngine<'a, A, Q> {
    pub fn new(
        router: &'a RouterService,
        planner: &'a RoutePlanner,
        authz: &'a A,
        quota: &'a Q,
    ) -> Self {
        Self {
            router,
            planner,
            authz,
            quota,
        }
    }

    pub fn route(
        &self,
        input: RouterInput,
        candidates: Vec<RouterCandidate>,
    ) -> Result<RouterDecision, DfrError> {
        let (filter_outcome, plans) = self.router.evaluate(&input, candidates)?;
        let mut rejected = filter_outcome.rejected.clone();
        if plans.is_empty() {
            return Err(DfrError::NoRoute);
        }

        let mut sequence = 1u32;
        for plan in plans {
            match self.authz.authorize(&input, &plan) {
                Ok(_) => {}
                Err(err) => {
                    let code = match &err {
                        DfrError::AuthDenied(_) => "auth_denied",
                        _ => "auth_error",
                    };
                    rejected.push((code.into(), err.to_string()));
                    sequence = sequence.saturating_add(1);
                    continue;
                }
            }
            match self.quota.consume(&input, &plan) {
                Ok(_) => {}
                Err(err) => {
                    let code = match &err {
                        DfrError::Quota(_) => "quota_denied",
                        _ => "quota_error",
                    };
                    rejected.push((code.into(), err.to_string()));
                    sequence = sequence.saturating_add(1);
                    continue;
                }
            }

            let mut decision_path = self.planner.build_decision_path(&input, &plan, sequence);
            let mut explain = plan.explain.clone();
            explain.rejected = rejected.clone();
            let mut enriched_plan = RoutePlan { explain, ..plan };

            let issued_at = OffsetDateTime::now_utc();
            let history = self.router.history();
            let oscillation = history.record_outcome(&input, &enriched_plan, issued_at);
            crate::router::RouteHistory::annotate(&mut enriched_plan, oscillation);
            if oscillation > 0 {
                decision_path
                    .rationale
                    .thresholds_hit
                    .push(format!("route_oscillation:{}", oscillation));
            }

            return Ok(RouterDecision {
                plan: enriched_plan,
                decision_path,
                rejected,
                context_digest: input.context_digest.clone(),
                issued_at,
            });
        }

        Err(DfrError::NoRoute)
    }
}
