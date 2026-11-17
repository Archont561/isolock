//! BDD scenario tests asserting airlock plan drift detection contracts.

use isolock_cas::Sha256Digest;
use isolock_core::Registry;
use isolock_plan::{check_plan_drift, AirlockPlan, PlanDrift, PlannedArtifact};
use rstest::fixture;
use rstest_bdd_macros::{given, scenario, then, when};
use std::cell::RefCell;

#[derive(Default)]
struct PlanWorld {
    expected: RefCell<AirlockPlan>,
    current: RefCell<AirlockPlan>,
    drift_result: RefCell<Option<PlanDrift>>,
}

#[fixture]
fn world() -> PlanWorld {
    PlanWorld::default()
}

#[given("an expected airlock plan with a locked package")]
fn given_expected_plan_with_package(world: &PlanWorld) {
    let digest =
        Sha256Digest::parse("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
            .unwrap();
    let mut plan = AirlockPlan::new();
    plan.add(PlannedArtifact {
        name: "polars".into(),
        version: "1.8.0".into(),
        registry: Registry::Pypi,
        digest,
    });
    *world.expected.borrow_mut() = plan;
    *world.current.borrow_mut() = AirlockPlan::new();
}

#[when("evaluated against an empty current plan")]
fn when_evaluated_against_empty(world: &PlanWorld) {
    let exp = world.expected.borrow().clone();
    let cur = world.current.borrow().clone();
    *world.drift_result.borrow_mut() = Some(check_plan_drift(&exp, &cur));
}

#[then("plan drift is detected")]
fn then_drift_detected(world: &PlanWorld) {
    let drift = world
        .drift_result
        .borrow()
        .clone()
        .expect("drift evaluated");
    assert!(!drift.is_clean());
}

#[then("the package is reported as missing")]
fn then_package_missing(world: &PlanWorld) {
    let drift = world
        .drift_result
        .borrow()
        .clone()
        .expect("drift evaluated");
    match drift {
        PlanDrift::DriftDetected { missing, .. } => {
            assert!(missing.contains(&"polars@1.8.0".to_string()));
        }
        PlanDrift::InSync => panic!("expected drift, got in sync"),
    }
}

#[scenario(path = "features/plan.feature", index = 0)]
fn bdd_plan_contract(world: PlanWorld) {
    let _ = world;
}
