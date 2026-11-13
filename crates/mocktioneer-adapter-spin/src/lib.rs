#[cfg(target_arch = "wasm32")]
use mocktioneer_core::MocktioneerApp;
#[cfg(target_arch = "wasm32")]
use spin_sdk::http::Request;
#[cfg(target_arch = "wasm32")]
use spin_sdk::http_service;

#[cfg(target_arch = "wasm32")]
#[http_service]
async fn handle(req: Request) -> anyhow::Result<edgezero_adapter_spin::SpinResponse> {
    edgezero_adapter_spin::run_app::<MocktioneerApp>(req).await
}
