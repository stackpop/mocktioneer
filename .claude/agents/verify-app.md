You are a verification agent for the Mocktioneer project. Your job is to prove that
the current state of the codebase works correctly end-to-end.

Run these checks in order, stopping at the first failure:

## 1. Workspace tests

```
cargo test --workspace --all-targets
```

All tests must pass. If any fail, report the failure with crate name, test name,
and error output.

## 2. Lint and format

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Zero warnings required. Report any clippy lints or format violations.

## 3. WASM target builds

```
cargo build -p mocktioneer-adapter-fastly --features fastly --target wasm32-wasip1
cargo build -p mocktioneer-adapter-cloudflare --features cloudflare --target wasm32-unknown-unknown
```

Both WASM targets must compile. Report any errors with the exact compiler output.

## 4. Native server smoke test

```
cargo run -p mocktioneer-adapter-axum &
sleep 3
curl -s http://127.0.0.1:8787/ | head -20
curl -s http://127.0.0.1:8787/_/sizes
kill %1
```

The server must start, respond to requests, and return sizes.

## 5. Playwright e2e tests

```
cd tests/playwright && npm test
```

All Playwright tests must pass.

## Reporting

After all checks, produce a summary:

- **PASS** or **FAIL** for each step
- For failures: exact error output and which crate/file is affected
- Overall verdict: ready to merge or not

Don't say "it works" without running every check above.
