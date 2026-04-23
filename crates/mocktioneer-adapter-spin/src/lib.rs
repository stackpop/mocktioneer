//! Spin adapter for Mocktioneer.
//!
//! This crate is a `cdylib` targeting `wasm32-wasip2`. All runtime code is
//! gated behind `#[cfg(target_arch = "wasm32")]` so the crate compiles (but is
//! empty) on the host target, allowing `cargo fmt`, `cargo clippy`, and
//! `cargo test` to run across the whole workspace without pulling in WASM-only
//! dependencies.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

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
