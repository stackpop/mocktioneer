#![cfg_attr(target_arch = "wasm32", no_main)]
#![cfg_attr(
    target_arch = "wasm32",
    expect(
        unsafe_code,
        reason = "#[http_component] expands to WASI wit-bindgen code containing an export_name function plus unsafe declarations and blocks; this is the only entry into the Spin runtime"
    )
)]

#[cfg(target_arch = "wasm32")]
use mocktioneer_core::MocktioneerApp;
#[cfg(target_arch = "wasm32")]
use spin_sdk::http::{IncomingRequest, IntoResponse};
#[cfg(target_arch = "wasm32")]
use spin_sdk::http_component;

#[cfg(target_arch = "wasm32")]
#[http_component]
async fn handle(req: IncomingRequest) -> anyhow::Result<impl IntoResponse> {
    edgezero_adapter_spin::run_app::<MocktioneerApp>(include_str!("../../../edgezero.toml"), req)
        .await
}
