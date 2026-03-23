//! Phase 4 hardening: tenant quota + MU exhaustion tests.

mod common;

use mesh_hub::config::PolicyConfig;
use mesh_hub::policy::PolicyEngine;
use mesh_hub::tenant::MuError;

use common::open_temp_tenant_manager;

/// Create a tenant with max_descriptors=10, store 10 descriptors worth of quota,
/// then verify the 11th is rejected by PolicyEngine::check_quotas.
#[test]
fn test_quota_enforcement_at_limit() {
    let engine = PolicyEngine::new(PolicyConfig::default());
    let dir = tempfile::tempdir().unwrap();
    let tm = open_temp_tenant_manager(dir.path());

    let tenant = tm.create_tenant("quota-test", "free").unwrap();
    tm.update_quotas(&tenant.id, Some(10), None, None).unwrap();
    let mut tenant = tm.get_tenant(&tenant.id).unwrap().unwrap();

    // 9/10 — under limit
    tenant.current_descriptors = 9;
    assert!(
        engine.check_quotas(&tenant, 1024).is_ok(),
        "9/10 descriptors should pass quota check"
    );

    // 10/10 — at limit
    tenant.current_descriptors = 10;
    assert!(
        engine.check_quotas(&tenant, 1024).is_err(),
        "10/10 descriptors should fail quota check"
    );

    // 11/10 — over limit
    tenant.current_descriptors = 11;
    assert!(
        engine.check_quotas(&tenant, 1024).is_err(),
        "11/10 descriptors should fail quota check"
    );
}

/// Test MU exhaustion: deduct most of the balance, then fail on over-deduction,
/// and verify the balance is unchanged after the failure.
#[test]
fn test_mu_exhaustion() {
    let dir = tempfile::tempdir().unwrap();
    let tm = open_temp_tenant_manager(dir.path());

    let tenant = tm.create_tenant("mu-test", "free").unwrap();
    // Free tier starts with mu_balance=10_000; set custom limit and drain to 100.
    tm.update_quotas(&tenant.id, None, None, Some(100)).unwrap();
    tm.deduct_mu(&tenant.id, 9_900).unwrap();

    let usage = tm.get_usage(&tenant.id).unwrap();
    assert_eq!(usage.mu_balance, 100);

    // Deduct 90 MU → balance=10
    tm.deduct_mu(&tenant.id, 90).unwrap();
    let usage = tm.get_usage(&tenant.id).unwrap();
    assert_eq!(usage.mu_balance, 10);

    // Deduct 20 MU → insufficient
    let result = tm.deduct_mu(&tenant.id, 20);
    assert!(result.is_err());
    match result.unwrap_err() {
        MuError::InsufficientBalance { balance, cost } => {
            assert_eq!(balance, 10);
            assert_eq!(cost, 20);
        }
        e => panic!("expected InsufficientBalance, got: {e}"),
    }

    // Balance unchanged after failed deduction
    let usage = tm.get_usage(&tenant.id).unwrap();
    assert_eq!(
        usage.mu_balance, 10,
        "balance should be unchanged after failed deduction"
    );
}

/// Verify each tier produces correct quota limits from tier_quotas.
#[test]
fn test_tier_quota_progression() {
    let dir = tempfile::tempdir().unwrap();
    let tm = open_temp_tenant_manager(dir.path());

    // Free tier (matches tier_quotas("free") in tenant.rs)
    let free = tm.create_tenant("free-org", "free").unwrap();
    assert_eq!(free.max_descriptors, 100);
    assert_eq!(free.max_storage_bytes, 1_048_576);
    assert_eq!(free.max_query_rate, 10);
    assert_eq!(free.max_store_rate, 1);
    assert_eq!(free.mu_limit, 10_000);
    assert_eq!(free.mu_balance, 10_000);

    // Starter tier (matches tier_quotas("starter") in tenant.rs)
    let starter = tm.create_tenant("starter-org", "starter").unwrap();
    assert_eq!(starter.max_descriptors, 1_000);
    assert_eq!(starter.max_storage_bytes, 10_485_760);
    assert_eq!(starter.max_query_rate, 50);
    assert_eq!(starter.max_store_rate, 5);
    assert_eq!(starter.mu_limit, 100_000);
    assert_eq!(starter.mu_balance, 100_000);

    // Pro tier (matches tier_quotas("pro") in tenant.rs)
    let pro = tm.create_tenant("pro-org", "pro").unwrap();
    assert_eq!(pro.max_descriptors, 10_000);
    assert_eq!(pro.max_storage_bytes, 104_857_600);
    assert_eq!(pro.max_query_rate, 100);
    assert_eq!(pro.max_store_rate, 10);
    assert_eq!(pro.mu_limit, 1_000_000);
    assert_eq!(pro.mu_balance, 1_000_000);

    // Enterprise tier (matches tier_quotas("enterprise") in tenant.rs)
    let enterprise = tm.create_tenant("enterprise-org", "enterprise").unwrap();
    assert_eq!(enterprise.max_descriptors, 1_000_000);
    assert_eq!(enterprise.max_storage_bytes, 10_737_418_240);
    assert_eq!(enterprise.max_query_rate, 1000);
    assert_eq!(enterprise.max_store_rate, 100);
    assert_eq!(enterprise.mu_limit, 10_000_000);
    assert_eq!(enterprise.mu_balance, 10_000_000);

    // Verify strict monotonic progression across tiers
    assert!(starter.max_descriptors > free.max_descriptors);
    assert!(pro.max_descriptors > starter.max_descriptors);
    assert!(enterprise.max_descriptors > pro.max_descriptors);

    assert!(starter.mu_limit > free.mu_limit);
    assert!(pro.mu_limit > starter.mu_limit);
    assert!(enterprise.mu_limit > pro.mu_limit);
}
