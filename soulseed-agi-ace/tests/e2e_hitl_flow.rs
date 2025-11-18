/// E2E-3: HITL异步流程端到端验证测试
///
/// 验证HITL (Human-in-the-Loop) 优先级队列的完整流程：
/// 1. P0-P3四个优先级队列的正确排序
/// 2. HITL注入与队列管理
/// 3. 优先级peek和resolve机制
/// 4. 租户隔离验证

use serde_json::json;
use soulseed_agi_ace::hitl::{HitlInjection, HitlPriority, HitlQueueConfig, HitlService};
use soulseed_agi_core_models::TenantId;
use time::OffsetDateTime;
use uuid::Uuid;

#[test]
fn test_e2e_hitl_priority_ordering() {
    let config = HitlQueueConfig::default();
    let hitl = HitlService::new(config);
    let tenant = TenantId::from_raw_unchecked(101);

    // 初始队列应该为空
    assert!(hitl.is_empty());
    assert_eq!(hitl.pending_for(tenant), 0);

    // 插入不同优先级的注入，顺序混乱
    let p3_injection = HitlInjection::new(
        tenant,
        HitlPriority::P3Low,
        "user",
        json!({"message": "P3 low priority"}),
    );
    hitl.enqueue(p3_injection.clone());

    let p0_injection = HitlInjection::new(
        tenant,
        HitlPriority::P0Critical,
        "admin",
        json!({"message": "P0 critical"}),
    );
    hitl.enqueue(p0_injection.clone());

    let p1_injection = HitlInjection::new(
        tenant,
        HitlPriority::P1High,
        "supervisor",
        json!({"message": "P1 high"}),
    );
    hitl.enqueue(p1_injection.clone());

    let p2_injection = HitlInjection::new(
        tenant,
        HitlPriority::P2Medium,
        "user",
        json!({"message": "P2 medium"}),
    );
    hitl.enqueue(p2_injection.clone());

    // 验证队列计数
    assert!(!hitl.is_empty());
    assert_eq!(hitl.pending_for(tenant), 4);

    // 验证next_ready返回最高优先级（P0）
    let top_injection = hitl.next_ready();
    assert!(top_injection.is_some());
    let top = top_injection.unwrap();
    assert!(matches!(top.priority, HitlPriority::P0Critical));
    assert_eq!(top.payload["message"], "P0 critical");

    // 取出后计数应该减1
    assert_eq!(hitl.pending_for(tenant), 3);

    println!("✓ E2E Test: HITL priority ordering - P0 has highest priority");
}

#[test]
fn test_e2e_hitl_queue_operations() {
    let config = HitlQueueConfig::default();
    let hitl = HitlService::new(config);
    let tenant = TenantId::from_raw_unchecked(102);

    // 添加5个注入
    let mut injection_ids = Vec::new();
    for i in 0..5 {
        let injection = HitlInjection::new(
            tenant,
            HitlPriority::P1High,
            "user",
            json!({"index": i}),
        );
        injection_ids.push(injection.injection_id);
        hitl.enqueue(injection);
    }

    // 验证计数
    assert_eq!(hitl.pending_for(tenant), 5);

    // Resolve第一个注入
    let resolved1 = hitl.resolve_injection(injection_ids[0]);
    assert!(resolved1.is_some());
    assert_eq!(resolved1.unwrap().payload["index"], 0);

    // 计数应该减1
    assert_eq!(hitl.pending_for(tenant), 4);

    // Resolve第二个注入
    let resolved2 = hitl.resolve_injection(injection_ids[1]);
    assert!(resolved2.is_some());
    assert_eq!(resolved2.unwrap().payload["index"], 1);

    // 计数应该再减1
    assert_eq!(hitl.pending_for(tenant), 3);

    // Resolve所有剩余的
    for &id in &injection_ids[2..] {
        let resolved = hitl.resolve_injection(id);
        assert!(resolved.is_some());
    }

    // 现在应该为空
    assert_eq!(hitl.pending_for(tenant), 0);
    assert!(hitl.is_empty());

    println!("✓ E2E Test: HITL queue operations - Enqueue/resolve verified");
}

#[test]
fn test_e2e_hitl_multi_tenant_isolation() {
    let config = HitlQueueConfig::default();
    let hitl = HitlService::new(config);

    let tenant1 = TenantId::from_raw_unchecked(201);
    let tenant2 = TenantId::from_raw_unchecked(202);

    // Tenant 1的注入
    for i in 0..3 {
        let injection = HitlInjection::new(
            tenant1,
            HitlPriority::P1High,
            "user",
            json!({"tenant": 1, "index": i}),
        );
        hitl.enqueue(injection);
    }

    // Tenant 2的注入
    for i in 0..5 {
        let injection = HitlInjection::new(
            tenant2,
            HitlPriority::P1High,
            "user",
            json!({"tenant": 2, "index": i}),
        );
        hitl.enqueue(injection);
    }

    // 验证租户隔离 - 每个租户只看到自己的计数
    assert_eq!(hitl.pending_for(tenant1), 3);
    assert_eq!(hitl.pending_for(tenant2), 5);

    println!("✓ E2E Test: HITL multi-tenant isolation - Tenants properly isolated");
}

#[test]
fn test_e2e_hitl_resolve_nonexistent() {
    let config = HitlQueueConfig::default();
    let hitl = HitlService::new(config);

    // Resolve不存在的注入应该返回None
    let nonexistent = hitl.resolve_injection(Uuid::new_v4());
    assert!(nonexistent.is_none());

    println!("✓ E2E Test: HITL resolve nonexistent - Returns None correctly");
}

#[test]
fn test_e2e_hitl_empty_queue_behavior() {
    let config = HitlQueueConfig::default();
    let hitl = HitlService::new(config);
    let tenant = TenantId::from_raw_unchecked(107);

    // 空队列行为验证
    assert!(hitl.is_empty());
    assert_eq!(hitl.pending_for(tenant), 0);
    assert!(hitl.next_ready().is_none());

    // 添加一个然后resolve
    let injection = HitlInjection::new(
        tenant,
        HitlPriority::P1High,
        "user",
        json!({"test": true}),
    );
    let id = injection.injection_id;
    hitl.enqueue(injection);

    assert!(!hitl.is_empty());
    assert_eq!(hitl.pending_for(tenant), 1);

    let resolved = hitl.resolve_injection(id);
    assert!(resolved.is_some());

    // 再次查询应该为空
    assert!(hitl.is_empty());
    assert_eq!(hitl.pending_for(tenant), 0);

    println!("✓ E2E Test: HITL empty queue behavior - Empty state handled correctly");
}

#[test]
fn test_e2e_hitl_priority_consistency() {
    let config = HitlQueueConfig::default();
    let hitl = HitlService::new(config);
    let tenant = TenantId::from_raw_unchecked(108);

    // 先添加低优先级
    hitl.enqueue(HitlInjection::new(
        tenant,
        HitlPriority::P3Low,
        "user",
        json!({"p": "P3"}),
    ));

    // next_ready应该返回P3
    let p3 = hitl.next_ready().unwrap();
    assert!(matches!(p3.priority, HitlPriority::P3Low));

    // 添加高优先级
    hitl.enqueue(HitlInjection::new(
        tenant,
        HitlPriority::P0Critical,
        "admin",
        json!({"p": "P0"}),
    ));

    // 添加中等优先级
    hitl.enqueue(HitlInjection::new(
        tenant,
        HitlPriority::P2Medium,
        "user",
        json!({"p": "P2"}),
    ));

    // next_ready应该返回P0（最高优先级）
    let p0 = hitl.next_ready().unwrap();
    assert!(matches!(p0.priority, HitlPriority::P0Critical));

    // 再次取出应该是P2
    let p2 = hitl.next_ready().unwrap();
    assert!(matches!(p2.priority, HitlPriority::P2Medium));

    // 现在应该为空
    assert!(hitl.is_empty());

    println!("✓ E2E Test: HITL priority consistency - Priority always reflects highest");
}

#[test]
fn test_e2e_hitl_mixed_priorities() {
    let config = HitlQueueConfig::default();
    let hitl = HitlService::new(config);
    let tenant = TenantId::from_raw_unchecked(109);

    // 创建复杂的混合优先级场景
    // P3 x2, P2 x2, P1 x3, P0 x2
    for _ in 0..2 {
        hitl.enqueue(HitlInjection::new(
            tenant, HitlPriority::P3Low, "user", json!({"p": "P3"})));
    }
    for _ in 0..2 {
        hitl.enqueue(HitlInjection::new(
            tenant, HitlPriority::P2Medium, "user", json!({"p": "P2"})));
    }
    for _ in 0..3 {
        hitl.enqueue(HitlInjection::new(
            tenant, HitlPriority::P1High, "user", json!({"p": "P1"})));
    }
    for _ in 0..2 {
        hitl.enqueue(HitlInjection::new(
            tenant, HitlPriority::P0Critical, "admin", json!({"p": "P0"})));
    }

    // 验证总计数
    assert_eq!(hitl.pending_for(tenant), 9);

    // next_ready应该返回最高优先级P0
    let first = hitl.next_ready().unwrap();
    assert!(matches!(first.priority, HitlPriority::P0Critical));

    // 再次取出也应该是P0（因为有2个P0）
    let second = hitl.next_ready().unwrap();
    assert!(matches!(second.priority, HitlPriority::P0Critical));

    // 接下来应该是P1
    let third = hitl.next_ready().unwrap();
    assert!(matches!(third.priority, HitlPriority::P1High));

    println!("✓ E2E Test: HITL mixed priorities - Complex scenario verified");
}

#[test]
fn test_e2e_hitl_concurrent_tenants() {
    let config = HitlQueueConfig::default();
    let hitl = HitlService::new(config);

    // 创建多个租户的注入
    for tenant_id in 301..310 {
        let tenant = TenantId::from_raw_unchecked(tenant_id);
        for i in 0..3 {
            let injection = HitlInjection::new(
                tenant,
                HitlPriority::P1High,
                "user",
                json!({"tenant": tenant_id, "index": i}),
            );
            hitl.enqueue(injection);
        }
    }

    // 验证每个租户都有3个注入
    for tenant_id in 301..310 {
        let tenant = TenantId::from_raw_unchecked(tenant_id);
        assert_eq!(hitl.pending_for(tenant), 3,
            "Tenant {} should have 3 pending injections", tenant_id);
    }

    // 总队列不应该为空
    assert!(!hitl.is_empty());

    println!("✓ E2E Test: HITL concurrent tenants - Multiple tenants handled correctly");
}

#[test]
fn test_e2e_hitl_injection_timestamps() {
    let config = HitlQueueConfig::default();
    let hitl = HitlService::new(config);
    let tenant = TenantId::from_raw_unchecked(110);

    let before = OffsetDateTime::now_utc();

    // 创建注入但不设置时间戳（应该自动设置）
    let mut injection = HitlInjection::new(
        tenant,
        HitlPriority::P1High,
        "user",
        json!({"test": true}),
    );

    // 保存ID以便后续resolve
    let id = injection.injection_id;

    // 手动设置为UNIX_EPOCH以测试自动时间戳
    injection.submitted_at = OffsetDateTime::UNIX_EPOCH;

    hitl.enqueue(injection);

    let after = OffsetDateTime::now_utc();

    // Resolve并检查时间戳
    let resolved = hitl.resolve_injection(id).unwrap();

    // 时间戳应该在before和after之间（自动设置）
    assert!(resolved.submitted_at >= before);
    assert!(resolved.submitted_at <= after);
    assert_ne!(resolved.submitted_at, OffsetDateTime::UNIX_EPOCH);

    println!("✓ E2E Test: HITL injection timestamps - Auto-timestamp verified");
}
