#![allow(clippy::unwrap_used, clippy::expect_used)] // Test fixtures fail fast on invalid setup.
//! BDD scenario tests asserting proxy routing contracts.

use isolock_core::AirlockMode;
use isolock_proxy::{evaluate_proxy_request, ProxyDecision};
use rstest::fixture;
use rstest_bdd_macros::{given, scenario, then, when};
use std::cell::RefCell;

#[derive(Default)]
struct ProxyWorld {
    mode: RefCell<AirlockMode>,
    decision: RefCell<Option<ProxyDecision>>,
}

#[fixture]
fn world() -> ProxyWorld {
    ProxyWorld::default()
}

#[given("a proxy running in LocalAirlock mode")]
fn given_local_airlock_mode(world: &ProxyWorld) {
    *world.mode.borrow_mut() = AirlockMode::LocalAirlock;
}

#[when("a requested artifact is not present in CAS")]
fn when_artifact_not_cached(world: &ProxyWorld) {
    let mode = *world.mode.borrow();
    *world.decision.borrow_mut() = Some(evaluate_proxy_request(mode, false));
}

#[then("the proxy issues a fail closed airlock violation")]
fn then_proxy_fails_closed(world: &ProxyWorld) {
    let decision = world.decision.borrow().expect("decision computed");
    assert_eq!(decision, ProxyDecision::FailClosedAirlockViolation);
}

#[scenario(path = "features/proxy.feature", index = 0)]
fn bdd_proxy_contract(world: ProxyWorld) {
    let _ = world;
}
