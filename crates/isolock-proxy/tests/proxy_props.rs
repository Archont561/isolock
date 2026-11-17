//! Property-based invariants testing for proxy decision logic using proptest.

use isolock_core::AirlockMode;
use isolock_proxy::{compress_brotli, decompress_brotli, evaluate_proxy_request, ProxyDecision};
use proptest::prelude::*;

proptest! {
    #[test]
    fn brotli_compression_losslessly_roundtrips_for_any_payload(
        payload in proptest::collection::vec(proptest::num::u8::ANY, 0..4096)
    ) {
        let compressed = compress_brotli(&payload).expect("compression succeeds");
        let decompressed = decompress_brotli(&compressed).expect("decompression succeeds");
        prop_assert_eq!(decompressed, payload);
    }

    #[test]
    fn cached_artifact_always_served_regardless_of_mode(
        mode_idx in 0..3
    ) {
        let mode = match mode_idx {
            0 => AirlockMode::Connected,
            1 => AirlockMode::Ci,
            _ => AirlockMode::LocalAirlock,
        };
        let decision = evaluate_proxy_request(mode, true);
        prop_assert_eq!(decision, ProxyDecision::ServeCached);
    }

    #[test]
    fn uncached_never_fetches_upstream_in_airlock_or_ci(
        is_ci in proptest::bool::ANY
    ) {
        let mode = if is_ci { AirlockMode::Ci } else { AirlockMode::LocalAirlock };
        let decision = evaluate_proxy_request(mode, false);
        prop_assert_eq!(decision, ProxyDecision::FailClosedAirlockViolation);
    }
}
