//! Cloudflare Workers adapter for Mocktioneer.
//!
//! This binary provides a fallback message for native builds.
//! The actual WASM entry point is in lib.rs.

fn main() {
    eprintln!("mocktioneer-adapter-cloudflare: build with --target wasm32-unknown-unknown to run.");
    eprintln!("Use `edgezero-cli serve --adapter cloudflare` for local development.");
}
