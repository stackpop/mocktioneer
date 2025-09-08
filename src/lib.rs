pub mod openrtb;
pub mod config;
pub mod render;
pub mod auction;
pub mod routes;

pub use routes::build_app;

// wasm entrypoint lives in src/main.rs for Fastly Compute build

