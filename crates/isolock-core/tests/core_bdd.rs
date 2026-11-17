//! BDD scenario tests asserting the core contract using rstest-bdd.

use isolock_core::{is_valid_semver, Semver, VERSION};
use rstest::fixture;
use rstest_bdd_macros::{given, scenario, then, when};
use std::cell::RefCell;

#[derive(Default)]
struct CoreWorld {
    queried_version: RefCell<Option<&'static str>>,
    parsed: RefCell<Option<Result<Semver, &'static str>>>,
}

#[fixture]
fn world() -> CoreWorld {
    CoreWorld::default()
}

#[given("the core crate is referenced")]
fn given_core_crate_referenced(world: &CoreWorld) {
    let _ = world;
}

#[when("I query the exported VERSION constant")]
fn when_query_version_constant(world: &CoreWorld) {
    *world.queried_version.borrow_mut() = Some(VERSION);
    *world.parsed.borrow_mut() = Some(Semver::parse(VERSION));
}

#[then("the version string is not empty")]
fn then_version_not_empty(world: &CoreWorld) {
    let v = world.queried_version.borrow().expect("version was queried");
    assert!(!v.is_empty(), "VERSION must not be empty");
}

#[then("the version string conforms to semantic versioning")]
fn then_version_conforms_semver(world: &CoreWorld) {
    let res = world.parsed.borrow().expect("parsed result exists");
    assert!(
        res.is_ok(),
        "VERSION '{VERSION}' must parse as semver: {res:?}"
    );
    assert!(is_valid_semver(VERSION));
}

#[scenario(path = "features/core.feature", index = 0)]
fn bdd_core_contract(world: CoreWorld) {
    let _ = world;
}
