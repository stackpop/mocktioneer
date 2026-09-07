//! Typed application config, loaded from `mocktioneer.toml`. The TOML file
//! maps 1:1 onto this struct (no `[config]` wrapper). v1 carries a single
//! `bid_cpm` field; `config validate --strict` enforces the rules below.

use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Deserialize, Serialize, Validate, edgezero_core::AppConfig)]
#[serde(deny_unknown_fields)]
pub struct MocktioneerConfig {
    /// Fixed bid CPM in USD. Must be strictly positive. `exclusive_min` also
    /// rejects `0.0`, negatives, and `NaN` (any comparison with `NaN` is
    /// false); non-finite floats are additionally rejected by edgezero's typed
    /// config loader before this runs.
    #[validate(range(exclusive_min = 0.0_f64))]
    pub bid_cpm: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auction::FIXED_BID_CPM;
    use edgezero_core::app_config::{AppConfigError, load_app_config};
    use std::fs::write;
    use tempfile::tempdir;

    #[test]
    fn accepts_positive_finite_cpm() {
        let cfg = MocktioneerConfig { bid_cpm: 0.20 };
        cfg.validate().unwrap();
    }

    #[test]
    fn rejects_zero_negative_and_nan() {
        // `range(exclusive_min = 0.0)` rejects these (NaN fails every
        // comparison). `+Inf` passes the range check but is rejected upstream
        // by edgezero's non-finite-float loader, so it is not asserted here.
        for bad in [0.0_f64, -1.0_f64, f64::NAN] {
            let cfg = MocktioneerConfig { bid_cpm: bad };
            assert!(cfg.validate().is_err(), "expected {bad} to be rejected");
        }
    }

    /// Pins the upstream guarantee the `bid_cpm` doc comment relies on: the
    /// typed-config loader rejects non-finite floats *before* validation runs.
    /// `range(exclusive_min = 0.0)` alone would let `inf` through (`inf > 0.0`
    /// is true) and TOML can express `bid_cpm = inf`, so without that guard an
    /// operator could serve infinite-priced bids. If edgezero ever drops it,
    /// this test fails here rather than silently in production.
    #[test]
    fn loader_rejects_non_finite_bid_cpm() {
        let dir = tempdir().expect("tempdir");
        for literal in ["inf", "-inf", "nan"] {
            let path = dir.path().join(format!("mocktioneer-cfg-{literal}.toml"));
            write(&path, format!("bid_cpm = {literal}\n")).expect("write temp config");
            let result = load_app_config::<MocktioneerConfig>(&path, "mocktioneer");
            // `InvalidValue` specifically — the loader's non-finite guard, not a
            // downstream validation error (which `inf` would never trigger).
            assert!(
                matches!(result, Err(AppConfigError::InvalidValue { .. })),
                "loader must reject `bid_cpm = {literal}` with InvalidValue",
            );
        }
    }

    /// The shipped `mocktioneer.toml.example` — which the Docker image bakes in
    /// and CI validates — must still deserialize into the current struct and
    /// carry the `FIXED_BID_CPM` default. Nothing else keeps the two in lockstep,
    /// so a drift in either the template or the constant fails here.
    #[test]
    fn example_template_matches_shipped_default() {
        let cfg: MocktioneerConfig =
            toml::from_str(include_str!("../../../mocktioneer.toml.example"))
                .expect("template parses into MocktioneerConfig");
        assert_eq!(cfg.bid_cpm.to_bits(), FIXED_BID_CPM.to_bits());
    }
}
