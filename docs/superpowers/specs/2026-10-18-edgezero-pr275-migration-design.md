# Design: Migrate Mocktioneer to edgezero #275 (outbound HTTP and request lifecycle)

- **Date:** 2026-10-18
- **Branch:** `feat/edgezero-pr275-migration`, cut from PR #118
  (`worktree-deps-rust-1.98-align-ez366`, head `e6c2982`)
- **Upstream:** [IABTechLab/edgezero#275](https://github.com/IABTechLab/edgezero/pull/275),
  "implement portable outbound HTTP and request lifecycle contracts". **Draft,
  open, unmerged.**
  - Current implementation pin: `d47c8026a941a2361182c9e2042bebdcb8be4d04`,
    the PR head observed for the user's 2026-10-23 latest-revision request.
    The original R1–R9 audit used `f6c708ccb5be19ba9fa92575db3c08c0f17e0d72`;
    R10 records the implementation/API/wire delta rather than treating its old
    source line references as a fresh audit of every upstream subsystem.
  - Preflight head on 2026-10-21: `852ec74bbb72139dca2809f3537689bb1ee5f0c6`,
    base `main`. Final pre-handoff refresh observed `619c69bd2b2b24f35210c482b560cb46b1b96888`;
    PR #275 remains draft/open/unmerged. R10 supersedes the earlier frozen-pin
    decision with the explicitly requested latest-head adoption (§11).
  - It contains release `v0.0.10` and the IABTechLab transfer (#419). Nothing on
    edgezero `main` is missing from it.
- **Status:** migration and review fixes implemented; latest-head qualification is recorded in R10. Upstream merge/release and JWKS destination admission remain gated. Execution is tracked in
  the [implementation plan](../plans/2026-10-20-edgezero-pr275-migration-implementation.md).
- **Revision history:**
  - **R1:** initial draft.
  - **R2:** self-review. A throwaway compile probe of every target against
    `f6c708cc`, plus four independent reviews (edgezero API claims, mocktioneer
    claims, lifecycle design, whole-document coherence). Every claim marked
    "verified" below was checked against code at `f6c708cc` or by the probe.
  - **R3:** an external review of R1. R2 already resolved most of its findings.
    R3 adds:
    - the full wasm-bindgen family resolution check (§5.1)
    - a fixed detail for internal-kind transport errors (§5.6)
    - JWKS cache alias handling and a byte budget (§6.2)
    - a config-error matrix with tests (§7.1)
    - an explicit wire-diff corpus (§8)
    - a Cloudflare wasm JWKS flow test, a Fastly entrypoint smoke and a Spin
      drift rejection check (§9)
    - the Cloudflare KV binding prerequisite (§9, §10)
    - the `/_mocktioneer/config` scope note (§6.3, §11)
  - **R4:** resolves the R3 review findings:
    - buffered JWKS error ordering and the five-second outbound timeout are
      explicit wire changes (§5.6, §7)
    - the cache budget charges owned parsed allocations (§6.2)
    - framing requirements distinguish normalized headers from raw input and
      cover every ordinary empty response (§5.8, §7, §8)
    - the wire diff uses separate seeded and unseeded server startups (§8)
    - Fastly uses the pinned Viceroy HTTP server, and the local Cloudflare
      workerd smoke is required (§9)
  - **R5:** records the user's 2026-10-20 clarification: the related project
    does not depend on mocktioneer, and no outbound batch fan-out is needed.
    Removes the corresponding downstream compatibility conditions.
  - **R6:** incorporates the remaining decoding, normalized-head test and docs-CI
    review corrections; clarifies shared framing versus provider delivery tests;
    records execution approval and freezes the audited implementation pin.
  - **R7:** execution confirms the exact F10 header-group permutation, adds the
    required legacy-date BYOB compatibility flag, fixes direct Wrangler command
    CWD, and records completed verification in the implementation plan.
  - **R8:** review follow-up adds CORS to matched-route errors (C8), preserves
    error logging by placing CORS outside RequestLogger, clarifies clock-dependent
    operational checks and restores JWKS debug diagnostics. R7 timeout and cache
    policies remain unchanged.
  - **R9:** the second review fixes malformed-response comparison, adds Spin
    admitted-ingress/finalization coverage and real handler framing contracts,
    shares immutable cached keys and preserves redaction without kind-string
    matching. The new JWKS destination-admission decision remains a merge gate
    pending the user's answer; DNS syntax validation is not SSRF prevention.
  - **R10:** explicitly authorized latest-head adoption on 2026-10-23. All six
    dependencies and all eight lockfile sources move to `d47c8026`. Test helpers
    select platform framing modes and pass response status/decoding policy.
    The final metadata refresh advanced from `7fdef8b0` to `d47c8026` with
    bounded synthesized-response headers. Independent core/provider audits
    verified the new policy defaults empty; Mocktioneer leaves it unconfigured,
    preserving its existing routed-error CORS and routing/head-error scope.
    Upstream body overflow becomes 413 `payload_too_large` (F14), without
    changing caps, budgets, CORS or cache policy. The full local matrix is rerun.

## 0. Confirmed scope (2026-10-20)

1. **The related project does not depend on mocktioneer.** There is no
   cross-project Cargo source-identity requirement, public-helper compatibility
   requirement, or need to provide it with a stable mocktioneer commit. Remove
   `mocktioneer_core::build_app()` as proposed in §5.2. Mocktioneer's own wire
   compatibility and merge/release requirements still apply.
2. **No outbound batch fan-out is needed.** `send_all_until` and
   `start_batch_until` adoption remain out of scope (§2). Test clients implement
   the required trait method without adding a production fan-out feature.

## 1. Intent

Mocktioneer moves onto the edgezero version in #275 and adopts the new APIs
where they do real work. The related trusted-server project does not depend on
mocktioneer and imposes no downstream integration requirements on this migration.

The cut is hard **for the edgezero API**:

- no shims or feature flags
- no code that supports both edgezero versions
- no deprecation period

Deploy-side changes ship in the same release.

The hard cut does **not** extend to mocktioneer's own wire contract. Bid output
and responses stay the same, except for the changes listed in §7.

Decided by the user: adopt the new APIs and the hard cutoff, with no dependency
from the related project and no batch fan-out requirement. Everything marked
**(my call)** was decided while writing this spec. Appendix A collects those
calls for confirmation.

## 2. Goals, non-goals, anti-goals

### Goals

1. Mocktioneer builds and tests on #275 and can be deployed on it.
   - Every CI gate passes.
   - The three contract suites pass on their runtimes: Viceroy, the
     wasm-bindgen test runner, and wasmtime.
   - This PR makes no live deploys. §10 is the release runbook **(my call)**.
2. Every edgezero API that #275 removed or changed is migrated wherever
   mocktioneer uses it (§5).
3. Mocktioneer adopts the new APIs that have a real job here (§6):
   - **the outbound HTTP client** for the JWKS fetch, with an explicit policy and
     a capability declaration
   - **the portable monotonic clock**, with a bounded JWKS cache
   - **a fallible `configure` hook** that installs route-aware ingress admission
     for the bid routes and a bounded config-read timeout
   - **size-capped JSON extractors** for the auction and APS routes
4. The wire contract stays stable. For the same input, every response is
   byte-identical to the pre-migration build, except for the changes in §7. §8
   gives the procedure that proves it.

### Non-goals

- **The outbound batch API (confirmed by the user).** Nothing in mocktioneer
  fans out; mediation runs in process (§0.2).
- **App-owned egress completion tokens** (`ResponseEgressCompletion::new`,
  `late_bound`, `join`, typed `IngressGrant`s). Mocktioneer holds nothing that
  must be released when a response ends. It uses
  `ResponseEgressCompletion::empty()` and `IngressGrant::empty()`.
- **Reusing the built app across requests.** This covers Cloudflare
  `dispatch_app`, Fastly `serve_app`, and Spin.
  - Upstream says to retain an app only when measurements show that building it
    is expensive (edgezero `docs/guide/adapters/overview.md`). There is no such
    measurement for mocktioneer.
  - Fastly `Serve` defaults to unlimited requests and an idle timeout of
    `Duration::MAX`, and idle time is billed.
  - Under `Serve`, a failed build sends a typed assembly error and retries
    assembly on the next request; an aborted delivery ends the loop.
  - Revisit this with measurements.
- **The memory-envelope check.** With template constants it comes out
  `Indeterminate` on all four platform profiles: host-ingress accounting is
  unknown on Fastly and Cloudflare, and every fact is unknown on Spin and Axum.
  It can never fail.
- **Tight config size limits.** A blob over the limit renders 500 `internal`,
  not the 503 `config_out_of_date` that operators rely on. Also, the
  `config-read-allocation-bounds` capability is Unsupported on every adapter.
  Only the timeout is set (§6.3).
- **`ReadBodyBeforeFallback` for 404/405.** It turns the JSON 404/405 into a
  plain-text 413 or 408 whenever a body exceeds the cap, and it makes every
  404/405 wait for a body drain.
- **A class on every trigger.** The read deadline governs only body reads, and
  only the three POST routes read a body.
- **A custom error renderer, a detached-error egress factory, per-response
  egress deadlines, and the Fastly custom lifecycle** (`serve_custom`,
  `run_custom`).
- **The Akamai Functions profile.**
- **`required` capabilities.** `outbound-http` is BestEffort on Fastly, so
  marking it `required` fails the CLI gate.
- **Hosted capability qualification.**

### Anti-goals

- A wire change that §7 does not list.
- Docs or code claiming a guarantee that #275 marks `BestEffort` or
  `Unsupported` (§12).
- Any compatibility layer for the old edgezero API.
- Editing edgezero. Upstream problems are recorded for filing (§11).
- Copying errors from edgezero's own docs (§12.2).

## 3. Constraints

- **`CLAUDE.md` still applies:**
  - WASM compatibility first.
  - No Tokio in core.
  - `futures::executor::block_on` in tests.
  - No direct `http` imports.
  - Markup stays in templates.
  - `{id}` route syntax.
  - The strict clippy gate.
  - Deterministic pricing and bid selection for fixed input and configuration.
    Operational deadlines and JWKS cache expiry use monotonic clocks; signature
    freshness uses wall time. Operational outcomes can vary with timing and keys.
- **The CI gates in `CLAUDE.md` must pass,** plus the three contract jobs in
  `test.yml`.
- **Pin:** the six `edgezero-*` entries use
  `git = "https://github.com/IABTechLab/edgezero.git", rev = "d47c8026a941a2361182c9e2042bebdcb8be4d04"`.
  There is no seventh edgezero dependency (§6.1).
- **PR target (my call):** open the PR against `main`.
  - Every workflow triggers only on `pull_request: branches: [main]`, so a PR
    stacked on #118's branch would get no CI.
  - Until #118 merges, the diff also contains #118's commits.
- **Merge timing (my call):** follow the #118 precedent. Do not
  merge before #275 merges, then re-pin to the edgezero tag that ships it (§11).

## 4. Current state

Mocktioneer touches edgezero in these places:

- **The core crate:**
  - the `app!` macro and `#[action]` handlers
  - the `AppConfig`, `ValidatedJson`, `ValidatedQuery`, `Headers` and
    `ForwardedHost` extractors
  - a custom `FromRequest` (`ValidatedSize`) and a custom `Middleware` (`Cors`)
  - `Body` and `EdgeError`
  - one outbound call: the JWKS fetch in `verification.rs`, which uses
    `proxy::ProxyRequest` and `ctx.proxy_handle()`
- **The four adapter entrypoints.**
- **The three contract suites.**
- **`mocktioneer-cli`**, which reuses edgezero-cli's `*Args` and `run_*`.
- **Four copies of an in-memory `ConfigStore`:** in `routes.rs` tests,
  `tests/endpoints.rs`, and the Cloudflare and Fastly `tests/contract.rs`.

The JWKS fetch runs only when an OpenRTB request carries `site.domain` and an
`ext.trusted_server` block that passes the field and freshness checks. The
result is cached per domain for 10 minutes in a process-wide
`LazyLock<Mutex<HashMap>>`, keyed by the raw domain.

**Existing bug, fixed in §6.2:** the cache stores `std::time::Instant`
(`verification.rs:11, 148, 170`), which panics on `wasm32-unknown-unknown`. The
first successful JWKS fetch on Cloudflare therefore panics.

**Reason strings are part of bid output.** `routes.rs:325-334` turns a
verification error into `SignatureStatus::Failed { reason: err.to_string() }`,
and `auction.rs:160-165` and `render.rs:183-200` serialize that reason into
every bid's `adm`.

## 5. Required migration (forced by #275)

Line references are to `e6c2982`. Every item in this section was confirmed by
the compile probe.

### 5.1 Dependencies and toolchain

- **`Cargo.toml`:** the six-entry repin (§3).
- **R10 delta:** the dependency family/toolchain below was already aligned by
  the original migration. Moving from `f6c708cc` to `d47c8026` changes only the
  eight EdgeZero lockfile source identities; it needs no SDK or toolchain bump.
- **`crates/mocktioneer-adapter-cloudflare/Cargo.toml:33`:**
  `wasm-bindgen-test = "=0.3.71"` becomes `"=0.3.78"`. Without the bump the lock
  does not resolve: the old pin locks `js-sys =0.3.98`, and #275's `worker
  =0.8.5` needs `^0.3.102`.
- **Fastly adapter wasm32 dev-dependencies:** add
  `edgezero-adapter-fastly = { workspace = true, features = ["fastly", "test-utils"] }`
  and `futures`, for `block_on` in the contract tests.
- **Expected lock movement.** These are exact pins inside edgezero, verified by
  the probe.
  - **Backwards:**
    - `http` 1.5.0 → 1.4.2 and `serde_json` 1.0.151 → 1.0.150
    - the compression family: `async-compression` 0.4.45 → 0.4.43,
      `brotli-decompressor` 5.0.3 → 5.0.1, `compression-codecs` 0.4.40 → 0.4.38,
      `compression-core` 0.4.33 → 0.4.32, `crc32fast` 1.5.1 → 1.5.0, `flate2`
      1.1.10 → 1.1.9, `miniz_oxide` 0.9.1 → 0.8.9, `simd-adler32` 0.3.10 → 0.3.9
  - **Forwards:**
    - `worker` 0.8.3 → 0.8.5 (`worker-macros` and `worker-sys` 0.8.6),
      `hyper` 1.11.1 → 1.12.0, `wasm-streams` 0.5 → 0.6
    - **The wasm-bindgen family, which must move together:**
      - `wasm-bindgen` and its `-macro`, `-macro-support` and `-shared` crates:
        0.2.121 → 0.2.128
      - `js-sys` and `web-sys`: 0.3.98 → 0.3.105
      - `wasm-bindgen-futures`: 0.4.71 → 0.4.78
      - `wasm-bindgen-test` and `-macro`: 0.3.71 → 0.3.78, with
        `wasm-bindgen-test-shared` at 0.2.128
      - `minicov`: 0.3.9 → 0.3.8
  - **Unmoved:** `fastly`, `spin-sdk`, `bytes` and `reqwest` were already at the
    pinned versions.
  - **Check that the family resolved together:**
    `cargo tree -p mocktioneer-adapter-cloudflare --target wasm32-unknown-unknown -e normal,dev -i js-sys`
    shows a single `js-sys` version. Run the same check for `wasm-bindgen` and
    `web-sys`.
- **wasm-bindgen CLI:** CI takes the version from `Cargo.lock`, so it follows
  automatically. Locally, the runner bundled with the asdf Rust install is
  0.2.121 and cannot run the new artifacts. Install it with
  `cargo install wasm-bindgen-cli --version 0.2.128 --locked`.
- **`.tool-versions`:** wasmtime 48.0.1 → 48.0.2, and add `spin 4.1.0` to match
  edgezero.
  - The builder confirms that `asdf install` still succeeds with the `spin`
    line.
  - If it fails without an asdf Spin plugin, leave `spin` out and keep the
    "Spin 4.1+ required" note.
  - Update `CLAUDE.md:59` either way.
- **Remove `futures-util`** from `crates/mocktioneer-core/Cargo.toml` if the
  §5.6 port leaves it unused. Its only use is `verification.rs:7`.

### 5.2 App assembly

- **R10:** `Hooks::routes()` and generated `build_router()` now return
  `Result<RouterService, EdgeError>`. The app macro and `App::build` propagate
  this without new application code; there are no handwritten Hook callers here.
- `Hooks::build_app()` is gone. Build apps with
  `App::build::<MocktioneerApp>(platform)?`.
- **Tests:**
  - Core tests use `PlatformMetadata::default()`.
  - Cloudflare service tests use `edgezero_adapter_cloudflare::CLOUDFLARE_PLATFORM`.
- **`mocktioneer_core::build_app()` (`lib.rs:14-18`):**
  - Delete it and its unused `Hooks as _` import (`lib.rs:12`), following the
    confirmed absence of a downstream dependency (§0.1). Build directly with
    `App::build::<MocktioneerApp>(platform)?` at the existing call sites.
- `Hooks::configure` now returns `Result<(), EdgeError>`. See §6.3.

### 5.3 Adapter entrypoints

- **Fastly** (`src/main.rs:13-17`):
  - Remove `#[fastly::main]` and write
    `pub fn main() -> Result<(), fastly::Error> { edgezero_adapter_fastly::run_app::<MocktioneerApp>() }`.
  - Keep the `wasm32` gating and the host stub.
- **Spin:** not forced, because `anyhow::Result<impl IntoResponse>` still
  compiles. Change the return type to
  `anyhow::Result<edgezero_adapter_spin::SpinResponse>` to match the template
  **(my call)**.
- **Cloudflare and Axum:** unchanged.
  - Axum `run_app` now fails if a logger is already installed. Mocktioneer
    installs none.
  - Axum loads its config snapshot before binding the listener. A restart after
    `config push` was already required at v0.0.10.

### 5.4 Platform config

- **`wrangler.toml`:** add
  `compatibility_flags = ["enable_request_signal", "enable_weak_ref", "internal_stream_byob_return_view"]`.
  - Without `enable_weak_ref`, bounded KV reads see no `FinalizationRegistry`.
    `AppConfig` then returns 503 `service_unavailable` with no `Retry-After`.
    The CLI does not validate this compatibility flag; the real workerd check is required.
  - Real workerd verification found that the retained 2023 date also needs
    `internal_stream_byob_return_view`. Its legacy internal-stream EOF returns
    undefined, while the bounded KV reader requires an empty returned view.
    Adding only this flag changed the same seeded auction from 503 to 200.
    This matches Cloudflare's [documented compatibility behavior](https://developers.cloudflare.com/workers/configuration/compatibility-flags/#returning-empty-uint8array-on-final-byob-read).
  - Keep `compatibility_date = "2023-05-01"` **(my call)**. Note that this date
    rules out `OutboundCachePolicy::Bypass` on Cloudflare (§6.1).
- **`edgezero.toml`:** add `[capabilities]` (§6.1) and `class = "bid"` on three
  triggers (§6.3). #275 now rejects:
  - unknown top-level keys
  - any key named `capabilities` other than the exact lowercase top-level table
  - an empty `class`
- **`spin.toml`:** no change.
  - `allowed_outbound_hosts = ["https://*:*"]` must equal
    `[capabilities.outbound].hosts`. If the hosts are omitted they default to
    the same value.
  - The drift check runs in edgezero-cli's `build`, `serve` and `deploy`, which
    `mocktioneer-cli` inherits. Fermyon's `spin build` and `spin up` do **not**
    run it.
- **`fastly.toml`:** no change. Dynamic backends were already needed for the
  JWKS fetch at v0.0.10.

### 5.5 Handler code

- `routes.rs:212, 234, 240`: replace `ctx.request().method()` and
  `ctx.request().uri()` with `ctx.method()` and `ctx.uri()`.
- `verification.rs`: the JWKS port (§5.6).
- Verified unchanged:
  - `#[action]`, including the `RequestContext(ctx)` special case
  - the `Body` variants and `into_bytes()`
  - `IntoResponse`, `FromRequest` and `Middleware`
  - `BlobEnvelope`, `StoreRegistry` and `ConfigStoreBinding`
  - `load_app_config` and `AppConfigError::InvalidValue`
  - the `AppConfig` signature
  - every `edgezero_cli` function, `args::*`, `DiffExit` and
    `init_cli_logger`. `run_build`, `run_serve` and `run_deploy` keep their
    signatures but add manifest discovery and the capability and Spin drift
    gates.
  - host extraction (`ForwardedHost`, `Host`, `Headers`), query parsing, path
    params and router matching, on every adapter

### 5.6 JWKS fetch port (forced: `proxy` is gone)

The proxy API no longer exists, so this port is part of the forced unit, not an
adoption step.

- **Client:**
  - Get it with `ctx.http_client()`. Every adapter installs one on every
    request, whether or not a capability is declared.
  - If it is `None`, keep today's error.
- **Request:**
  - `OutboundRequest::get(&jwks_url)`. It takes `AsRef<str>`, so the `Uri`
    parse step goes away.
  - It returns `EdgeError`, and `VerificationError` has no `From`, so it needs a
    `map_err`.
  - Set `.timeout(JWKS_FETCH_TIMEOUT)` (5 s) and
    `.max_response_bytes(MAX_JWKS_BODY_BYTES)`. Change the constant to `u64`,
    because the restriction lints deny `as` casts.
  - The cache policy is §6.1.
- **Send and read:**
  - `max_response_bytes` selects **buffered** mode. `client.send(req).await`
    performs response framing checks, decoding and bounded collection before
    returning a response. Body failures can therefore occur before mocktioneer
    sees the status. This ordering is accepted in F12 (§7).
  - Recognized gzip, Brotli and deflate responses are decoded before the final
    64 KiB buffered cap is applied. Old Axum did not decode these responses;
    other old adapters did not decode deflate. Encoded JWKS may now verify
    successfully. Axum and Fastly also default `Accept-Encoding` to `identity`
    when absent. These negotiation/decoding differences are accepted in F12.
  - Map any send-time `ResponseTooLarge` to `jwks_body_too_large_error`, and
    every other send error to `JWKS fetch failed: {detail}` using the sanitizing
    rule below.
  - After a successful send, require a 200. A non-200 response gets the status
    reason without parsing its JSON. Then read a 200 with
    `resp.into_bytes_bounded(MAX_JWKS_BODY_BYTES).await`,
    then `serde_json::from_slice::<TrustedServerResponse>`.
  - The second bounded read is defensive: standard clients return an already
    buffered body for this request. If a custom client returns a stream, a
    non-limit drain error still uses `Stream read failed: {detail}`.
  - Do **not** use `json_bounded`. It replaces serde's message with a fixed
    "failed to decode upstream JSON", which would change the parse-failure
    reason.
- **Reason strings.** These are `err.to_string()` values that reach bid `adm`.
  The test asserts each full string.

  | Stage | String | Status |
  | --- | --- | --- |
  | URL build | `HTTP error: Invalid JWKS URL: {detail}` | Prefix kept. The detail becomes edgezero's text; unreachable in practice, because the host is pre-validated. |
  | No client | `HTTP error: Proxy not available` | Kept verbatim **(my call)**, even though the proxy API is gone. |
  | Send, including buffered body failures | `HTTP error: JWKS fetch failed: {detail}` | Includes transport, decoding, body-read and timeout errors from `send()`. On Fastly and Cloudflare, failures previously observed while draining a response can move here from `Stream read failed:` (F12). |
  | Non-200 after a successful send | `HTTP error: JWKS server returned status: {status}` | String unchanged. Send-time failures take precedence; a payload-bearing non-200 can fail collection before this check (F12). |
  | Over cap | `HTTP error: JWKS response body exceeds 65536 byte limit` | String unchanged. Match `EdgeError::ResponseTooLarge { .. }` with `matches!`, both from `send()` (declared-length checks or bounded collection) and from the defensive `into_bytes_bounded()`. Keep `jwks_body_too_large_error`. |
  | Defensive body drain, other errors | `HTTP error: Stream read failed: {detail}` | Only for a custom client returning a stream despite buffered mode. Standard adapter body errors occur during `send()` and use its prefix. The detail text changes **(my call)**. |
  | Parse | `HTTP error: JWKS parse failed: {serde}` | Unchanged. |
  | Cache lock | `HTTP error: Cache lock poisoned: {detail}` | Removed. A poisoned lock becomes a cache miss (§6.2). |

- **How `{detail}` is produced from an `EdgeError`** **(my call)**. The text goes
  into bid `adm`, which is public. `err.to_string()` is `EdgeError`'s `Display`,
  not edgezero's sanitized wire renderer.
  - For the gateway, timeout and response-too-large kinds, the adapters supply
    fixed static text, so `Display` is safe. Examples: Axum "upstream request
    failed", Fastly "outbound destination could not be reached", Cloudflare
    "upstream response body failed".
  - `EdgeError::internal(..)` displays as `internal error: {source}`. Some
    adapter internals carry provider diagnostics in that source; Cloudflare, for
    example, wraps JavaScript runtime errors.
  - **Rule:** when `err.status() == StatusCode::INTERNAL_SERVER_ERROR`, the detail is the fixed text
    `internal error`. Otherwise it is `err.to_string()`.
  - A test feeds the stub an internal error whose source contains a sentinel
    string, and asserts that the sentinel does not appear in the reason.

- **Delete** `collect_jwks_body` and its test,
  `collect_jwks_body_rejects_oversized_once_body`. The over-cap path is covered
  by a stub-client test instead.
- **Stub client for tests:**
  - Implement `OutboundHttpClient` with `send`, plus `start_batch_until`
    returning `OutboundBatch::cutoff(n)`.
  - Build responses with
    `OutboundResponse::new(method, status, headers, body, clock, ContentTypeProvenance::Unknown)`.
  - Insert `HttpClient::with_client(stub)` into the request extensions.
  - At `d47c8026`, `ResponseBodyPolicy` also requires `response_decoding` and
    `status`. Copy the former from the actual request parts and pass the reply's
    actual status; decode fixtures use 200. Default decoding remains enabled,
    but 206 preserves original content coding. Non-200 JWKS still fails
    verification after a successful buffered send; partial-response collection
    can therefore change which upstream failure precedes that status check.
  - Capture the request and assert buffered response mode, a 65536-byte limit,
    and a five-second timeout. On success the stub returns a buffered body.
    Model buffered body failures as errors returned by `send`, rather than
    returning a stream that postpones the failure until application collection.
  - Cover:
    - success
    - a missing client
    - a non-200 status, including a malformed JSON body that must not be parsed
    - send-time over-cap failures from declared-length checks and cumulative
      chunk collection, including a payload-bearing non-200 response
    - a transport error
    - a buffered body-read or decode error, which uses `JWKS fetch failed:`
    - a typed timeout error and its reason mapping (C7)
    - a parse error
    - the defensive custom-client stream-drain error, tested separately from
      the standard buffered-client cases
  - Each case asserts its full reason string.

### 5.7 Test doubles and contract tests

- **Every `ConfigStore` mock:**
  - `get` returns `Result<Option<ConfigValue>, ConfigStoreError>`.
  - It implements the required
    `get_bounded(&self, key, clock: &MonotonicClock, deadline: Deadline, max_backend_bytes: u64, max_value_bytes: u64) -> Result<BoundedStoreRead<ConfigValue>, ConfigStoreError>`.
  - That method checks `deadline.is_expired_at(clock.now())` first, then
    delegates to `edgezero_core::config_store::finish_bounded_config_read`. That
    function is not re-exported at the crate root. Follow the demo mock, edgezero
    `examples/app-demo/crates/app-demo-core/src/handlers.rs:556-582`.
- **Fastly contract tests:**
  - Replace `.dispatch(req)` with `.capture_egress_for_test(req)`, followed by
    `complete_envelope`, as in edgezero
    `crates/edgezero-adapter-fastly/tests/contract.rs:49-56`.
  - Read bodies with `block_on(response.into_body().into_bytes_bounded(n))`.
  - R10 requires `envelope.begin(ResponseEgressFramingMode::HostAutomatic)`,
    matching the actual Fastly sender. Core, Axum, Cloudflare and Spin select
    `ExplicitBufferedLength`; `prepare_response_egress` now takes that mode first.
- **Cloudflare contract tests:** `test_env_ctx` installs two test-only shims with
  `Reflect::set`:
  1. a `waitUntil` function on the context object, as in edgezero
     `tests/contract.rs:234-246`. Without it, worker panics with
     `arg0.waitUntil is not a function`.
  2. a `globalThis.FixedLengthStream` polyfill (a `TransformStream` subclass)
     **(my call)**.
     - With a declared `Content-Length`, #275's Cloudflare egress uses the
       Workers-only `FixedLengthStream`. The browser runner lacks it; the root,
       pixel and auction responses exercised by the three base contracts all
       declare a length.
     - Without the polyfill, all three tests return 500. With it they pass, as
       the probe showed.
     - The alternative was dropping mocktioneer's explicit `Content-Length`. It
       was rejected because it changes headers.
     - Install the polyfill only when the native global is absent, and only in
       browser tests. Never replace a real `FixedLengthStream`.
     - The required local `wrangler dev` check (§9) covers the real Workers
       path. Passing the polyfilled tests cannot substitute for that check.
- **Spin contract tests:** build with `SPIN_PLATFORM`, validate normalized heads,
  admit and dispatch through the app, then prepare and complete the egress
  envelope. Controlled-clock tests cover selected read budgets and body expiry;
  the config limit is asserted as 2 s. These are WASI core lifecycle tests,
  not native Spin request-handle or provider writer tests.
- **The existing cache-seeding test** (`verification.rs` ~925-939, which inserts
  `Instant::now()`) moves to the new cache API (§6.2).
- **`router().oneshot(..)`** still works for handler tests. It skips:
  - admission
  - the app's config-extraction limits
  - an injected clock
  - the error renderer

  Tests of §6.3 must therefore go through `App::build`, then `begin_ingress`,
  then `dispatch_admitted`.

### 5.8 Empty-body framing (my call)

#275 streams responses that have no `Content-Length` on Fastly and Cloudflare.
Ordinary mocktioneer responses with an empty `Body::Once` currently lack an
explicit length, including:

- `/sync/start`'s 400 and 403 (`routes.rs:706, 716`)
- its empty 500 and 302 (`routes.rs:748-754`)
- `/resolve`'s 401s (`routes.rs:839, 843`)

Today they go out with `Content-Length: 0`; after the cut they may go out
chunked. Extend the shared `build_response` helper to set `Content-Length: 0`
for empty buffered responses whose status permits it, rather than patching
individual call sites. Axum already sends 0 for these ordinary empty responses.

Keep the bodyless rules at final egress: the framework rejects terminal 1xx
responses and strips 204 length. The helper still omits 204 length because
inserting and later removing it would reorder the preflight headers. Do
not replace a HEAD/304 representation length with zero merely because egress
suppresses its body. A 205 may carry a zero length. Cover ordinary empty
responses and these exceptions in helper tests. Exercise empty 400/403/401/302/500
responses through Fastly and Cloudflare egress contracts, constructing responses
for statuses whose route branches are unavailable on that runtime. On Fastly,
`ResponseEgressEnvelope::begin(ResponseEgressFramingMode::HostAutomatic)` already
calls shared `prepare_response_egress`;
assert the resulting prepared frame before completing the attempt. This proves
shared framing, not the private provider writer. Cloudflare tests exercise its
service response conversion. The required live smokes establish real delivery. Native
smokes cover reachable empty 400 and 302 responses; the Axum wire diff covers
the configured auth/allowlist branches. Cloudflare cannot exercise those 403/401
route branches because `std::env::var` is unsupported there. Do not assume that
Viceroy inherits arbitrary host variables into the guest.

## 6. New APIs adopted

### 6.1 JWKS request policy and capability declaration

- **Cache policy:** `OutboundCachePolicy::PlatformDefault` (the default) **(my
  call)**.
  - On Cloudflare, `Bypass` sets `RequestInit.cache = "no-store"`. With a
    compatibility date before 2024-11-11 and without the `cache_option_enabled`
    flag, the Workers runtime throws on that field. Every signed Cloudflare
    auction would then fail verification.
  - This preserves the previous Fastly and Cloudflare cache policy. It does
    not make provider caching irrelevant: a stale provider response can refill
    mocktioneer's cache after its 10-minute TTL expires. That TTL bounds local
    cache residency, not the maximum delay before a rotated key becomes visible.
    A provider-independent freshness policy would need a separate decision.
  - Record this as an upstream issue (§11). edgezero's capability table marks
    `outbound-cache-bypass` Native, and its own template uses the same 2023 date.
- **Budget:** `.timeout(5 s)` only. Combining `.timeout` with `.deadline` is
  redundant, because the earlier of the two wins.
  - This is a chosen wire change (C7): v0.0.10 used a 30 s Axum timeout and a
    15 s Fastly first-byte timeout. A previously successful slow JWKS fetch can
    now produce a failed-verification reason in the auction's `adm`.
  - Outbound deadlines are Native on Axum and BestEffort on Cloudflare, Fastly
    and Spin. The configured budget does not promise finite provider teardown.
- **Manifest:**

  ```toml
  [capabilities]
  optional = ["outbound-http"]

  [capabilities.outbound]
  hosts = ["https://*:*"]
  ```

  - This declares the dependency for the CLI gate. On Fastly the gate warns,
    because outbound HTTP is only best-effort there.
  - The hosts value must equal `spin.toml`'s `allowed_outbound_hosts`. A core
    test parses both files and asserts that they are equal **(my call)**.
    edgezero's `validate_spin_outbound_host_contract` would need a seventh
    edgezero dependency (`edgezero-adapter` with `cli`), so it is not used.

### 6.2 JWKS cache: portable clock and bound

- **Make the cache a type** whose operations take `now: MonotonicInstant`. The
  static only wraps it.
  - `MonotonicInstant` is `web_time::Instant`, re-exported as
    `edgezero_core::MonotonicInstant`, and it works on `wasm32-unknown-unknown`.
  - The handler passes `ctx.monotonic_clock().now()`.
  - Every adapter's clock defaults to `web_time::Instant::now`, so instants from
    different requests are comparable.
- **Rules:**
  - **Key by the canonical host.**
    - Today the key is the raw `site_domain`: `verify_request_id_signature`
      passes it unchanged at `verification.rs:478`, and `get_cached_jwks` copies
      it at `:144`. Whitespace, case and trailing-dot variants of one host
      therefore each take their own entry.
    - A key can be close to the request-size cap. In a review probe, 128 padded
      aliases of one host held 128 MiB of keys.
    - Fix: call `validate_jwks_host` once, before the cache lookup. It returns a
      trimmed, lowercased, dot-stripped hostname that must pass
      `is_valid_public_dns_hostname`, so it is at most 253 bytes. Use that value
      as both the cache key and the fetch host.
  - Cap the cache at 32 entries **(my call)**.
  - **Owned-allocation charge budget** `MAX_JWKS_CACHE_BYTES = 2 MiB`
    **(my call)**. The cache retains parsed values, so charging the serialized
    response length would undercount many small keys. Charge each entry as:

    ```text
    canonical_host.capacity()
    + jwks.keys.capacity() * size_of::<JwkKey>()
    + sum(key.kid.capacity() + key.x.capacity())
    ```

    - Use checked multiplication and addition. Overflow makes an entry
      uncacheable.
    - This charges owned string and vector allocation capacities, including
      spare capacity. It excludes map buckets, fixed container structs, mutex
      metadata, allocator overhead, and transient fetch/parser buffers or
      request-local clones. The 32-entry cap bounds container population; this
      is not a total-memory or RSS guarantee.
    - Under the insertion lock, remove any existing entry for the same host and
      subtract its charge first. If the replacement alone exceeds the budget
      or its charge overflows, use the fetched JWKS for the current request
      without caching it or evicting unrelated entries.
    - Otherwise remove expired entries, then evict the oldest until both the
      entry cap and charge budget permit insertion. Update the entries and
      charge total atomically under the same lock. Replacements, expiry and
      eviction must subtract the removed entries' charges.
  - Entries are cached before any key lookup or signature check. An attacker who
    controls an HTTPS endpoint can therefore fill the cache with unverified or
    empty key sets without holding a valid signature. The entry cap and the byte
    budget are what bound this.
  - Evict expired entries first, then the oldest.
  - Treat `checked_duration_since` returning `None` as expired.
  - Treat a poisoned lock as a miss. Bypass both cache lookup and insertion
    when the lock is poisoned and emit a debug diagnostic; a successful fetch must still reach signature
    verification without failing on a second lock acquisition.
  - Release the lock before awaiting the fetch, as today.
  - Cache hits clone an `Arc` to immutable key storage instead of deep-cloning
    strings under the mutex. Insertion still charges the owned clone retained by
    the cache; fixed Arc metadata and values retained by in-flight requests are
    outside the cache allocation budget.
- **Why now and not later:** the cache outlives requests today on Axum, the
  Cloudflare isolate and Spin. Spin's WASI P3 components are subject to instance
  reuse. Once the clock is fixed, the Cloudflare cache starts filling, so the
  bound has to land in the same unit as the clock fix.
- **Tests:**
  - Run on a cache instance, never the global, to avoid parallel-test races.
  - Cover:
    - hits inside the TTL, and a refetch after it
    - the entry cap and eviction order
    - the allocation charge budget: exact-boundary insertion, one byte over,
      vector/string spare capacity, and arithmetic overflow
    - a near-64 KiB document with many small keys, whose parsed allocation
      charge exceeds its serialized length
    - replacement, expiry and eviction accounting, and successful verification
      when an uncacheable fetched value is not inserted
    - a clock that goes backwards
    - a poisoned lock, including successful verification after bypassing both
      lookup and insertion
  - **Alias regression test:** whitespace-padded, mixed-case and trailing-dot
    variants of one host, plus one roughly 1 MiB whitespace-padded variant,
    occupy exactly one entry whose key is at most 253 bytes.
- **wasm proof:** an unsigned auction never reaches the cache, so it proves
  nothing here.
  - Add a Cloudflare-crate wasm test, which runs on `wasm32-unknown-unknown`. It
    calls the public `mocktioneer_core::verification::verify_request_id_signature`
    twice, against a `RequestContext` whose extensions hold
    `HttpClient::with_client(stub)`.
  - The stub returns a JWKS for a test Ed25519 key, and the request is signed
    with that key and a fresh timestamp. On wasm32 the freshness check uses
    `js_sys::Date`.
  - The first call fetches and inserts using the web-time clock; the second
    hits the cache. The stub's call count is 1 and both calls verify.
  - This exercises the code path that panics today.

### 6.3 The `configure` hook

Wire it with
`edgezero_core::app!("../../edgezero.toml", MocktioneerApp, configure = crate::configure_app)`.
The positional name with keyword arguments is verified.

```rust
fn configure_app(app: &mut App) -> Result<(), EdgeError> {
    app.set_config_extraction_limits(ConfigExtractionLimits {
        timeout: CONFIG_READ_TIMEOUT, // 2 s
        ..ConfigExtractionLimits::default()
    })?;
    app.set_ingress_admission_policy(|head| {
        let budget = match head.route_resolution() {
            RouteResolution::Matched(route) if route.class() == Some("bid") => BID_READ_BUDGET, // 10 s
            _ => DEFAULT_INBOUND_READ_BUDGET, // 30 s, edgezero's default
        };
        AdmissionDecision::Admit {
            completion: ResponseEgressCompletion::empty(),
            grant: IngressGrant::empty(),
            read_deadline: head.read_deadline_after(budget),
        }
    });
    Ok(())
}
```

- **`class = "bid"`** goes on the three POST triggers only: `/openrtb2/auction`,
  `/e/dtb/bid` and `/adserver/mediate`.
- **The read deadline** is absolute from request start and governs only body
  reads.
  - The auction and APS handlers extract `AppConfig` before the body.
  - The 2 s config timeout lets extraction classify an observed expiry as a
    config-read 503 rather than proceeding to a body-read 408. Config deadlines
    are BestEffort everywhere: a synchronous provider call, including Fastly's,
    can consume more than 2 s before expiry is observed. It is not a guarantee
    that 8 s of the bid budget remains. A test asserts
    `CONFIG_READ_TIMEOUT < BID_READ_BUDGET`.
- **What this changes:**
  - The size limits keep edgezero's defaults.
  - `ConfigExtractionLimits` applies only to `AppConfig` extraction. The
    framework's `/_mocktioneer/config` diagnostic handler
    (`edgezero_core::introspection::config`) reads with an unbounded `get` and
    parses the envelope without these limits; on Cloudflare it takes the
    whole-value read path. That behavior is pre-existing. Mocktioneer keeps the
    framework handler, and the gap is recorded as an upstream exception (§11).
  - Every other route, 404 and 405 keeps edgezero's default behavior:
    immediate canonical JSON 404/405 and a 30 s read budget.
- **Tests:**
  - Each of the three triggers resolves with class `bid`, and no other trigger
    has a class: `build_router().resolve(&method, path).resolution()`.
  - Admission deadlines:
    1. `App::build::<MocktioneerApp>(PlatformMetadata::default())`
    2. `app.begin_ingress(IngressHeadParts::new(..), start)`
    3. match `IngressBeginOutcome::Admitted(prepared)`
    4. check `prepared.read_deadline()` for a bid route and for a GET route
  - A seeded-store `AppConfig` read through admitted dispatch succeeds with the
    configured limits.

### 6.4 Size-capped JSON extractors

- **Auction and APS:** switch to `ValidatedJsonWithin<T, MAX_BID_REQUEST_BYTES>`
  for `OpenRTBRequest` and `ApsBidRequest`, with a cap of 1 MiB **(my call)**.
  - The builder measures the largest request in the Playwright suite and the
    endpoint tests, and confirms that it is well under the cap.
  - Over the cap, the response is 413 `payload_too_large` "request body too large",
    with no `Retry-After` (R10/F14). Malformed JSON still returns 400, and a validator failure is
    still 422.
- **Mediation keeps `ValidatedJson`,** with edgezero's 8 MiB default.
  - Mediation requests embed bidder `adm`, and mocktioneer's own `adm` contains
    the full OpenRTB request plus a response preview.
  - The size therefore grows with impressions × bidders.
- **Tests:**
  - One over-cap test per capped route.
  - The auction test must seed a config registry, because `AppConfig` runs
    first. Without it, the auction returns 500 before the cap applies.

## 7. Wire changes (the complete list)

Every response difference allowed by this migration. Anything else is a
regression.

**Forced by #275:**

| # | Change |
| --- | --- |
| F1 | `EdgeError::internal` 500s render JSON with `message: "internal server error"`. The kind and shape are unchanged. `StoreExtraction` 500s keep their own message. |
| F2 | Config error wording changes. Missing blob uses `; run` instead of `— run`. The deserialization message drops its hint. `ConfigStoreError` maps to fixed text. Status, kind and `Retry-After` for missing-blob, deserialization and validation failures are unchanged. |
| F3 | A declared but unopenable config store returns 503 `service_unavailable` with no `Retry-After` on Cloudflare (no KV binding) and on Axum (unreadable or malformed snapshot). It was 500. Fastly stays 500. An oversized Axum snapshot fails startup. |
| F4 | 405 carries a sorted, deduplicated `Allow`. |
| F5 | Normalized head validation runs before routing, so its error responses carry no CORS headers. Visible target lengths over 8192 bytes return 414; more than 100 normalized headers or 64 KiB of normalized headers returns 431. Framing ambiguity that survives host parsing returns 400. On Fastly and Cloudflare the target length counts the absolute URL. Raw framing rejection is not guaranteed; see below. |
| F6 | An inbound body read past the read deadline returns 408 (default 30 s). |
| F7 | JSON bodies are capped at 8 MiB by default. Over the cap returned 400 at the original pin and returns 413 under R10/F14, with fixed message "request body too large". |
| F8 | Axum now parses JSON bodies without `Content-Type: application/json`, as Prebid's `text/plain` posts send them. They were rejected with 400 on Axum before; the other adapters already accepted them. |
| F9 | Axum is HTTP/1 only. It caps connections at 256 and closes connections with no response after a 10 s head-read or idle timeout. Hyper enforces raw head limits. |
| F10 | Egress strips hop-by-hop headers, canonicalizes `Content-Length`, and applies the 204/205/304/HEAD body rules. Removing/reinserting the length group moves the last application header group into its former slot and appends the canonical length. Duplicate values retain their order; host-added `Date` stays in place. With C6, ordinary empty responses insert length before other application fields, then undergo this same permutation. Responses with no length may stream on Fastly and Cloudflare; §5.8 preserves zero-length framing for all ordinary empty mocktioneer responses. |
| F11 | The JWKS fetch no longer follows redirects. Axum and Cloudflare used to follow them. A redirecting JWKS URL now fails verification closed. |
| F12 | Outbound error details change to the sanitized per-adapter messages in §5.6. Buffered fetching can move body-read and decode failures from `Stream read failed:` to `JWKS fetch failed:`. Send-time framing, size, body and timeout failures take precedence over the non-200 status reason, because the status is checked only after `send` succeeds. Size failures keep the legacy JWKS size-limit reason. Recognized gzip/Brotli/deflate decoding and Axum/Fastly default `Accept-Encoding: identity` negotiation are accepted changes, including formerly invalid encoded JWKS now verifying successfully. The 65536-byte final buffered cap applies after recognized decoding. |
| F13 | `/_mocktioneer/manifest` gains a `capabilities` object, which is always serialized, and `class` on the three bid triggers. |
| F14 | At `d47c8026`, EdgeZero-owned inbound byte overflow returns 413 `payload_too_large` with fixed message "request body too large" and no `Retry-After`. This replaces the original pin's 400 `bad_request`; malformed JSON 400, validation 422, read-expiry 408 and config errors are unchanged. The corpus permits this only on the three named body-cap cases and preserves exact CORS/header/body comparisons. |

**Chosen by this spec:**

| # | Change |
| --- | --- |
| C1 | Bid routes get a 10 s body-read budget instead of 30 s (§6.3). |
| C2 | An observed expiry of the 2 s config extraction deadline returns 503 `service_unavailable`. Config deadlines remain BestEffort (§6.3). |
| C3 | Auction and APS bodies over 1 MiB hit the selected cap (§6.4): originally 400, now 413 under R10/F14. |
| C4 | A poisoned JWKS cache lock is a miss, not a `Cache lock poisoned` failure (§6.2). |
| C5 | The JWKS cache is capped at 32 entries and a 2 MiB owned-allocation charge, and keyed by canonical host (§6.2). An uncacheable value can still verify the current request. |
| C6 | `Content-Length: 0` is set explicitly on every ordinary empty buffered response whose status permits it (§5.8). |
| C7 | The JWKS outbound budget is 5 s. An observed expiry yields a failed-verification reason in `adm`, while the auction still returns 200. A previously successful slower fetch may now fail. Enforcement is Native on Axum and BestEffort on the other adapters (§6.1). |
| C8 | CORS renders matched-route errors with the framework's default `IntoResponse`, then adds the same three CORS fields as successful responses. This includes malformed JSON/queries, validator failures, body-cap 413s (400 at the original pin), body-read 408s, config 503s and matched asset 404s. Unmatched routing 404/405 and normalized-head errors still bypass route middleware. No exposed response headers are added; overflow responses carry no `Retry-After`. |

CORS is the outer route middleware, with RequestLogger inside it so the logger
still observes errors before they are rendered. Registered OPTIONS requests run
their handler and logger before CORS decorates the response; their wire contract
remains 204 with the existing headers.

**F5 scope:** these are checks of the head visible to edgezero, not a raw-wire
parser contract. Raw ingress framing validation is Unsupported on every
adapter. Axum's Hyper parser collapses identical duplicate `Content-Length`
fields and removes `Content-Length` when `Transfer-Encoding` is present before
edgezero sees the request. Such raw inputs need not produce a 400 and can reach
a matched handler with its usual CORS headers. A host may also reject a request
before application dispatch. Characterize these cases separately; enforcing
stricter raw rejection would require upstream work outside this migration.

### 7.1 Config-error matrix (`AppConfig` on the auction and APS routes)

#275 reports `AppConfig` failures as `EdgeError::StoreExtraction { reason }`. The
status and kind come from `StoreExtractionReason::wire_status` and `wire_kind`
(`crates/edgezero-core/src/error.rs:84-127`). `Retry-After: 60` is set only when
the kind is `config_out_of_date` (`error.rs:412-413`). `StoreExtraction`
messages are public, including on 500s; only `Internal`, `BadGateway`,
`GatewayTimeout` and `ResponseTooLarge` get fixed text.

| Trigger in mocktioneer | Reason | Status | Kind | `Retry-After` | vs v0.0.10 |
| --- | --- | --- | --- | --- | --- |
| No config registry | `MissingRegistry` | 500 | `internal` | no | Same status and kind. The message drops "internal error:" and uses `;` instead of `—` (F2). |
| Store bound, key absent | `MissingBlob` | 503 | `config_out_of_date` | 60 | Same. The message uses `;` instead of `—` (F2). |
| `data` doesn't deserialize into `MocktioneerConfig` | `Deserialization` | 503 | `config_out_of_date` | 60 | Same. The message drops its push hint (F2). |
| `bid_cpm` fails the validator | `Validation` | 503 | `config_out_of_date` | 60 | Same. |
| Envelope JSON is malformed | `MalformedEnvelope` | 500 | `internal` | no | Same status. The "internal error:" prefix is gone (F2). |
| `sha256` mismatch | `IntegrityMismatch` | 500 | `internal` | no | As above. |
| `version` ≠ 1 | `UnsupportedVersion` | 500 | `internal` | no | As above. |
| Store unavailable: Cloudflare with no KV binding or no `FinalizationRegistry`, or an unreadable Axum snapshot | `BackendUnavailable` | 503 | `service_unavailable` | no | Was 500, because the registry was dropped (F3). |
| Observed expiry of the 2 s config deadline | `DeadlineExceeded` | 503 | `service_unavailable` | no | New (C2). |
| Fastly store open failure | `BackendFailure` | 500 | `internal` | no | Same status. |
| Blob over 8 MiB, depth over 64, or more than 8192 nodes | `ValueTooLarge`, `StructuralLimit` | 500 | `internal` | no | New. Unreachable with the shipped config. |
| Invalid key | `InvalidKey` | 400 | `bad_request` | no | Unreachable: the key is fixed. |

**Tests:** one core test per reachable row, through admitted dispatch (§5.7).
Each test asserts the status, the kind, whether `Retry-After` is present, and the
exact message.

- Seed a mock store for the store-backed rows.
- For the unavailable row, the mock returns `ConfigStoreError::Unavailable`.
- For the deadline row, use an injected clock (`App::set_monotonic_clock`) that
  the mock's `get_bounded` sees pass the deadline.
- The builder records the exact message strings in the tests.

## 8. Behavior that must not change, and the wire-diff procedure

- **Must not change:**
  - Bid prices, creatives, `adm`, mtkid, and every header and body not covered
    by §7.
  - Validation codes: bad JSON is 400, a validator failure is 422, bad query
    parameters are 400, and `EdgeError::validation` is 422.
  - CORS on successful matched routes, including explicit OPTIONS triggers.
    Matched-route errors gain CORS under C8. Unmatched routing 404/405 and
    normalized-head errors carry no CORS headers before or after.
  - The CI config-push check: `jq '.mocktioneer_config | fromjson | .data.bid_cpm'`
    over `.edgezero/local-config-mocktioneer_config.json` returns 0.35.
- **Wire-diff procedure** (run by the builder, with results attached to the PR):
  1. **Revisions:** create a disposable baseline worktree with
     `git worktree add <baseline-dir> e6c2982`, and a separate disposable
     candidate worktree at the branch head.
     Run each server and CLI from that revision's repository root, using the
     same fixed host and port for both revisions.
  2. **Startup scenarios:** capture both scenarios separately for each revision.
     Stop the server between scenarios; both versions snapshot config at
     startup, so deleting or pushing the file while a process runs cannot
     switch its scenario.

     | Scenario | Config before startup | Requests |
     | --- | --- | --- |
     | Seeded | Copy `mocktioneer.toml.example` to `mocktioneer.toml`, then run `cargo run -p mocktioneer-cli -- config push --adapter axum --yes`. | The main corpus. |
     | Unseeded | Confirm `.edgezero/local-config-mocktioneer_config.json` is absent; do not push config. | Missing-config auction and APS cases, which return 503. |

     Start each scenario with `cargo run -p mocktioneer-adapter-axum` only after
     its config state is prepared. Keep all config files in these disposable
     worktrees; do not remove an operator's existing local config.
  3. **Environment:** remove ambient `FASTLY_*`, `MOCKTIONEER_*` and
     `EDGEZERO__*` overrides from both server environments, then apply the
     fixed values in step 4. Do not bump the crate version; `info_html` embeds it.
  4. **Environment:** pin `MOCKTIONEER_TS_DOMAINS` and `MOCKTIONEER_PULL_TOKEN`
     to the same fixed values in both runs. This lets the successful `/sync/start`
     redirect and `/resolve` paths run.
  5. **Corpus:** commit an explicit request list. The `examples/` scripts are not
     enough: they default to Fastly's port, pipe through `jq`, use `uuidgen`, and
     contain no mediation request, no successful sync or resolve, and no
     successful signed auction. The list must cover every response family:
     - **Basics:** `/` and `/_/sizes`.
     - **`/static/img/{size}`:** a standard size and a non-standard size.
     - **`/static/creatives/{size}`:** a standard size and a non-standard
       size.
     - **`/click`:** with and without parameters.
     - **`/pixel`:** a fixed `pid`, and a missing `pid`.
     - **`/aps/win`.**
     - **`/e/dtb/bid`:** valid, validator failure, malformed JSON, and missing
       config in the unseeded scenario.
     - **`/openrtb2/auction`:**
       - one impression, several impressions, multi-format, a non-standard size
       - with eids and Edge Cookie fields, and unsigned
       - a `trusted_server` block that fails before any JWKS fetch with
         deterministic text, such as missing fields. Exclude anything whose
         reason embeds the time drift.
       - malformed JSON, a validator failure, and missing config in the
         unseeded scenario, which must return 503 from §7.1
     - **`/adserver/mediate`:**
       - one bidder and several bidders
       - a price tie, where the first bidder wins
       - a floor that filters some bids, and one that filters all
       - bids with `adm`, which pass through, and without it, which get a
         generated creative
     - **`/sync/start`:** an allowed domain (302), a disallowed domain (403), bad
       parameters (400).
     - **`/sync/done`.**
     - **`/resolve`:** a valid token (200), a wrong token and a missing token
       (401).
     - **OPTIONS** on every path, a 405 on every path (F4), and an unknown-path
       404.
     - **The F5 overages:** a 9 KB target and more than 100 headers. Test
       framing faults that remain visible in a normalized head separately
       through adapter validation, or explicitly call
       `head.validate_normalized(app.ingress_head_limits())` before admission.
       Admitted dispatch alone skips this validation. Do not assume raw
       requests preserve these faults.
     - **Raw framing characterization:** use a TCP client for identical
       duplicate `Content-Length: 0` fields and for CL plus chunked TE with a
       complete terminating chunk. Record parser acceptance/rejection and the
       observed response for each revision. A high-level HTTP client may
       rewrite these inputs. Do not require a universal 400; F5's normalized
       checks cannot recover framing removed by the host parser.
     - **The size caps:** C3 (a 1.1 MiB auction or APS body) and F7 (a 9 MiB
       mediation body), retaining the successful baseline's CORS headers under C8
       and using 413/no-retry under R10/F14. Also compare the preceding `04c3635`
       implementation's exact 400 body-cap responses against the new 413s.
     - **Matched errors (C8):** compare the exact added CORS fields, preserving
       error statuses, bodies, Retry-After and unrelated headers. Routing/head
       errors must not gain CORS. Controlled-clock tests cover 408/config-expiry
       503 without real-time sleeps.
     - A JSON POST without `Content-Type` (F8).
     - **Excluded:**
       - A successful signed auction. It needs a public JWKS host, because
         `validate_jwks_host` rejects localhost; §5.6 and §6.2 cover it with
         tests.
       - `register_partner.sh` (an external host, needs credentials).
       - The introspection routes (F13).
  6. **Capture:** for every request, record the raw status line, every header
     in order including duplicates, and the raw body bytes. Use for example
     `curl -sS --raw -D <hdrs> -o <body>`, or a small client. Never pass output
     through `jq`.
  7. **Masking:** confine it to generated values. Mask UUIDv7 values only at
     their known emission sites:
     - bid and response ids (`auction.rs:50-51`, `mediation.rs:101-102`)
     - pixel ids (`render.rs:86-87`)

     These ids are 32 hex characters with no dashes. Assert that the mask count
     is equal on both sides. Also mask the `Date` header.

     Never mask mtkid. It is a deterministic `SHA-256("mtkid:" || host)`,
     truncated to 32 hex characters, so a loose 32-hex pattern would hide
     regressions in it.
     **Existing validation-order exception:** the APS empty-slots and mediation
     empty-impressions validator messages contain a `HashMap` parameter display
     whose `min`/`value` order varies even between repeated baseline requests.
     For those two named cases only, the comparator accepts the two exact
     observed message bodies, with their fixed paths, values, status and kind.
     Every header and all other body bytes remain protected. Mutations to either
     parameter, the field name, kind or surrounding text must fail. This is a
     characterization of baseline nondeterminism, not an additional migration
     wire change or permission to normalize arbitrary validation errors.

  8. **Pass condition:** for each startup scenario, the masked diff is empty
     except for responses that the corpus predicts from §7 and §7.1, and each
     difference is annotated with its number. Attach the raw-framing
     characterization separately and identify any host-parser differences under
     F5/F9; do not count it as proof of raw framing validation.

## 9. Verification

**Automated (CI):**

- `cargo fmt`, strict clippy, `cargo test --workspace --all-targets`, the
  `fastly cloudflare` feature check, and `config validate --strict` (the
  `test.yml` invocation).
- **Contract suites:**
  - Fastly under Viceroy 0.21.0.
  - Cloudflare under wasm-bindgen-test-runner 0.2.128, from the lock.
  - Spin under wasmtime 48.0.2.
    - The probe passed on 45.0.0 with plain `wasmtime run`.
    - edgezero's CI now uses `wasmtime run -W component-model-async=y -S p3=y -S http=y`.
    - Adopt those flags in `test.yml` only if 48.0.2 needs them.
- **Playwright:** all tests (41 at base) against a live Axum server.
- **Docs:** lint, format and build. Add `npm run build` to the existing
  `format-docs` PR job in `.github/workflows/format.yml`; the current docs
  deployment workflow builds only after a main push or manual dispatch.
- **The new tests from §5.6, §5.7, §6.1 to §6.4 and §7.1.** This includes the
  Cloudflare wasm JWKS flow test from §6.2, which runs in the Cloudflare
  contract job.

**Required local checks, by the builder, with results in the PR:**

If a required local check fails or cannot run, report verification as incomplete.
Passing CI does not substitute for these checks.

- The §8 wire diff.
- **Docker:**
  - The image builds, as `docker.yml` checks.
  - Manual: the container serves `/` with a 200, and an OpenRTB auction from the
    seeded config returns `price: 0.2`. `docker.yml` itself only checks for a 200.

- **The Fastly production entrypoint:** contract tests call `FastlyService`,
  not `main()`, so they never exercise the new `run_app()` raw-ABI response
  path.
  1. From the repository root, confirm the standalone `viceroy --version`
     reports 0.21.0 and build the wasip1 release:

     ```sh
     cargo build --locked --release -p mocktioneer-adapter-fastly \
       --features fastly --target wasm32-wasip1
     ```

  2. Copy the adapter's `fastly.toml` to a temporary, uncommitted manifest.
     Under `[local_server.config_stores.mocktioneer_config]`, set
     `format = "inline-toml"`; in its `.contents` table, set the
     `mocktioneer_config` key to the serialized pushed envelope. Set
     `MOCKTIONEER_SMOKE_CONFIG` to this temporary manifest's absolute path, then
     start the HTTP listener from the repository root:

     ```sh
     viceroy serve --config "$MOCKTIONEER_SMOKE_CONFIG" --addr 127.0.0.1:7676 \
       target/wasm32-wasip1/release/mocktioneer-adapter-fastly.wasm
     ```

     `viceroy run` executes once and exits; it is not the HTTP server for this
     check. `fastly compute serve` may select a different bundled Viceroy, so it
     does not establish the required 0.21.0 runtime evidence.
  3. Check that `/` returns 200 with its body and `Content-Length`, that an
     auction returns the seeded price, and that `/pixel` returns `image/gif`.
     Also check reachable empty `/sync/start` responses: a malformed hostname
     returns 400, and an allowed hostname returns 302; both carry
     `Content-Length: 0`. Capture the redirect without following it. Use the
     synthetic egress contract cases in §5.8 for 403/401; do not require guest
     environment-variable support to run this smoke.

- **Cloudflare:** `wrangler dev` with the new flags serves an auction from
  seeded config. This is the only check of the real `FixedLengthStream` path
  and of `enable_weak_ref` in workerd, so it is required even when the browser
  contracts pass with their polyfill.
  1. Temporarily add the following to `wrangler.toml`, and do not commit it:

     ```toml
     [[kv_namespaces]]
     binding = "mocktioneer_config"
     id = "local-dev-placeholder"
     ```

  2. Run `cp mocktioneer.toml.example mocktioneer.toml`.
  3. Run `cargo run -p mocktioneer-cli -- config push --adapter cloudflare --local`.
  4. Run `wrangler dev --cwd crates/mocktioneer-adapter-cloudflare`.
  5. Verify an auction's 200 status, seeded price, complete body and matching
     declared length. Check the empty 400 from a syntactically invalid
     `/sync/start` hostname and the empty 302 for an allowed hostname for
     `Content-Length: 0` as well. Capture the redirect without following it.
     No polyfill may be installed in this Workers run.
  6. Record Wrangler and workerd versions and the results. A skipped or failed
     smoke leaves verification incomplete; a browser result is not proof that
     the real Workers path works.

  These are the steps in `docs/guide/adapters/cloudflare.md:58-80`.

**Manual, best effort, with results recorded:**

- **Spin:**
  - `spin up` on Spin 4.1.0 serves `/` with a 200.
  - `mocktioneer-cli build --adapter spin` passes the outbound-hosts drift
    check. Fermyon's `spin build` does not run that check.
  - **Mismatch case:** temporarily change `spin.toml`'s `allowed_outbound_hosts`
    so it differs from `[capabilities.outbound].hosts`.
    `mocktioneer-cli build --adapter spin` must fail with "Spin outbound host
    drift". Then revert.

**Unverified, and recorded as such in the PR:**

- A live outbound JWKS fetch on the Fastly, Cloudflare or Spin runtimes.
  `validate_jwks_host` rejects localhost, IP literals and dotless hosts, so this
  needs a public trusted-server host. The stub-client tests cover mocktioneer's
  logic.

## 10. Release runbook (for whoever releases after #275 merges)

- **Cloudflare:**
  - **Prerequisite: a KV binding.** The committed `wrangler.toml` has no
    `[[kv_namespaces]]`, so the flags alone are not enough: auctions keep
    returning 503 `service_unavailable` (§7.1) until a binding exists.
    1. `mocktioneer-cli provision --adapter cloudflare` creates the namespace and
       writes `binding = "mocktioneer_config"` into the deploy's `wrangler.toml`.
       Production namespace ids stay uncommitted.
    2. Then run `mocktioneer-cli config push --adapter cloudflare`.
  - The `EDGEZERO__STORES__CONFIG__*` Worker vars only rename which binding is
    looked up. They do not create one.
  - Deploy with the new `compatibility_flags`. Without `enable_weak_ref`, every
    `AppConfig` route returns 503 even with a binding. The retained date also
    requires `internal_stream_byob_return_view` for successful bounded KV reads.
- **Fastly:** dynamic backends must stay enabled. This is unchanged from v0.0.10.
  - Without them, the JWKS fetch gets a typed 502 internally. The auction still
    returns 200, and signed requests get a `Failed` reason in `adm`.
  - They are needed only for signed trusted-server verification.
- **Axum and Docker:** restart after `config push`, as before. The Dockerfile
  pushes config at build time, before the server starts.
- **Spin:** Spin CLI 4.1.0 or later.
- **Rollback:**
  - The blob envelope format is unchanged, so a pre-migration build reads blobs
    pushed after the migration. `mocktioneer-cli rollback` works as before.
  - The new wrangler flags and enabled dynamic backends are harmless to the old
    build.

## 11. Risks and follow-ups

- **The draft is moving.** `d47c8026` sits on a draft branch.
  - A force-push leaves it unreferenced, and a later GC breaks `cargo fetch`.
  - The user explicitly requested latest-head adoption on 2026-10-23. R10
    reviews the delta from `f6c708cc`, pins `d47c8026`, and adopts its required
    413 and framing changes. It does not automatically follow subsequent heads.
  - Before adopting any newer revision, review its implementation/API/wire delta,
    update this spec and the plan, and rerun §9 in full.
- **Re-pin when #275 ships:**
  1. Diff the CHANGELOG and public API from the rev to the tag.
  2. Rerun §9, including the wire diff.
  3. Switch `rev` to `tag`.
- **Downstream clients** (Prebid.js, Prebid Server, trusted-server) see the §7
  changes. The ones to call out in release notes:
  - F8: Axum accepts `text/plain` JSON.
  - C1, C3 and F14: the new 413s and 408s on bid routes.
  - C8: browser clients can read matched-route error statuses and bodies through
    CORS, including body-cap, body-read and config failures.
  - F11: no redirects for JWKS.
  - F12 and C7: recognized content decoding and negotiation, buffered JWKS
    error ordering and the five-second outbound budget, which can change
    signature results in `adm`.
- **The Cloudflare `FixedLengthStream` polyfill** could hide a real Workers
  failure. The required local `wrangler dev` check is the completion gate for
  that path; skipped or failed checks leave verification incomplete.
- **Upstream issues to record** for the user to file:
  1. Cloudflare `OutboundCachePolicy::Bypass` throws before compatibility date
     2024-11-11 without `cache_option_enabled`. edgezero's capability table marks
     it Native, and its template uses a 2023 date.
  2. The doc errors in §12.2.
  3. `introspection::config`, which serves `/_mocktioneer/config`, reads
     config with an unbounded `get` and no timeout, bypassing
     `ConfigExtractionLimits`. On Cloudflare it materializes the whole value.
     The handler is pre-existing and framework-owned; a bounded variant belongs
     in core.
- **CI's wasmtime install** uses `wasmtime.dev/install.sh`, which edgezero
  replaced after it broke. This is pre-existing. Copy edgezero's pinned-tarball
  install if 48.0.2 install fails.

## 12. Documentation

### 12.1 Mocktioneer docs to update

- **`CLAUDE.md`:**
  - the toolchain lines: wasmtime 48.0.2, and Spin per §5.1
  - `:117` "validation returns 400" is wrong at both versions; it is 422
- **`AGENTS.md`, `README.md`:** any version or behavior text the cut changes.
- **`docs/api/`:**
  - `index.md`: the error table (F1, F4, F5, F7, F8, C1–C3), with F5 limited
    to normalized-head checks rather than a raw framing guarantee
  - `openrtb-auction.md` (~226-250): the JWKS fetch without redirects, and the
    1 MiB cap; the buffered error precedence and five-second JWKS budget
  - `aps-bid.md`: the 1 MiB cap
  - `sync.md`, `resolve.md`: check every status and redirect statement
- **`docs/integrations/trusted-server.md`:** the JWKS fetch behavior (F11, F12,
  C7), and the distinction between the local cache TTL and provider freshness.
- **`docs/guide/`:**
  - `configuration.md`: the wrangler flags, the outbound capability, and the
    Axum restart, which is not new
  - `architecture.md`, `getting-started.md`
  - `adapters/index.md`, `adapters/axum.md`, `adapters/cloudflare.md`,
    `adapters/fastly.md`
  - `index.md`, `what-is-mocktioneer.md`
- **Capability wording** must match #275's per-adapter matrix:
  - outbound HTTP is BestEffort on Fastly and Native elsewhere
  - inbound read deadlines are Native on Axum and BestEffort elsewhere
  - config-read deadlines and response egress are BestEffort everywhere
  - ingress admission is Native everywhere
  - raw ingress framing validation is Unsupported everywhere; normalized-head
    checks do not strengthen that classification

### 12.2 Errors in edgezero's docs not to copy (at `f6c708cc`)

- The generated Cloudflare template retains the 2023 date with only two flags.
  Bounded KV reads also require `internal_stream_byob_return_view` at that date
  (§5.4). Keep this as a local upstream follow-up; no remote issue was filed.

- `ctx.request()` no longer exists, but several pages still use it:
  - `handlers.md:192-194, 650`
  - `middleware.md:25-26, 108, 143`
  - `adapters/cloudflare.md:184`
  - `adapters/fastly.md:121, 336`
  - `adapters/spin.md:307`
- The `handlers.md:207-214` table lists `request()`, `json()`, `form()`, `body()`
  and an infallible `into_request()`.
- `proxying.md:32` is missing the `?` after `ctx.into_request()`.
- `handlers.md:389` and `adapters/fastly.md:401` use `edgezero_core::Response`.
  The real path is `edgezero_core::http::Response`.
- `adapters/axum.md:258-260` offers a manual `AxumDevServer` fallback. It wraps
  `App::new(router)`, which drops `configure`.
- `CHANGELOG.md:338-339, 385` describe `runtime_env_config` and
  `emit_boot_diagnostics`, which don't exist. `:406-409` gives a stale toolchain.
- `crates/edgezero-cli/src/templates/root/README.md.hbs:48-50` says Fastly reads
  `EDGEZERO__LOGGING__LEVEL` from its config store. It does not.

## 13. Sequencing

One PR against `main`, with one commit per unit. Each commit ends green.

1. **This spec.**
2. **The forced migration (§5):** the repin and lock, the dependency edits, the
   entrypoints, `wrangler.toml`, the handler accessors, the JWKS port and its
   reason-string tests, the test doubles, the contract-suite shims, and empty-body
   framing. Include §5's specified port policy and wire-preservation fixes.
3. **The JWKS cache:** the portable clock and the bound (§6.2).
4. **The JWKS policy and manifest (§6.1).**
5. **The `configure` hook and the bid class (§6.3).**
6. **The size-capped extractors (§6.4).**
7. **Docs, `CLAUDE.md` and the toolchain notes (§12).**

## 14. Left to the builder

- Fine-tuning `JWKS_FETCH_TIMEOUT`, `BID_READ_BUDGET`, `CONFIG_READ_TIMEOUT`
  and `MAX_BID_REQUEST_BYTES` within the reasons given. Any change must keep
  `CONFIG_READ_TIMEOUT < BID_READ_BUDGET` and update the corresponding §7 wire
  contract, tests and documentation together, including C7 if the five-second
  JWKS budget changes.
- Whether to share the test doubles (`MapConfigStore`, `complete_envelope`, the
  stub client) in one test-support module or keep a copy per crate. Prefer the
  smaller diff.
- The exact masking regexes in §8, within its constraints.
- Whether wasmtime 48.0.2 needs edgezero's P3 runner flags.
- Whether `asdf install` tolerates `spin` in `.tool-versions`.

## 15. References (edgezero at `f6c708cc`)

- **CHANGELOG:**
  - the outbound migration table: `CHANGELOG.md:135-139`
  - the `Hooks` and assembly table: `:320-329`
  - the additional hard migrations: `:372-385`
  - earlier PR revisions: `:389-396`
- **Reference consumers:**
  - the demo: `examples/app-demo/`
  - the template: `crates/edgezero-cli/src/templates/core/src/lib.rs.hbs`.
    `configure_app` is at lines 81-117.
  - the reusable fixtures: `tests/fixtures/reusable-app/`
- **Core:** `crates/edgezero-core/src/`
  - `app.rs`: `build`, the setters, `begin_ingress`
  - `ingress.rs`: `AdmissionDecision`, `DEFAULT_INBOUND_READ_BUDGET`
  - `router.rs`: `RouteResolution`, `RouteMetadata::class`
  - `outbound.rs`
  - `config_store.rs`: `ConfigValue`, `get_bounded`,
    `finish_bounded_config_read`, `ConfigExtractionLimits`
  - `context.rs`, `time.rs`, `error.rs`
- **Adapters:** each `crates/edgezero-adapter-*/src/lib.rs`.
- **Guides:** `docs/guide/capabilities.md` and `docs/guide/adapters/overview.md`.
- **Probe evidence:** the throwaway diffs and logs from the R2 compile probe:
  - `probe-wave-forced.diff`: the §5 fixes
  - `probe-final.diff`: including §6 APIs that this revision then trimmed

  They are kept outside the repo, in the session scratchpad.

## Appendix A. Calls to confirm

These change what gets built or how it ships:

1. **Merge timing:** after #275 merges, then re-pin (§3).
2. **`build_app`:** delete it (§5.2), following the user's confirmation that the
   related project does not depend on mocktioneer (§0.1).
3. **PR target:** `main`, so CI runs (§3).
4. **No live deploys** in this PR (§2).
5. **App reuse deferred** (§2). This is the biggest departure from "adopt the
   new APIs".
6. **Dropped:** the memory-envelope check and tight config size limits (§2).
7. **Admission scope:** the 10 s budget on the three bid routes only, with 404
   and 405 left at the default (§6.3).
8. **2 s config read timeout** (§6.3).
9. **1 MiB cap** on auction and APS; mediation stays at 8 MiB (§6.4).
10. **`PlatformDefault` cache policy** instead of `Bypass` (§6.1).
11. **Reason strings:** keep `"Proxy not available"` and the strings in §5.6;
    accept the decoding, negotiation, buffered error-stage and precedence changes
    listed in F12.
12. **JWKS cache:** 32 entries, a 2 MiB owned-allocation charge budget,
    canonical-host keys computed before the lookup, and a poisoned lock
    bypassed for lookup and insertion (§6.2).
13. **The Cloudflare `FixedLengthStream` test polyfill** (§5.7).
14. **Explicit `Content-Length: 0`** on all ordinary empty buffered responses
    whose status permits it (§5.8).
15. **Keep `compatibility_date = "2023-05-01"`** (§5.4).
16. **Internal-kind transport errors** get the fixed detail `internal error` in
    signature reasons, so provider diagnostics can't reach `adm` (§5.6).
17. **The wire diff pins `MOCKTIONEER_TS_DOMAINS` and `MOCKTIONEER_PULL_TOKEN`**
    to fixed values, so the success paths run, and captures separate seeded and
    unseeded server startups (§8).
18. **Five-second JWKS outbound budget** with provider-specific enforcement
    limits (C7).
19. **Required local Cloudflare workerd smoke** despite the browser test
    polyfill (§9).
