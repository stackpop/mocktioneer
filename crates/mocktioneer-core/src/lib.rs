pub mod aps;
pub mod auction;
pub mod config;
pub mod mediation;
pub mod openrtb;
pub mod render;
pub mod routes;
pub mod verification;

use edgezero_core::app::App;
use edgezero_core::config_store::ConfigExtractionLimits;
use edgezero_core::ingress::{AdmissionDecision, DEFAULT_INBOUND_READ_BUDGET, IngressGrant};
use edgezero_core::router::RouteResolution;
use edgezero_core::{EdgeError, ResponseEgressCompletion};
use std::time::Duration;

const BID_READ_BUDGET: Duration = Duration::from_secs(10);
const CONFIG_READ_TIMEOUT: Duration = Duration::from_secs(2);

edgezero_core::app!(
    "../../edgezero.toml",
    MocktioneerApp,
    configure = crate::configure_app
);

fn configure_app(app: &mut App) -> Result<(), EdgeError> {
    app.set_config_extraction_limits(ConfigExtractionLimits {
        timeout: CONFIG_READ_TIMEOUT,
        ..ConfigExtractionLimits::default()
    })?;
    app.set_ingress_admission_policy(|head| {
        let budget = if matches!(head.route_resolution(), RouteResolution::Matched(route) if route.class() == Some("bid")) {
            BID_READ_BUDGET
        } else {
            DEFAULT_INBOUND_READ_BUDGET
        };
        // Completion/grant match d47c8026's private default policy. Upstream has
        // no public delegate/getter; recheck these values when reviewing a repin.
        AdmissionDecision::Admit {
            completion: ResponseEgressCompletion::empty(),
            grant: IngressGrant::empty(),
            read_deadline: head.read_deadline_after(budget),
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use edgezero_core::app::App;
    use edgezero_core::http::{HeaderMap, Method, Version};
    use edgezero_core::{IngressBeginOutcome, IngressHeadParts, PlatformMetadata};
    use std::time::Duration;

    #[test]
    fn route_classes_choose_request_start_relative_read_deadlines() {
        let app = App::build::<super::MocktioneerApp>(PlatformMetadata::default()).unwrap();
        assert!(super::CONFIG_READ_TIMEOUT < super::BID_READ_BUDGET);
        let start = app.monotonic_clock().now();
        for (method, path, seconds) in [
            (Method::POST, "/openrtb2/auction", 10),
            (Method::POST, "/e/dtb/bid", 10),
            (Method::POST, "/adserver/mediate", 10),
            (Method::GET, "/", 30),
            (Method::OPTIONS, "/openrtb2/auction", 30),
            (Method::GET, "/unknown", 30),
            (Method::PUT, "/openrtb2/auction", 30),
        ] {
            let head = IngressHeadParts::new(
                method,
                path.parse().unwrap(),
                Version::HTTP_11,
                HeaderMap::new(),
            );
            let IngressBeginOutcome::Admitted(prepared) = app.begin_ingress(head, start).unwrap()
            else {
                panic!("expected admission");
            };
            assert_eq!(
                prepared
                    .read_deadline()
                    .instant()
                    .checked_duration_since(start),
                Some(Duration::from_secs(seconds))
            );
        }
    }

    #[test]
    fn manifest_declares_optional_outbound_with_matching_spin_hosts() {
        let manifest: toml::Value = toml::from_str(include_str!("../../../edgezero.toml")).unwrap();
        let spin: toml::Value =
            toml::from_str(include_str!("../../mocktioneer-adapter-spin/spin.toml")).unwrap();
        assert_eq!(
            manifest["capabilities"]["optional"].as_array().unwrap(),
            &[toml::Value::String("outbound-http".to_owned())]
        );
        assert_eq!(
            manifest["capabilities"]["outbound"]["hosts"],
            spin["component"]["mocktioneer"]["allowed_outbound_hosts"]
        );
        assert!(manifest["capabilities"].get("required").is_none());
        let classified: Vec<_> = manifest["triggers"]["http"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|trigger| trigger.get("class").is_some())
            .collect();
        assert_eq!(classified.len(), 3);
        for id in ["openrtb_auction", "aps_bid", "adserver_mediate"] {
            let trigger = classified
                .iter()
                .find(|trigger| trigger["id"].as_str() == Some(id))
                .unwrap();
            assert_eq!(trigger["class"].as_str(), Some("bid"));
            assert_eq!(
                trigger["methods"].as_array().unwrap(),
                &[toml::Value::String("POST".to_owned())]
            );
        }
    }
}
