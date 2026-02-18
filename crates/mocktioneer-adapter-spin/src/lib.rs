#[cfg(target_arch = "wasm32")]
use mocktioneer_core::MocktioneerApp;
#[cfg(target_arch = "wasm32")]
use spin_sdk::http::{IncomingRequest, IntoResponse};
#[cfg(target_arch = "wasm32")]
use spin_sdk::http_component;

#[cfg(target_arch = "wasm32")]
#[http_component]
async fn handle(req: IncomingRequest) -> anyhow::Result<impl IntoResponse> {
    edgezero_adapter_spin::run_app::<MocktioneerApp>(req).await
}
