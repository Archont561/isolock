//! BDD scenario tests asserting CAS storage contracts.

use isolock_cas::{InMemoryCas, Sha256Digest};
use rstest::fixture;
use rstest_bdd_macros::{given, scenario, then, when};
use std::cell::RefCell;

#[derive(Default)]
struct CasWorld {
    store: RefCell<InMemoryCas>,
    last_digest: RefCell<Option<Sha256Digest>>,
    last_payload: RefCell<Vec<u8>>,
}

#[fixture]
fn world() -> CasWorld {
    CasWorld::default()
}

#[given("an initialized empty CAS store")]
fn given_empty_cas_store(world: &CasWorld) {
    *world.store.borrow_mut() = InMemoryCas::new();
}

#[when("an artifact with a valid SHA-256 digest is stored")]
fn when_artifact_stored(world: &CasWorld) {
    let payload = b"hello hermetic airlock".to_vec();
    let digest = Sha256Digest::from_bytes(&payload);
    world
        .store
        .borrow_mut()
        .insert(digest.clone(), payload.clone())
        .expect("matching payload digest is accepted");
    *world.last_digest.borrow_mut() = Some(digest);
    *world.last_payload.borrow_mut() = payload;
}

#[then("the CAS store contains the artifact digest")]
fn then_cas_contains_digest(world: &CasWorld) {
    let digest = world.last_digest.borrow().clone().expect("digest set");
    assert!(world.store.borrow().contains(&digest));
}

#[then("the retrieved payload matches the stored bytes")]
fn then_retrieved_matches(world: &CasWorld) {
    let digest = world.last_digest.borrow().clone().expect("digest set");
    let payload = world.last_payload.borrow().clone();
    let retrieved = world.store.borrow().get(&digest).map(<[u8]>::to_vec);
    assert_eq!(retrieved, Some(payload));
}

#[scenario(path = "features/cas.feature", index = 0)]
fn bdd_cas_contract(world: CasWorld) {
    let _ = world;
}
