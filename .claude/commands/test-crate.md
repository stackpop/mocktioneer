Run tests for a specific crate. Usage: /test-crate <crate-name>

Run `cargo test -p $ARGUMENTS` and report results. If no crate name is provided, ask which crate to test from the workspace members:

- mocktioneer-core
- mocktioneer-adapter-axum
- mocktioneer-adapter-cloudflare
- mocktioneer-adapter-fastly
