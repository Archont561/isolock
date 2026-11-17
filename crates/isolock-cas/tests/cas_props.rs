//! Property-based invariants testing for CAS digests using proptest.

use isolock_cas::{InMemoryCas, Sha256Digest};
use proptest::prelude::*;

proptest! {
    #[test]
    fn valid_hex_64_parses_successfully(
        hex in "[0-9a-fA-F]{64}"
    ) {
        let digest = Sha256Digest::parse(&hex);
        prop_assert!(digest.is_ok());
        let d = digest.unwrap();
        prop_assert_eq!(d.as_str(), hex.to_ascii_lowercase());
        prop_assert!(d.sharded_path().starts_with("sha256/"));
        prop_assert_eq!(d.sharded_path().len(), 7 + 2 + 1 + 2 + 1 + 64);
    }

    #[test]
    fn invalid_length_or_chars_rejected(
        bad in "[0-9a-fA-F]{0,63}|[0-9a-fA-F]{65,80}|[^0-9a-fA-F]{1,10}"
    ) {
        if bad.len() != 64 || !bad.chars().all(|c| c.is_ascii_hexdigit()) {
            prop_assert!(Sha256Digest::parse(&bad).is_err());
        }
    }

    #[test]
    fn inserted_payload_always_roundtrips(
        hex in "[0-9a-f]{64}",
        payload in proptest::collection::vec(any::<u8>(), 0..1024)
    ) {
        let digest = Sha256Digest::parse(&hex).unwrap();
        let mut store = InMemoryCas::new();
        store.insert(digest.clone(), payload.clone());
        prop_assert!(store.contains(&digest));
        prop_assert_eq!(store.get(&digest), Some(payload.as_slice()));
    }
}
