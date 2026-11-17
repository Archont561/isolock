//! Property-based invariants testing using proptest.

use isolock_core::{is_valid_semver, Semver, VERSION};
use proptest::prelude::*;

proptest! {
    #[test]
    fn semver_parsing_succeeds_for_any_numeric_triple(
        major in 0u64..10_000,
        minor in 0u64..10_000,
        patch in 0u64..10_000,
    ) {
        let v_str = format!("{major}.{minor}.{patch}");
        let parsed = Semver::parse(&v_str);
        prop_assert!(parsed.is_ok());
        let s = parsed.unwrap();
        prop_assert_eq!(s.major, major);
        prop_assert_eq!(s.minor, minor);
        prop_assert_eq!(s.patch, patch);
        prop_assert!(is_valid_semver(&v_str));
    }

    #[test]
    fn semver_parsing_rejects_alphabetic_noise(
        s in "[a-zA-Z~!@#$%^&*()_+]{1,32}"
    ) {
        prop_assert!(Semver::parse(&s).is_err());
        prop_assert!(!is_valid_semver(&s));
    }

    #[test]
    fn exported_version_always_satisfies_semver_invariants(_dummy in 0..10) {
        prop_assert!(!VERSION.is_empty());
        let parsed = Semver::parse(VERSION);
        prop_assert!(parsed.is_ok());
        let semver = parsed.unwrap();
        prop_assert_eq!(format!("{}.{}.{}", semver.major, semver.minor, semver.patch), VERSION);
    }
}
