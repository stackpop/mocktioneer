# Mocktioneer TODO

Working backlog and per-task plans live here. Before coding, capture an approved plan in the sections below, then mark items as you progress.

## Queue (near-term)
- [ ] _(add upcoming work items here)_

## Active Plan
- [x] Audit route extractors in `mocktioneer-core` to see where path/query parameters map to structs.
- [x] Introduce validator-enabled structs (derive or impl `Validate`) and swap handlers to use `ValidatedPath`/`ValidatedQuery`.
- [x] Run targeted tests (core crate) and document outcomes in the review log.
- [x] Narrow `StaticCreativeQuery` pixel handling to accept only explicit `true`/`false` and adjust route/tests accordingly.
- [x] Define validation rules for `OpenRTBRequest` (derive/implement `Validate`) in `openrtb.rs`.
- [x] Swap `handle_openrtb_auction` to use `ValidatedJson<OpenRTBRequest>` and handle validation errors gracefully.
- [x] Extend tests (unit + endpoints) to cover invalid OpenRTB payload scenarios.
- [x] Allow static asset size parsing to tolerate malformed query delimiters (e.g., `&crid=...`) and cover with tests.
- [x] Simplify query extractors by limiting the string helper to click params and letting other extractors use serde defaults.

### 2025-09-21 – Align Mocktioneer with latest EdgeZero core
- [x] Update path dependencies (core, adapters) and shared crates to match the reorganised EdgeZero workspace; drop `edgezero-controller`, align `validator` version, and refresh the lockfile.
- [x] Refactor `mocktioneer-core` routes for the new async handler + `RouterService` API (response builders, CORS/logger middleware) and ensure helpers cover headers/cookies.
- [x] Revise crate-level tests (module + integration) to drive handlers via the new request/context types and router entrypoints.
- [x] Adapt Fastly and Cloudflare bins to the new adapter APIs (dispatch helpers, logging init) while preserving config-driven logging behaviour.
- [x] Run `cargo check` for the `mocktioneer` workspace to confirm the updated code compiles without regressions.

### 2025-09-21 – Restore action macro ergonomics
- [x] Convert `mocktioneer-core` route handlers to use `#[edgezero_core::action]` with async functions and extractor inputs (`ValidatedPath`, `ValidatedQuery`, `ValidatedJson`, etc.).
- [x] Adjust middleware wiring/tests to call the macro-generated handlers while keeping CORS/logger behaviour.
- [x] Ensure Fastly/Cloudflare bins still compile after handler signature changes and run the core crate tests.

## Review Log
- **2025-09-22 16:59 UTC** – Adopted the shared `Headers` extractor + new core validation errors (no more local wrappers); tests and checks stay green.
- **2025-09-20 02:18 UTC** – Swapped mocktioneer handlers to use `#[action]` extractors (plus custom query/path helpers) and revalidated core/bin builds (`cargo test -p mocktioneer-core`, `cargo check`).
- **2025-09-20 02:09 UTC** – `cargo test -p mocktioneer-core` now passes after refactoring the test helpers to use the new router helpers.
- **2025-09-18 02:27 UTC** – Adopted `ValidatedPath`/`ValidatedQuery` in `mocktioneer-core` routes with validator-backed structs (size, bid, pixel toggle, click params) and added regression tests. `cargo test -p mocktioneer-core`.
- **2025-09-18 02:31 UTC** – Simplified bid validation to use `#[validate(range(min = 0.0))]` instead of a custom helper; tests remain green. `cargo test -p mocktioneer-core`.
- **2025-09-18 02:34 UTC** – Restricted creative pixel query to literal `true`/`false` values, updated handler defaulting, and revalidated tests. `cargo test -p mocktioneer-core`.
- **2025-09-18 02:41 UTC** – Reverted iframe template flattening per request; preserved multi-line markup for readability.
- **2025-09-18 02:45 UTC** – Updated size parsing to ignore trailing query fragments (missing `?`) and added coverage for malformed delimiters. `cargo test -p mocktioneer-core`.
- **2025-09-18 02:57 UTC** – After router fix, simplified size parsing back to path-only and removed redundant malformed-delimiter test. `cargo test -p mocktioneer-core`.
- **2025-09-18 02:51 UTC** – Switched creative pixel query extractor to `Option<bool>` with strict deserialization and adjusted failure status to 400. `cargo test -p mocktioneer-core`.
- **2025-09-18 02:53 UTC** – Added validator-backed `OpenRTBRequest` (with required `id`/`imp`), updated auction handler to use `ValidatedJson`, and introduced rejection tests for malformed payloads. `cargo test -p mocktioneer-core`.
- **2025-09-18 03:06 UTC** – Simplified click query extraction by parsing width/height as numbers (validated range) and dropping generic string coercion; full test suite still passes. `cargo test -p mocktioneer-core`.
- **2025-09-18 02:55 UTC** – Trimmed query extraction helpers to only coerce click params into strings; booleans now use native serde handling. `cargo test -p mocktioneer-core`.
- **2025-09-20 01:38 UTC** – Updated dependencies and rewrote the EdgeZero integration (middleware, async handlers, tests, Fastly/Cloudflare adapters) to match the latest core API; `cargo check` on the mocktioneer workspace succeeds.
