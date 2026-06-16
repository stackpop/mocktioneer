//! Typed application config, loaded from `mocktioneer.toml`. The TOML file
//! maps 1:1 onto this struct (no `[config]` wrapper). v1 carries a single
//! `bid_cpm` field; `config validate --strict` enforces the rules below.

use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError};

#[derive(Debug, Deserialize, Serialize, Validate, edgezero_core::AppConfig)]
#[serde(deny_unknown_fields)]
pub struct MocktioneerConfig {
    /// Fixed bid CPM in USD. Validated finite and strictly positive.
    #[validate(custom(function = "validate_bid_cpm"))]
    pub bid_cpm: f64,
}

/// `validator` passes `Copy` scalar fields (like `f64`) by value to custom
/// fns; `range` does not reject NaN / inf for floats, so validate explicitly.
fn validate_bid_cpm(value: f64) -> Result<(), ValidationError> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(ValidationError::new("bid_cpm_must_be_finite_positive"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_positive_finite_cpm() {
        let cfg = MocktioneerConfig { bid_cpm: 0.20 };
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn rejects_zero_negative_and_non_finite() {
        for bad in [0.0_f64, -1.0, f64::NAN, f64::INFINITY] {
            let cfg = MocktioneerConfig { bid_cpm: bad };
            assert!(cfg.validate().is_err(), "expected {bad} to be rejected");
        }
    }
}
