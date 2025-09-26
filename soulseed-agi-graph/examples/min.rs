use soulseed_agi_graph::{
    api::{RecallFilters, RecallQuery, RecallQueryTextOrVec, TimelineQuery},
    mock::{Capabilities, MockExecutor},
    plan::{PlanComponent, PlannerConfig},
    planner::Planner,
    SessionId, TenantId,
};
use std::collections::HashSet;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let planner = Planner::new(PlannerConfig::default());
    let executor = MockExecutor::new(Capabilities {
        allowed_tenants: [1u64].into_iter().collect::<HashSet<_>>(),
        vector_idx_available: true,
        live_max_per_tenant: 10,
    });

    let timeline_query = TimelineQuery {
        tenant_id: TenantId::new(1),
        session_id: Some(SessionId::new(100)),
        participants: None,
        window: None,
        after: None,
        limit: 20,
        require_fields: None,
        scenario: None,
        event_types: None,
    };
    let (plan, hash, degr) = planner.plan_timeline(&timeline_query)?;
    println!("timeline plan hash={}, degradation={:?}", hash, degr);
    let response = executor.execute_timeline(plan, &timeline_query)?;
    println!("timeline response indices={:?}", response.indices_used);

    let recall_query = RecallQuery {
        tenant_id: TenantId::new(1),
        query: RecallQueryTextOrVec::Text("寻找旅行建议".into()),
        k: 10,
        filters: RecallFilters {
            scenes: None,
            topics: None,
            time_window: None,
            participant: None,
            event_types: None,
        },
    };
    let (recall_plan, recall_hash, recall_degr) = planner.plan_recall(&recall_query, true, 768)?;
    println!(
        "recall plan hash={}, degradation={:?}",
        recall_hash, recall_degr
    );
    let recall_resp = executor.execute_recall(recall_plan, &recall_query, recall_degr)?;
    println!("recall response indices={:?}", recall_resp.indices_used);

    let hybrid_plan = planner.plan_hybrid(
        vec![
            PlanComponent {
                name: "vec".into(),
                weight: 0.6,
            },
            PlanComponent {
                name: "sparse".into(),
                weight: 0.3,
            },
            PlanComponent {
                name: "causal".into(),
                weight: 0.1,
            },
        ],
        20,
    );
    println!("hybrid plan indices={:?}", hybrid_plan.indices_used);

    Ok(())
}
