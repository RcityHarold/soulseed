use crate::errors::DfrError;
use crate::planner::RoutePlanner;
use crate::router::{RouteHistory, RouterEvaluation, RouterService};
use crate::types::{RouterCandidate, RouterDecision, RouterInput};
use time::OffsetDateTime;

pub struct DfrEngine<'a> {
    router: &'a RouterService,
    planner: &'a RoutePlanner,
}

impl<'a> DfrEngine<'a> {
    pub fn new(router: &'a RouterService, planner: &'a RoutePlanner) -> Self {
        Self { router, planner }
    }

    pub fn route(
        &self,
        input: RouterInput,
        candidates: Vec<RouterCandidate>,
    ) -> Result<RouterDecision, DfrError> {
        let evaluation: RouterEvaluation = self.router.evaluate(&input, candidates)?;
        if evaluation.plans.is_empty() {
            return Err(DfrError::NoRoute);
        }

        let mut plan = evaluation
            .plans
            .into_iter()
            .next()
            .expect("non-empty plans");
        let mut rejected = evaluation.rejected.clone();
        plan.explain.rejected = rejected.clone();

        let issued_at = OffsetDateTime::now_utc();
        let history = self.router.history();
        let oscillation = history.record_outcome(&input, &plan, issued_at);
        RouteHistory::annotate(&mut plan, oscillation);

        let mut decision_path =
            self.planner
                .build_decision_path(&input, &plan, 1, &evaluation.fork_scores);
        if oscillation > 0 {
            decision_path
                .rationale
                .thresholds_hit
                .push(format!("route_oscillation:{}", oscillation));
        }

        Ok(RouterDecision {
            plan,
            decision_path,
            rejected,
            context_digest: input.context_digest.clone(),
            issued_at,
        })
    }
}
