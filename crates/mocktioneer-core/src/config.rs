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
}
