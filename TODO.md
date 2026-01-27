# Mocktioneer TODO

Working backlog and per-task plans live here. Before coding, capture an approved plan in the sections below, then mark items as you progress.

## Queue (near-term)
- [ ] _(add upcoming work items here)_

## Active Plan
- [x] 2026-01-27: Follow-up doc fixes after user edits (schema accuracy, setup guidance, verbosity).
- [x] Re-audit remaining mismatches (mediation schema, signature verification fields, non-standard size behavior, Docker/CI references).
- [x] Update API docs to match current handlers (mediation request shape, signature fields, static asset error codes, pricing/default behavior).
- [x] Update guide/integration docs for repo/tooling reality (EdgeZero CLI install/run, Cloudflare wrangler config, remove/qualify Docker/CI, correct "no-bid" guidance).
- [x] Capture formatting/test status and summarize in Review with timestamp.
- [x] 2026-01-27: Review documentation exposure changes vs main (redundancy, inconsistency, missing docs, thoroughness, verbosity).
- [x] Capture diff scope vs `main` and list touched documentation files.
- [x] Review for redundancy and inconsistency across docs/README/edgezero config references.
- [x] Review for missing or thin documentation coverage.
- [x] Review for thoroughness and verbosity (clarity vs noise).
- [x] Summarize findings, assumptions, unresolved items in Review section with timestamp.
- [x] 2026-01-21: Low finding #1 — stabilize `/_/sizes` output (safe parsing, deterministic ordering) and align the test with dynamic size count.
- [x] 2026-01-21: Low finding #2 — reduce `decode_aps_price` visibility to test-only or crate-only.
- [x] 2026-01-21: Low finding #3 — reduce `size_key` allocation overhead in size lookups.
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

### 2026-01-21 – Review latest changes
- [x] Capture the current diff scope and impacted files for review.
- [x] Review `auction.rs` sizing/pricing updates plus APS/OpenRTB behavior and tests.
- [x] Review new sizes endpoint plus template/config updates.
- [x] Summarize findings, risks, and testing gaps for follow-up.

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

## Review
- Date: 2026-01-21 03:35 UTC
- Summary: Switched size lookups to packed integer keys to avoid string allocations; finished the remaining low findings.
- Assumptions: Packed size keys (u32 width/height) cover all supported sizes.
- Unresolved: None from the low-finding list.

- Date: 2026-01-27 01:06 UTC
- Summary: Reviewed new documentation set vs `main` for accuracy, redundancy, missing coverage, and verbosity; captured discrepancies against current code/config.
- Assumptions: The docs should describe the current edgezero-based implementation (no embedded edgezero submodule) and the documented API targets the shipped handlers in `mocktioneer-core`.
- Unresolved: Clarify intended supported size list/pricing tables, signature verification field names, and whether Docker/CI image guidance is in-scope for this repo.

- Date: 2026-01-27 02:05 UTC
- Summary: Updated API/guide/integration docs to match current mediation schema, signature verification fields, creative error behaviors, and edge tooling setup; clarified containerization and no-bid guidance.
- Assumptions: EdgeZero CLI is installed from the EdgeZero repository when needed; Docker usage is optional and requires a user-provided Dockerfile.
- Unresolved: None.

- Date: 2026-01-27 02:08 UTC
- Summary: Ran docs formatting and linting (`npm run format:write`, `npm run lint`) to normalize Markdown and confirm lint clean.
- Assumptions: None.
- Unresolved: None.
