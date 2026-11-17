//! Property-based invariants testing for airlock plans using proptest.

use isolock_cas::Sha256Digest;
use isolock_core::Registry;
use isolock_plan::{check_plan_drift, AirlockPlan, PlannedArtifact};
use proptest::prelude::*;

proptest! {
    #[test]
    fn identical_plans_always_yield_in_sync(
        pkg_name in "[a-z0-9_-]{1,20}",
        version in "[0-9]{1,3}\\.[0-9]{1,3}\\.[0-9]{1,3}",
        hex in "[0-9a-f]{64}"
    ) {
        let digest = Sha256Digest::parse(&hex).unwrap();
        let artifact = PlannedArtifact {
            name: pkg_name,
            version,
            registry: Registry::Conda,
            digest,
        };

        let mut plan_a = AirlockPlan::new();
        plan_a.add(artifact.clone());

        let mut plan_b = AirlockPlan::new();
        plan_b.add(artifact);

        let drift = check_plan_drift(&plan_a, &plan_b);
        prop_assert!(drift.is_clean());
    }

    #[test]
    fn mismatched_digest_always_yields_drift(
        pkg_name in "[a-z0-9_-]{1,20}",
        version in "[0-9]{1,3}\\.[0-9]{1,3}\\.[0-9]{1,3}",
        hex1 in "[0-9a-f]{64}",
        hex2 in "[0-9a-f]{64}"
    ) {
        if hex1 != hex2 {
            let d1 = Sha256Digest::parse(&hex1).unwrap();
            let d2 = Sha256Digest::parse(&hex2).unwrap();

            let mut plan_a = AirlockPlan::new();
            plan_a.add(PlannedArtifact {
                name: pkg_name.clone(),
                version: version.clone(),
                registry: Registry::Npm,
                digest: d1,
            });

            let mut plan_b = AirlockPlan::new();
            plan_b.add(PlannedArtifact {
                name: pkg_name,
                version,
                registry: Registry::Npm,
                digest: d2,
            });

            let drift = check_plan_drift(&plan_a, &plan_b);
            prop_assert!(!drift.is_clean());
        }
    }
}
