Run the full CI gate checks locally before pushing. Run all commands sequentially and report any failures:

1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace --all-targets --all-features -- -D warnings`
3. `cargo test --workspace --all-targets`
4. `cargo check --workspace --all-targets --features "fastly cloudflare"`
5. `cargo run -p mocktioneer-cli -- config validate --strict --app-config mocktioneer.toml.example` (`mocktioneer.toml` is gitignored; validate the committed template)

If any step fails, show the errors and suggest fixes. Do not proceed to the next step until the current one passes.
