# EdgeZero PR #275 Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkboxes for execution tracking.

**Goal:** Migrate all Mocktioneer adapters to the audited EdgeZero #275 APIs, adopt bounded JWKS fetching and caching and route-aware ingress, and demonstrate the specified wire compatibility.

**Architecture:** Keep business logic and policy in `mocktioneer-core`, with thin adapter entrypoints and runtime-specific contract fixtures. Make the forced API cut atomically, then add the bounded cache, manifest contract, admission policy and request caps in independently testable units. Capture the old service before repinning and compare it against the final implementation.

**Tech Stack:** Rust 1.98.1, EdgeZero at an exact Git revision, Axum, Fastly/Viceroy 0.21.0, Workers/workerd, Spin 4.1.0/Wasmtime 48.0.2, wasm-bindgen 0.2.128, Node built-ins, Playwright, VitePress, Node 24.20.0.

---

## Review outcome and execution status

The user approved execution on 2026-10-21 ("can we start?"). Both scope questions are resolved: the related project does not depend on Mocktioneer, and no batch fan-out is needed. Task 0 incorporates the remaining design corrections before the API cut. Implementation and required local verification are complete. The user requested commit/push and issue/PR publication on 2026-10-21. Existing tracking issue [#130](https://github.com/stackpop/mocktioneer/issues/130) and draft PR [#131](https://github.com/stackpop/mocktioneer/pull/131) are reused; upstream merge/release remains a separate gate.

The 2026-10-23 second review adds a JWKS destination-admission decision that
remains open. The previous implementation checkpoints are complete; merge
qualification additionally requires resolving this new security policy.

Design: [2026-10-18-edgezero-pr275-migration-design.md](../specs/2026-10-18-edgezero-pr275-migration-design.md), working revision R12. Baseline application revision: `e6c298216fccd5c04e3c8e5fff6e4b602cb30d11` (PR #118). Worktree: `.claude/worktrees/edgezero-pr275`, branch `feat/edgezero-pr275-migration`.

Review on 2026-10-20 checked the exact proposed upstream pin `f6c708ccb5be19ba9fa92575db3c08c0f17e0d72`, not merely the upstream checkout's HEAD. PR #275 remains draft and unmerged. Its current head, `7cccaf10734914161803317aec6c55c22c2bccc9`, adds seven documentation files relative to that pin; it changes no implementation, dependencies or toolchain. Reconcile the spec's stale head metadata and its repinning rule in Task 0; do not silently change Cargo source identity.

The paragraph above records the original pre-execution review. On 2026-10-23 the
user explicitly requested the latest PR #275 revision. R10 below supersedes the
old pin freeze and adopts observed head `d47c8026a941a2361182c9e2042bebdcb8be4d04`
after reviewing its implementation/API/wire delta. It remains an exact Git pin,
not a moving branch selector or a shipping release.

Review corrections incorporated in spec R6 and the execution-backed R7 corrections are implemented and verified below:

| Finding | Evidence and consequence | Required correction |
| --- | --- | --- |
| **P2: Successful JWKS decoding is absent from the complete wire-change list.** | Spec §5.6 adopts buffered decoding, but §7 F12 accepts only failure details and precedence. Pinned `edgezero-core/src/outbound.rs:1271–1285` decodes gzip, Brotli and deflate. Old Axum enabled no reqwest decompression; old provider proxies did not decode deflate. Encoded valid JSON can therefore change a failed signature result to successful verification. New Axum/Fastly also default `Accept-Encoding` to `identity` when absent. | Extend the accepted outbound change in §7 and the release/docs checklist to include recognized content decoding and negotiation changes. State that the 64 KiB cap applies to the final buffered representation. Test encoded success and decoded oversize without a public host. |
| **P2: The framing-test layer is wrong.** | Spec §8's normalized-head cases say to use admitted dispatch. Pinned `App::begin_ingress` and `dispatch_admitted` do not call normalized-head validation; adapters do so before admission (`edgezero-adapter-axum/src/service.rs:225–240`). Admission alone cannot establish a 400/414/431 rejection or missing CORS. | Exercise the adapter service, or explicitly validate `IngressHeadParts` against `app.ingress_head_limits()` before admission. Keep admitted dispatch for config and deadline tests. Raw TCP cases remain characterization only. |
| **P2: The PR docs-build gate does not exist.** | Spec §9 calls docs lint, format and build automated CI. `.github/workflows/format.yml:77–84` runs only lint and format; `deploy-docs.yml` builds after a main push or manual dispatch. | Add `npm run build` to the existing PR `format-docs` job. Local build success alone does not create a PR gate. |

Execution preflight corrected one review claim: Fastly `ResponseEgressEnvelope::begin()` already invokes shared `prepare_response_egress` and returns a prepared frame. Assert its declared length/body disposition directly; preparing the response a second time is redundant. Envelope completion still does not invoke the private provider writer. The required Viceroy production-entrypoint smoke supplies real delivery evidence.

Review coverage and resulting work:

| Surface | Result / plan coverage |
| --- | --- |
| Git source identity, all six dependencies, lock resolution, wasm-bindgen family | Tasks 0, 2, 9; no downstream Cargo-consumer coordination is required. |
| Macro/hooks, app construction, public helper, request accessors, all four entrypoints | Task 2; no dual-version compatibility layer. |
| JWKS validation, redirects, negotiation, decoding, limits, errors, sensitive diagnostics | Tasks 2–3; add the decoding erratum above. |
| Cache lifetime, canonical keys, capacity charge, overflow, poison, concurrency, wasm clock | Task 3. Concurrent misses may fetch independently as before; no single-flight feature. |
| Admission, normalized heads, read deadlines, routing/fallback/CORS | Task 5; correct test layer above. Raw framing remains Unsupported. |
| Config store ABI, limits, error matrix, extractor ordering, introspection exception | Tasks 2, 5; diagnostic handler remains an explicit upstream exception. |
| Auction/APS caps, mediation default, validation codes and prices | Tasks 1, 6, 8. |
| Bodyless/empty response framing, egress completion, provider fixtures | Tasks 2, 8; distinguish prepared framing from live delivery. |
| Manifest capabilities, outbound host policy, Spin drift, CLI compatibility | Task 4; no seventh EdgeZero dependency. |
| Browser contracts, real workerd, Viceroy main, WASI P3 contracts | Tasks 2–3, 8; browser polyfill cannot qualify workerd. |
| CI, Docker, configuration push/restart, release and rollback, docs | Tasks 7–9; add missing PR build gate above. |

No additional material blocker was established in the other reviewed surfaces. Compile-probe evidence in the spec is useful reference, not a substitute for testing the final branch.

## Decisions and constraints

Scope decisions confirmed by the user on 2026-10-20:

1. **No downstream dependency.** The related project does not depend on Mocktioneer. Delete the unused public `mocktioneer_core::build_app` helper. No cross-project Cargo pin coordination or special stable-commit requirement applies. Do not infer a service/Docker dependency from this answer.
2. **No outbound batch fan-out.** Keep it out of scope; mediation remains in process. Test stubs still implement the required batch trait method with cutoff results.

**Checkpoint handling:** implementation units have green command evidence and independent reviews. Publication uses one verified atomic implementation commit after the existing spec commit, rather than constructing intermediate API-cut trees after implementation. Baseline/candidate sources use isolated snapshots because Git metadata writes were unavailable in the initial sandbox; authorized publication uses elevated Git access.

**Target revision:** freeze implementation at audited `f6c708ccb5be19ba9fa92575db3c08c0f17e0d72`. Fresh preflight observed upstream head `852ec74b`, which changes request overflow to 413 and adds outbound policy/provider behavior. Retaining the approved pin avoids expanding the reviewed wire contract; adopting that newer revision requires a separate delta review and full verification. All six dependencies use the identical audited selector.

Work only in the existing migration worktree. Preserve its existing uncommitted spec edits. Do not touch the parent checkout's unrelated `.claude/settings.json` or Fastly manifest changes. The later publication request authorizes the migration commit/push and updates to issue #130 and draft PR #131. Deployment, upstream edits, remote review comments, merge and history rewrite remain outside scope. Commit steps below are checkpoints for an approved implementation; obey the repository's test-before-commit rule.

Use @superpowers:test-driven-development for new behavior, @superpowers:systematic-debugging for unexpected failures and @superpowers:verification-before-completion before reporting success. Run `cargo test -p mocktioneer-core` after each cohesive Rust edit; run the affected adapter contracts after adapter edits. Keep generated captures, secrets, runtime state and temporary provider manifests outside tracked source. No test needs credentials or a public JWKS server.

## File map

| Files | Responsibility |
| --- | --- |
| This plan; `docs/superpowers/specs/2026-10-18-edgezero-pr275-migration-design.md` | Decisions, review corrections, execution tracking and evidence links. |
| Create `tests/wire-contract/{README.md,requests.json,wire_contract.mjs,wire_contract.test.mjs}` | Reproducible requests, raw capture and narrowly scoped comparison. Captures themselves remain untracked. |
| `Cargo.toml`, `Cargo.lock`, `.tool-versions` | Six matching EdgeZero sources, dependency resolution and runtime versions. |
| `crates/mocktioneer-core/Cargo.toml`, `src/{lib,routes,verification}.rs`, `tests/endpoints.rs` | Shared construction, hooks, caps, fetch/cache policy, error and compatibility tests. No business-logic move into adapters. |
| `crates/mocktioneer-adapter-fastly/{Cargo.toml,src/main.rs,tests/contract.rs}` | Raw-ABI entrypoint and updated contract API/prepared-frame tests. |
| `crates/mocktioneer-adapter-cloudflare/{Cargo.toml,wrangler.toml,tests/contract.rs}` | Version resolution, runtime flags, browser fixtures, real-clock signed JWKS regression. |
| `crates/mocktioneer-adapter-spin/{src/lib.rs,tests/contract.rs}` | Explicit response type and fallible app construction. |
| `edgezero.toml` | Optional outbound capability and exactly three bid classes. `spin.toml` stays unchanged except temporary drift testing. |
| `.github/workflows/format.yml` | Add PR docs build and run wire-harness unit tests. |
| `.github/workflows/test.yml` only if a demonstrated failure requires it | Wasmtime installer or P3 runner flags; retain existing matrix and config-push round trip. |
| `CLAUDE.md`, `AGENTS.md`, `README.md`; documentation enumerated in Task 7 | Accurate commands, versions, API contracts and operational instructions. |

Axum and Cloudflare production entrypoints, the CLI implementation, Dockerfile and provider manifests other than `wrangler.toml` need no planned behavior changes. Change them only if a concrete verification failure demonstrates a migration requirement and review that delta.

## Task 0: Resolve execution gates and repair design assertions

**Files:** design spec and this plan. **No application code.**

- [x] Record the user's §0 answers in the spec and this plan: no dependency from the related project; no batch fan-out. Delete the public helper during implementation and remove downstream Cargo-consumer coordination conditions.
- [x] Freeze the audited f6c708cc implementation pin and record newer upstream 852ec74b as unadopted. All six sources will use the same selector.
- [x] Amend §7 F12 (or add a numbered forced change) for successful decoding/negotiation changes, with the affected old adapters and decoded-size semantics. Update related release-note and §8 exception references together.
- [x] Correct §8's framing test instruction and §5.8/§9's Fastly coverage wording as described above. Preserve the distinction between raw and normalized framing.
- [x] Keep the spec's CI docs-build promise and include its missing workflow change in the implementation scope. Refresh remote-head metadata without confusing it with the audited pin.
- [x] Record implementation approval on 2026-10-21. The spec now reflects all Task 0 design corrections; execute the remaining tasks.

## Task 1: Preserve an executable wire baseline

**Create:** `tests/wire-contract/{README.md,requests.json,wire_contract.mjs,wire_contract.test.mjs}`.

- [x] Establish a writable evidence directory and task-local tools directory. Record command versions, checked-out SHA, tree status and the spec revision there. Use task-specific environment names, never repurpose `HOME` or `CODEX_HOME`.

  ```sh
  export MIGRATION_EVIDENCE="$(mktemp -d /tmp/mocktioneer-pr275-evidence.XXXXXX)"
  export TASK_TOOLS="$MIGRATION_EVIDENCE/tools"
  mkdir -p "$TASK_TOOLS/bin"
  git rev-parse HEAD
  git status --short
  rustc --version
  node --version
  viceroy --version
  ```

  Expect Rust 1.98.1, Node 24.20.0 and Viceroy 0.21.0. Verify installed wasm targets with `rustup target list --installed`. Do not assume asdf shims resolve the versions in `.tool-versions`.

- [x] Prepare the runtime tools used in Tasks 2 and 8. Install/copy wasm-bindgen and its runner **0.2.128** into `TASK_TOOLS/bin`; verify both `--version` outputs. Install Wasmtime **48.0.2** into that tools directory and verify it. One fresh-install option for the wasm tools is:

  ```sh
  cargo install wasm-bindgen-cli --version 0.2.128 --locked --root "$TASK_TOOLS"
  "$TASK_TOOLS/bin/wasm-bindgen" --version
  "$TASK_TOOLS/bin/wasm-bindgen-test-runner" --version
  "$TASK_TOOLS/bin/wasmtime" --version
  ```

  Reusing already verified 0.2.128 binaries is acceptable. On the reviewed machine PATH resolved wasm-bindgen 0.2.121 and Wasmtime 45.0.0; neither proves the required versions. Use a task-local Wrangler install/wrapper under Node 24.20.0 (the existing asdf Wrangler shim failed). Set `WRANGLER_LOG_PATH` inside the evidence directory and `WASM_BINDGEN_BIN` to the matching binary for worker-build. Browser contracts need Chromium/WebDriver as selected by wasm-bindgen-test; record actual runner/browser versions.

- [x] Write meaningful comparator tests before the harness: identical data passes; deterministic mtkid/price/body/header mutations fail; duplicate-header order changes fail; UUID count or cross-reference mismatches fail; an unlisted error change fails even if its status matches an allowed exception. Date and UUID values may differ only at documented sites. Two exact baseline validator-message permutations are additionally allowed for the named APS/mediation cases as documented in spec §8; no other error normalization is permitted. Use Node's built-in test runner, with fixture bytes kept in the test file.
- [x] Run `node --test tests/wire-contract/wire_contract.test.mjs`; confirm the first run fails because the comparator is absent. Implement the minimal capture/comparison code, then rerun and require all tests to pass.
- [x] Implement explicit `capture --base-url URL --scenario seeded|unseeded --out DIR`, `compare --before DIR --after DIR` and `characterize-raw --base-url URL --out DIR` commands. Capture via `curl --http1.1 --raw -sS -D <headers> -o <body>` without following redirects or piping through `jq`. Store the status line, ordered duplicate headers and raw body bytes. Preserve the original captures; normalize only in the comparison view. Use a separate small TCP path for malformed raw framing.
- [x] Build the corpus from spec §8: all success paths in `examples/`, OpenRTB, APS, mediation success and errors, root/static/creative/pixel/click/size/win endpoints, each explicit OPTIONS route, malformed JSON, validator failure, no Content-Type, 404/405, sync cookie/redirect and auth/allowlist paths. Include normalized-head over-limit cases, exact 1 MiB auction/APS and 8 MiB mediation boundaries, 2 MiB mediation success, malformed/validator mediation errors and large-body fallback cases. Exclude external partner registration, public-host signed success and framework introspection as the spec directs. Introspection receives focused tests in Tasks 4–5.
- [x] Give every case a stable ID, startup scenario, explicit request bytes/headers and expected difference number(s) from §7. Do not allow an entire response to differ merely because one field is permitted to change. For example F8 changes Axum's no-Content-Type outcome; F4 permits only canonical `Allow`; F12/decoding changes belong in stub tests unless the local wire fixture can reach that path. Raw cases are a separate report and cannot satisfy F5.
- [x] Mask only `Date` and UUIDv7 values at known auction/mediation response/bid ID and pixel ID emission sites, including their escaped copies embedded in `adm`. Reuse one token for repeated occurrences of the same generated ID and compare occurrence counts. Assert UUID version/variant. Do not mask arbitrary 32-hex values, caller-supplied IDs, signatures or mtkid. Preserve JSON ordering and whitespace during comparison; parsing may locate spans but must not reserialize the whole body.
- [x] Create a disposable baseline checkout at the exact PR #118 SHA. Use a separate target directory to preserve its binary while compiling the migration:

  ```sh
  git worktree add --detach "$MIGRATION_EVIDENCE/baseline" e6c298216fccd5c04e3c8e5fff6e4b602cb30d11
  CARGO_TARGET_DIR="$MIGRATION_EVIDENCE/baseline-target" \
    cargo build --locked --manifest-path "$MIGRATION_EVIDENCE/baseline/Cargo.toml" \
    -p mocktioneer-adapter-axum
  ```

- [x] Run the baseline from its own directory at `http://127.0.0.1:8787`. Strip ambient `FASTLY_*`, `MOCKTIONEER_*` and `EDGEZERO__*` overrides from each server's environment before setting fixed `MOCKTIONEER_TS_DOMAINS=ts.publisher.com` and `MOCKTIONEER_PULL_TOKEN=wire-test-token`. Use a fixed request Host and identical corpus inputs for both versions, without bumping the crate version embedded in root HTML. Capture **two separate server startups**: unseeded (snapshot absent) and seeded (CLI config push before startup, `bid_cpm = 0.20`). Config snapshots are loaded at startup; deleting or editing a file under a running server is not an unseeded test. Stop the old process before binding the next one.
- [x] Run the comparator on baseline versus a second baseline capture. Require zero unexpected differences; this checks the normalization before it can hide a migration regression. Save raw-framing characterization separately.
- [x] Run the harness tests and `cargo test -p mocktioneer-core`, inspect the staged diff, then create a green checkpoint: `test: capture migration wire contract`. Do not commit generated captures or task-local tools.

## Task 2: Make the forced API cut as one green unit

**Modify:** root dependencies/lock/tool versions; core `Cargo.toml`, `src/{lib,routes,verification}.rs`, `tests/endpoints.rs`; Fastly manifest/main/contracts; Cloudflare manifest/flags/contracts; Spin lib/contracts.

The six source pins, trait signatures, entrypoints and test fixtures depend on each other. They form one atomic compile unit. Add regression assertions first where the old API allows it; after the pin changes, compilation failures are the expected initial red state. Do not checkpoint a half-ported tree.

- [x] Add helper tests for ordinary empty statuses 400/401/403/302/500 and 205, forbidden-length 1xx/204, and preservation of 304/HEAD representation lengths. Add exact JWKS reason-string assertions for missing client, non-200, parse and size failures before replacing the fetch path.
- [x] Repin exactly `edgezero-{core,cli,adapter-axum,adapter-cloudflare,adapter-fastly,adapter-spin}` to the agreed URL/revision, preserving existing package/default-feature settings. At the audited pin, one entry is:

  ```toml
  edgezero-core = { git = "https://github.com/IABTechLab/edgezero.git", rev = "f6c708ccb5be19ba9fa92575db3c08c0f17e0d72", package = "edgezero-core" }
  ```

  Bump the Cloudflare wasm test pin from `=0.3.71` to `=0.3.78`. Add Fastly wasm-target dev dependencies on workspace `futures` and `edgezero-adapter-fastly` with `fastly,test-utils`. Resolve the lockfile once (`cargo update` after the manifest edit), inspect the diff against spec §5.1, and avoid unrelated updates. Thereafter use `--locked`.
- [x] Change Wasmtime to 48.0.2 in `.tool-versions`; add Spin 4.1.0 only if the project's asdf setup supports its plugin. Otherwise document the explicit Spin requirement. Confirm the resolved wasm-bindgen/js-sys/web-sys families are single-version:

  ```sh
  cargo tree -p mocktioneer-adapter-cloudflare --target wasm32-unknown-unknown -e normal,dev -i js-sys
  cargo tree -p mocktioneer-adapter-cloudflare --target wasm32-unknown-unknown -e normal,dev -i wasm-bindgen
  cargo tree -p mocktioneer-adapter-cloudflare --target wasm32-unknown-unknown -e normal,dev -i web-sys
  ```

  Expect wasm-bindgen 0.2.128, js-sys/web-sys 0.3.105 and wasm-bindgen-test 0.3.78. Run `cargo fetch --locked`; inspect `cargo tree -d` for duplicate EdgeZero source identities.
- [x] Replace removed `Hooks::build_app` with `App::build::<MocktioneerApp>(platform)?`. Native/core tests use `PlatformMetadata::default()`, Cloudflare service tests use `CLOUDFLARE_PLATFORM`. Delete Mocktioneer's public `build_app` and unused `Hooks as _` import, and migrate every caller to direct app construction. Do not retain a shim to the removed upstream method.
- [x] Change Fastly's wasm main to the raw-ABI entrypoint below; retain the native stub. Remove imports used solely by the old macro. Make Spin's handler return `anyhow::Result<edgezero_adapter_spin::SpinResponse>`. Leave Axum/Cloudflare entrypoints unchanged.

  ```rust
  pub fn main() -> Result<(), fastly::Error> {
      edgezero_adapter_fastly::run_app::<MocktioneerApp>()
  }
  ```

- [x] Add `compatibility_flags = ["enable_request_signal", "enable_weak_ref", "internal_stream_byob_return_view"]` to `wrangler.toml`, retaining `compatibility_date = "2023-05-01"`. No committed production KV IDs. Replace the three `ctx.request()` accesses in routes with `ctx.method()`/`ctx.uri()`.
- [x] Port all four `MapConfigStore` implementations. `get` returns `ConfigValue`; `get_bounded` first returns `ConfigStoreError::DeadlineExceeded` when `deadline.is_expired_at(clock.now())`, otherwise calls `edgezero_core::config_store::finish_bounded_config_read(Ok(self.0.get(key).cloned()), clock, deadline, max_backend_bytes, max_value_bytes)`. Use the map's actual value type/conversion; do not bypass bounded-read completion.
- [x] Port the JWKS request to `ctx.http_client()`, `OutboundRequest::get`, `.timeout(Duration::from_secs(5))`, `.max_response_bytes(65_536_u64)` and explicit `OutboundCachePolicy::PlatformDefault`. Set constants once; timeout and byte cap belong in the forced port because they define send mode and failure ordering. Read a successful 200 with `into_bytes_bounded`, then the existing `serde_json::from_slice::<TrustedServerResponse>`; do not use `json_bounded`. Check status only after send succeeds.
- [x] Implement a small error-detail function: return exactly `internal error` for `err.kind() == "internal"`; otherwise `err.to_string()`. Map `ResponseTooLarge` from either send or defensive drain to the existing size-limit reason. Other send errors use `JWKS fetch failed:`, defensive drain errors use `Stream read failed:`, missing client stays `Proxy not available`. Preserve the full strings in spec §5.6. Delete `collect_jwks_body` and remove core's `futures-util` dependency only if now unused.
- [x] Build a `Send + Sync` stub with `#[async_trait(?Send)]`. Store only reply descriptions (bytes/status/error variants) and captured metadata in mutexes; a `Body` stream or `OutboundRequest` may be non-Send. Construct `OutboundResponse::new(method, status, headers, body, clock, ContentTypeProvenance::Unknown)` inside `send`. Implement `start_batch_until` as `OutboundBatch::cutoff(requests.len())`. Insert `HttpClient::with_client(stub)` into request extensions. No public network access.
- [x] Add tests asserting full reasons for all §5.6 cases, captured GET URL/method, buffered `max_bytes = 65536`, five-second timeout and PlatformDefault policy. Test send-size failure taking precedence over payload-bearing non-200, send-time decode/body error prefix, internal source sentinel redaction, non-200 malformed JSON not parsed, timeout and a distinct custom-stream drain error. A stub returning an error proves Mocktioneer's mapping, not an actual adapter's codec enforcement.
- [x] Add focused decoded-body policy fixtures using public `edgezero_core::outbound::apply_response_body_policy(native: BodyStream, headers: &mut HeaderMap, policy: ResponseBodyPolicy)`, with known gzip/Brotli/deflate byte fixtures and Content-Encoding headers. Construct `ResponseBodyPolicy` from the configured request's `into_parts()` fields (response mode, decoder/window, encoded/decoded and chunk limits). This helper returns a stream; it does **not** perform final buffered collection. Pass its returned stream to `edgezero_core::outbound::collect_response_stream(stream, 65_536_u64).await` to model that stage. Do not use `Body::into_bytes_bounded` here: that inbound helper takes `usize` and emits `BadRequest`, whereas the outbound collector preserves `ResponseTooLarge`. Assert valid encoded JWKS becomes parseable and expanded output above 65536 fails with `ResponseTooLarge`. Use existing dev `futures` for the source stream. Keep encoded fixtures local and small; do not add a runtime codec dependency just to generate tests. Verify adapter negotiation defaults by source inspection and record the accepted change, rather than pretending the custom stub sets adapter headers.
- [x] Make `build_response` add `Content-Length: 0` for an actually empty buffered ordinary response with an allowed status. Preserve existing explicit representation lengths and the 1xx/204/304/HEAD rules. Test the helper, then test framing of synthetic empty 400/401/403/302/500 responses in provider contracts.
- [x] Update Fastly contracts to `capture_egress_for_test` and explicit envelope completion, then inspect the prepared frame returned by `envelope.begin()` (which invokes `prepare_response_egress` internally). Assert `declared_length()`/`transmits_body()`, mark the attempt writing and complete with its clock, then recover the response. Use `block_on` and bounded reads. Cloudflare synthetic routes must pass through `CloudflareService` conversion. Do not add production routes for test-only statuses.
- [x] In Cloudflare browser `test_env_ctx`, add `waitUntil` on the context object and install a conditional `FixedLengthStream` shim only when the global is absent:

  ```js
  globalThis.FixedLengthStream ??= class extends TransformStream {
    constructor(_length) { super(); }
  };
  ```

  Document that this shim does not enforce length. Never install it in workerd. Update Spin contracts to `App::build`.
- [x] Run native/core tests after cohesive edits, then all three target builds/contracts with the Task 8 commands. Require nonzero test counts, successful empty-response assertions and no stale runner mismatch. Run format and strict clippy before the green checkpoint `refactor: migrate to edgezero outbound and lifecycle APIs`.

## Task 3: Make the JWKS cache portable and bounded

**Modify/test:** `crates/mocktioneer-core/src/verification.rs`; Cloudflare `Cargo.toml` and `tests/contract.rs`.

- [x] Introduce private cache tests before changing implementation. Use a local cache and controlled `MonotonicClock`, not sleeps or mutation of the global cache. Tests must cover TTL-minus-one and exact TTL, backwards time, canonical aliases, 32-entry eviction, equal-time deterministic tie breaking, byte charge, replacement, expiry, oversized replacement, overflow and poison bypass through successful signature verification.
- [x] Use these private data/interface boundaries; keep the existing fetch result and public verifier API:

  ```rust
  struct CachedJwks {
      fetched_at: MonotonicInstant,
      jwks: JwksResponse,
      charge: usize,
  }

  #[derive(Default)]
  struct JwksCache {
      entries: HashMap<String, CachedJwks>,
      charged_bytes: usize,
  }

  fn allocation_charge(host_capacity: usize, jwks: &JwksResponse) -> Option<usize> {
      let vector_bytes = jwks.keys.capacity().checked_mul(std::mem::size_of::<JwkKey>())?;
      jwks.keys.iter().try_fold(host_capacity.checked_add(vector_bytes)?, |sum, key| {
          sum.checked_add(key.kid.capacity())?.checked_add(key.x.capacity())
      })
  }
  ```

  `get(&mut self, host: &str, now: MonotonicInstant) -> Option<JwksResponse>` expires an entry if `checked_duration_since` is `None` or age is at least TTL. `insert(&mut self, host: String, jwks: JwksResponse, now: MonotonicInstant)` owns the actual stored value and computes its charge. Clones can change spare capacity: do not charge one allocation layout and store another.
- [x] Factor the private loader to accept `&Mutex<JwksCache>` and `&MonotonicClock`; the production wrapper supplies the static and `ctx.monotonic_clock()`. Canonicalize once with `validate_jwks_host` before lookup and use that canonical host for both key and HTTPS fetch. Preserve signature-payload host semantics. Capture insertion time after the awaited fetch, as the old implementation did.
- [x] Implement insertion under one lock: remove old same-host entry and subtract its charge; if incoming charge overflows or exceeds 2 MiB, return without evicting unrelated entries. Otherwise remove expired/backwards-time entries and evict oldest until both byte and 32-entry limits fit. Break equal fetched-at times by hostname. Update entry and total together with checked arithmetic. Drop the lock before fetching. A poisoned lock bypasses lookup and insertion, including poisoning that happens during a fetch; the fetched value can still verify the current request.
- [x] Test string/vector spare capacity directly on the stored value; use an arithmetic helper accepting size inputs for overflow rather than impossible allocations. Include near-64 KiB JSON with many small keys whose owned parsed charge exceeds serialized length, exact-budget insertion, one-byte-over bypass, and charge totals after replacement/expiry/eviction. Scope assertions to owned allocations; no RSS or allocator-overhead claim.
- [x] Test a roughly 1 MiB whitespace-padded alias plus case/trailing-dot aliases: one canonical entry, host at most 253 bytes, one fetch within TTL. Exercise oversized-value and poisoned-cache successful verification, not merely cache-method return values. Replace the old global-cache-seeding native test with local state/stub coverage.
- [x] Add target-specific Cloudflare dev dependencies `base64 = { workspace = true }` and `ed25519-dalek = { workspace = true }`. Add `wasm_jwks_fetch_then_cache_hit_verifies_signature` in the browser contract suite. Use a fixed signing seed, a unique public-shaped hostname, fixed ID/kid, and a fresh timestamp from `worker::js_sys::Date::now()`. Convert to `u64` without a denied `as` cast.
- [x] Sign precisely ordered JSON `version,kid,host,scheme,id,ts`. Use a fixed-format literal for constant string fields plus the numeric timestamp, or a declaration-ordered Serialize struct; ordinary `json!().to_string()` sorts object keys. Stub a matching JWKS; call public `verify_request_id_signature` twice through a real-web-clock `RequestContext` with the stub extension. Both calls must verify and the call count must be 1. Do not clear the process-global cache. Request extensions do not override the context clock; clock-controlled TTL tests remain in the private loader fixture.
- [x] Run `cargo test --locked -p mocktioneer-core` and the Cloudflare target suite. Require the named wasm regression actually to run. Check no `std::time::Instant` or Tokio was introduced in core. Green checkpoint: `fix: bound jwks cache with portable monotonic time`.

## Task 4: Declare outbound requirements and detect manifest drift

**Modify/test:** `edgezero.toml`, core `src/lib.rs` tests (or existing manifest-test location).

- [x] Add a failing test parsing the committed `edgezero.toml` and `crates/mocktioneer-adapter-spin/spin.toml` with the existing `toml` dev dependency. Assert exact equality of capability hosts and Spin allowed hosts, presence of optional `outbound-http`, and absence of a required declaration. A deliberate differing host fixture must fail equality. Do not depend on a seventh EdgeZero crate.
- [x] Add exactly:

  ```toml
  [capabilities]
  optional = ["outbound-http"]

  [capabilities.outbound]
  hosts = ["https://*:*"]
  ```

- [x] Run core tests and `cargo run --locked -p mocktioneer-cli -- build --adapter spin`. Expect capability validation, matching-host validation and successful build; Fastly's BestEffort warning is expected, not a reason to mark the capability required.
- [x] In an isolated copy or with a reliably restored temporary edit, change Spin's allowed hosts, rerun the same Mocktioneer CLI command and require a nonzero result containing `Spin outbound host drift`. Restore and rerun successfully. `spin build` alone cannot test this gate. Save both outputs.
- [x] Inspect `/_mocktioneer/manifest` separately: the capabilities object is serialized and matches the declaration. After Task 5 it exposes only the three intended bid classes. Green checkpoint after tests: `feat: declare jwks outbound capability`.

## Task 5: Install admission and config extraction policy

**Modify/test:** core `src/lib.rs`, `src/routes.rs`, `tests/endpoints.rs`; `edgezero.toml`.

- [x] Add failing tests that build the app, assert exactly the three POST routes have class `bid`, and check read deadlines from a fixed request start: 10 s for matched bid routes, 30 s for GET/OPTIONS/unmatched/method mismatch. Test immediate JSON 404/405 without consuming a failing body, and sorted/deduplicated `Allow` on 405. Do not use `router().oneshot` for app policy.
- [x] Define `CONFIG_READ_TIMEOUT = 2 s` and `BID_READ_BUDGET = 10 s`; assert the former is less than the latter. Wire the fallible configure hook through:

  ```rust
  edgezero_core::app!("../../edgezero.toml", MocktioneerApp, configure = crate::configure_app);
  ```

  Use the full `configure_app` implementation from spec §6.3: default config byte/depth/node limits with only timeout changed, always `AdmissionDecision::Admit`, empty completion/grant, and a deadline selected from the matched route class. No custom error renderer, retained-app adoption, body-before-fallback or memory-envelope policy.
- [x] Set `class = "bid"` on `openrtb_auction`, `aps_bid` and `adserver_mediate` only. No OPTIONS classes. Preserve `AppConfig` before the body extractor on auction and APS.
- [x] Add an endpoint-test helper that builds the configured App, installs the controlled clock, calls `begin_ingress`, matches `IngressBeginOutcome::Admitted`, invokes `dispatch_admitted` and completes the envelope. Share it within the existing test file; avoid a new cross-crate test-support package. Successful config extraction must use the app's limits and clock.
- [x] For normalized-head assertions use `head.validate_normalized(app.ingress_head_limits())` **before** admission, or the real adapter service. Assert over-8192 visible target => 414, over-100/64 KiB normalized headers => 431 and surviving framing ambiguity => 400, all without CORS. Normalization removed by a host cannot be recovered. Never label admitted dispatch alone as framing validation.
- [x] Implement the entire reachable config-error table in spec §7.1 for both AppConfig bid routes, using mock stores and well-formed/corrupt envelopes. Assert exact status, kind, full message and Retry-After presence/value:

  | Fixture | Expected |
  | --- | --- |
  | No registry | 500 internal, no Retry-After |
  | Bound store, absent key | 503 config_out_of_date, Retry-After 60 |
  | Wrong config data type; validation failure | 503 config_out_of_date, Retry-After 60 |
  | Malformed envelope; integrity mismatch; unsupported version | 500 internal, no Retry-After |
  | Store `Unavailable` | 503 service_unavailable, no Retry-After |
  | Clock advanced past config deadline during bounded read | 503 service_unavailable, no Retry-After |
  | Generic backend failure | 500 internal, no Retry-After |
  | Value size-limit failure | 500 internal, no Retry-After |
  | JSON structural-limit failure (depth and node count) | 500 internal, no Retry-After |

  Use typed store errors for backend failures and `ConfigStoreError::ValueTooLarge` for the value-size row. There is no `ConfigStoreError::StructuralLimit`: supply real envelope JSON under the byte cap but over depth 64, and a separate fixture over 8192 nodes, to reach the extractor's structural scanner. Test envelope corruption through real parsing. Recompute integrity for fixtures intended to reach deserialization or validation. Invalid key is unreachable because the key is fixed; do not invent production code to reach it. Copy exact messages from the audited error implementation into assertions, rather than constructing expected strings with the same renderer under test.
- [x] Test observed config expiry versus body expiry with an injected app clock and a bounded mock store that advances it: config expiry yields 503; after successful config extraction an observed bid-body expiry yields 408. Do not sleep. Also test a seeded normal request succeeds and original missing-config precedence remains ahead of malformed/oversized body errors. Keep provider enforcement classified BestEffort.
- [x] Keep `/_mocktioneer/config` unchanged and document its unbounded framework-owned read exception; do not claim these limits cover introspection. Run core/endpoint tests and green-checkpoint `feat: configure bid ingress and config deadlines`.

## Task 6: Cap auction and APS JSON without shrinking mediation

**Modify/test:** core `src/routes.rs`, `tests/endpoints.rs`.

- [x] Measure and record the largest existing endpoint/Playwright bid request. Confirm it is well below 1 MiB before changing the extractors.
- [x] Add seeded tests for each capped route at 1 MiB exactly and one byte above, malformed JSON, validator failure and a JSON request without Content-Type. Pad a valid minimal payload with JSON whitespace to hit exact byte boundaries. Seed config for both auction and APS so AppConfig does not mask the body result.
- [x] Define `MAX_BID_REQUEST_BYTES: usize = 1024 * 1024` (use the const type required by the pinned extractor), and replace only those two body extractors with `ValidatedJsonWithin<T, MAX_BID_REQUEST_BYTES>`. Keep AppConfig first. Leave mediation's `ValidatedJson` at the upstream 8 MiB default.
- [x] Require exact-cap valid payloads to succeed, over-cap => 400 bad_request with `request body too large`, malformed JSON => 400, validator failure => 422 and Axum no-Content-Type valid JSON => normal success. Add a mediation payload over 1 MiB but below 8 MiB that succeeds and one over 8 MiB that returns the default-size error. Preserve bidding/mediation ordering, prices and creatives.
- [x] Run core/endpoint tests and the scoped Playwright suite if fixtures changed; green checkpoint `feat: cap auction and aps request bodies`.

## Task 7: Make documentation and CI match the implemented contract

**Modify:** `.github/workflows/format.yml`; `CLAUDE.md`, `AGENTS.md`, `README.md`; `docs/api/{index,openrtb-auction,aps-bid,sync,resolve}.md`; `docs/integrations/trusted-server.md`; `docs/guide/{configuration,architecture,getting-started,index,what-is-mocktioneer}.md`; `docs/guide/adapters/{index,axum,cloudflare,fastly}.md`.

- [x] Add this step after existing docs lint/format in the `format-docs` PR job:

  ```yaml
  - name: Build documentation
    working-directory: docs
    run: npm run build
  ```

  Add `node --test tests/wire-contract/wire_contract.test.mjs` to an existing suitable PR job so the committed wire comparator remains checked. Do not add network captures to ordinary unit CI.
- [x] Update tool versions, the fallible App construction examples, request accessors and the incorrect “validation returns 400” statement (validator failures are 422). Keep AGENTS as the short pointer to CLAUDE; change its fallback only when necessary. Do not copy the known stale EdgeZero examples listed in spec §12.2.
- [x] Document 1 MiB auction/APS and 8 MiB mediation caps; 10 s bid/30 s default read budgets; 2 s observed config-read expiry; exact config error statuses and Retry-After; Axum HTTP/1 limits and no-Content-Type acceptance. Describe normalized-head limits without promising raw framing rejection.
- [x] Document no JWKS redirects, five-second observed outbound budget, final buffered 64 KiB cap, successful decoding/negotiation changes and error precedence/sanitization. Explain canonical 32-entry/2 MiB owned-allocation charging and ten-minute local residency, including that provider caching can refill stale keys. Do not present the budget as total memory or guaranteed rotation freshness.
- [x] Document Cloudflare flags and required KV binding/config push, unchanged Axum restart requirement, Fastly dynamic backends and Spin version/host-drift gate. The committed Worker config remains free of production namespace IDs. Include rollback compatibility and the pending upstream introspection/cache-bypass/docs issues without filing or sending them remotely.
- [x] Audit capability language: outbound HTTP BestEffort on Fastly/Native elsewhere; inbound deadlines Native on Axum/BestEffort elsewhere; config deadlines and egress BestEffort everywhere; ingress admission Native everywhere; raw framing Unsupported everywhere. A stub test or browser shim does not upgrade a capability.
- [x] In `docs/`, run `npm ci`, `npm run lint`, `npm run format`, and `npm run build`, using the package's actual scripts (`npm run format` is the repository check command). Inspect for unintentional reformatting. Run harness tests and `cargo test -p mocktioneer-core` before checkpoint `docs: describe edgezero lifecycle migration and gate docs build`.

## Task 8: Prove the final branch across services and runtimes

**Evidence only**, except focused fixes justified by a failing check. For every required check record command, revision, tool version, result and log path. Do not mark this task complete if any required check is unavailable. A changed dependency pin or behavior after this task invalidates the affected evidence and requires rerunning it.

- [x] Run all native gates from the migration worktree:

  ```sh
  cargo fetch --locked
  cargo fmt --all -- --check
  cargo clippy --workspace --all-targets --all-features -- -D warnings
  cargo test --workspace --all-targets
  cargo check --workspace --all-targets --features "fastly cloudflare"
  cp mocktioneer.toml.example mocktioneer.toml
  cargo run -p mocktioneer-cli -- config validate --strict
  ```

  All must exit zero. Preserve a preexisting ignored app-config file before copying and restore it afterward.
- [x] Reproduce the CI nondefault config push in an isolated working directory/state, backing up any existing snapshot. Write `bid_cpm = 0.35` to a temporary TOML file, then:

  ```sh
  cargo run --locked -p mocktioneer-cli -- config push --adapter axum --yes --app-config "$MIGRATION_EVIDENCE/seed.toml"
  test "$(jq -r '.mocktioneer_config | fromjson | .data.bid_cpm' .edgezero/local-config-mocktioneer_config.json)" = "0.35"
  ```

  Require success. Remove only the snapshot created by this check or restore its previous contents. Restore the corpus's seeded price 0.20 before wire/service tests.
- [x] Build and run the exact target contract suites, using verified runtime paths:

  ```sh
  CARGO_TARGET_WASM32_WASIP1_RUNNER='viceroy run' \
    cargo test --locked -p mocktioneer-adapter-fastly \
    --features fastly --target wasm32-wasip1 --test contract

  CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER="$TASK_TOOLS/bin/wasm-bindgen-test-runner" \
    cargo test --locked -p mocktioneer-adapter-cloudflare \
    --features cloudflare --target wasm32-unknown-unknown --test contract

  CARGO_TARGET_WASM32_WASIP2_RUNNER="$TASK_TOOLS/bin/wasmtime run" \
    cargo test --locked -p mocktioneer-adapter-spin \
    --features spin --target wasm32-wasip2 --test contract
  ```

  Require nonzero executed tests and the named signed JWKS-cache regression in Cloudflare output. If Wasmtime 48.0.2 needs P3 flags, adopt `-W component-model-async=y -S p3=y -S http=y` only after reproducing that need on the actual artifact, and update CI identically. If its install script fails, use an official pinned release archive and verify the version; don't silently use 45.0.0.
- [x] From `tests/playwright`, run `npm ci`, `npx playwright install --with-deps chromium`, `npm test`. Require every discovered test to pass (41 at baseline). Run docs lint/format/build and wire-harness unit tests once on the final tree.
- [x] Create a separate disposable candidate checkout at the final implementation commit, with its own target/config state, just as for the baseline. Run its Axum server and CLI from that checkout's root, leaving the developer's original ignored config untouched. Run the final corpus using Task 1's exact host, port, scrubbed environment and two separate startup scenarios. Commands for each captured scenario are:

  ```sh
  node tests/wire-contract/wire_contract.mjs capture --base-url http://127.0.0.1:8787 --scenario seeded --out "$MIGRATION_EVIDENCE/after/seeded"
  node tests/wire-contract/wire_contract.mjs compare --before "$MIGRATION_EVIDENCE/before/seeded" --after "$MIGRATION_EVIDENCE/after/seeded"
  ```

  Repeat with `unseeded`. Require no differences beyond the case-specific accepted wire changes, with each difference identified by number and exact field. Attach raw TCP characterization separately. Do not pass unexpected differences by broadening masks or replacing byte comparison with semantic JSON equality.
- [x] Build and run Docker locally, using a disposable container and host port. Follow `Dockerfile`/`docker.yml`'s actual build context/options. Assert root 200 and nonempty body, seeded auction 200 and price 0.20; stop/remove only the task's container. Docker socket access was denied in the review sandbox; this remains **required/incomplete** until an accessible authorized runner produces the evidence. Do not substitute `cargo build` or a CI image-build-only result.
- [x] Exercise the **Fastly production main**, not only contracts:

  ```sh
  cargo build --locked --release -p mocktioneer-adapter-fastly --features fastly --target wasm32-wasip1
  viceroy serve --config "$MOCKTIONEER_SMOKE_CONFIG" --addr 127.0.0.1:7676 \
    target/wasm32-wasip1/release/mocktioneer-adapter-fastly.wasm
  ```

  `MOCKTIONEER_SMOKE_CONFIG` must be an absolute path to a temporary copy of the adapter's `fastly.toml`, with `[local_server.config_stores.mocktioneer_config] format = "inline-toml"` and `.contents.mocktioneer_config` set to the serialized pushed envelope. Verify Viceroy 0.21.0. Assert `/` 200/body/length, seeded auction price, GIF pixel and reachable empty `/sync/start` 400 (invalid host) and 302 (valid host) with `Content-Length: 0`; don't follow redirects. The synthetic prepared-frame tests cover 401/403/500, without assuming arbitrary guest env inheritance. `viceroy run` is not an HTTP listener.
- [x] Exercise **real Cloudflare workerd**. Back up and temporarily add the spec §9 local KV binding, named exactly `mocktioneer_config`, to the Worker manifest. Push the default app config with `cargo run --locked -p mocktioneer-cli -- config push --adapter cloudflare --local`; run `wrangler dev --cwd crates/mocktioneer-adapter-cloudflare` with the flags and matching worker-build binary. Assert full seeded auction body, price 0.20 and declared length; root/pixel delivery; empty sync 400/302 with zero length. Record Wrangler and workerd versions and restore the manifest after stopping the process. No FixedLengthStream polyfill is permitted. This check is mandatory even if all browser contracts pass.
- [x] Best effort: run Spin 4.1.0 `spin up` with seeded local config and check root 200; retain pass/failure details. The target Wasmtime contract and Mocktioneer CLI host-drift checks remain required independently. Do not weaken them if the live Spin host is unavailable.
- [x] Record live provider outbound JWKS as **unverified** unless separately authorized/available with a public trusted host. Localhost/IP hosts are intentionally rejected. Stub/codec tests prove application policy; they do not qualify provider networking. No external credentials or outbound public test server are required by this plan.
- [x] Inspect the final diff for new dependencies, direct `http` imports, core Tokio/std-Instant usage, legacy `:id`, credentials, temporary bindings and generated artifacts. Confirm the Rust/CLI workflows and new docs-build step cover the branch's PR against `main`. Summarize passed, failed, unavailable and best-effort checks separately.

## Task 9: Review, release gates and rollback handoff

**Modify:** design/plan completion notes and the project's existing release documentation as appropriate. No deployment or merge is executed by this task.

- [x] Request independent review of runtime/core, provider/build and wire-contract changes using the repository's multi-agent workflow. Have a verification reviewer inspect actual logs and counts. Resolve concrete defects and rerun affected checks; don't treat this plan's review as review of unwritten code.
- [ ] Target the PR at `main` so existing workflows trigger after prerequisite #118 lands. Publication encountered GitHub's stack-membership restriction and preserves the existing base. The PR description identifies its dependency, complete local verification, and pending GitHub/release checks.
- [ ] Keep merge/release blocked until upstream #275 merges and all required evidence exists. At release repinning, diff the audited implementation/API/CHANGELOG against the shipping tag, update all six pins/lock consistently, and rerun the entire Task 8 matrix, including wire comparison. No cross-project Cargo selector coordination is required.
- [x] Include the operating checklist: Cloudflare provision real KV binding then config push and deploy flags; Fastly dynamic backends; Axum/Docker restart after config push; Spin 4.1+; unchanged blob-envelope format and config rollback compatibility. Preserve the pre-migration artifact/config capture so rollback is concrete. Do not include provider IDs or credentials in the PR.
- [x] List accepted wire changes (including successful decoded JWKS behavior), unverified hosted outbound behavior and upstream follow-ups. Mark the migration complete only after required checks pass; leave an unavailable required runtime/Docker check open explicitly.

## Plan validation record

Independent plan review on 2026-10-20: **Approved**, with no remaining material issues after correcting the outbound collection fixture and distinguishing real JSON structural-limit fixtures from typed value-size errors. Runtime, outbound/cache and build/operations reviewers supplied the preceding design findings. The chosen implementation keeps core policy centralized and uses existing provider test hooks, avoiding upstream edits or new production test routes.

Document checks: Markdown parse, all nine shell-block syntax checks, fence/link checks, the Task 0–9 sequence, and existence of all 38 referenced existing implementation/documentation paths. The user's scope clarification has been incorporated into the plan and spec. Application tests and deployment/runtime checks above are future implementation requirements, not results obtained by writing the plan. Execution was approved on 2026-10-21 and Task 0 design corrections are complete.


## Execution evidence (2026-10-21)

- Evidence directory: `/tmp/mocktioneer-pr275-20261021.YgmPJO`.
- Baseline: 151 unit + 12 APS integration + 9 endpoint tests passed (172 total).
- Baseline source uses an exact-SHA `git archive` snapshot because workspace permissions deny creating `.git/worktrees` metadata. The archive preserves the reviewed application files without changing repository metadata.
- Source/adapter proposal agents independently confirmed the audited-pin strategy; latest upstream wire changes are deferred to a separate adoption review.

- Task 1 implementation: 85 seeded + 2 unseeded cases; 25 comparator regressions pass, independent spec and quality reviews pass. Expanded baseline/repeat captures at `baseline-v4/{seeded,unseeded}` and `baseline-repeat-v4/{seeded,unseeded}` self-compare successfully. Raw characterization is saved under `baseline-raw`.
- Baseline validation nondeterminism: APS and mediation derive validators render fixed `min`/`value` parameters in either order. Spec §8 and the comparator now permit only the two exact observed bodies for those named cases, with mutation guards. Auction uses its deterministic custom media validator for the 422 case.
- Forced port / cache checkpoints: native verification 44 tests pass; Fastly 4 contracts and Spin 2 contracts pass. Cloudflare 5 wasm contracts pass, including signed fetch followed by a cache hit, using wasm-bindgen runner 0.2.128 with cached Firefox 155.0 selected by task-local WebDriver configuration. The installed Firefox 158.0 cannot start while a 158.0.1 update is pending; no global browser state was changed.
- Endpoint policy fixtures first reproduced missing clock/head enforcement through routing-only dispatch, then passed after using normalized validation, admission and `dispatch_admitted`. All reachable config-error cases are asserted on both AppConfig routes, including exact messages/Retry-After; controlled-clock expiry distinguishes config 503 from body 408.


### Final verification and execution corrections

- Native workspace: **198 tests passed** (197 core/integration plus one CLI), strict workspace Clippy, rustfmt, feature check, and locked dependency fetch passed. Logs: `workspace-test-final.log`, `clippy-final.log`, `fmt-final.log`, `features-final.log`, `fetch-final.log`.
- Config: strict default validation passed. Isolated config push round-tripped **0.35**, then wire/runtime fixtures used **0.20**. The developer's original ignored config was not overwritten. Logs/snapshot: `config-validate.log`, `config-roundtrip.log`, `config-roundtrip-snapshot.json`.
- Final target suites: **Fastly 4**, **Cloudflare 5**, **Spin 2** tests passed. Cloudflare output includes `wasm_jwks_fetch_then_cache_hit_verifies_signature`. Runtime paths and task-local Firefox selection are recorded above; no Wasmtime P3 flags were needed.
- Playwright: **41 passed**. Docs: fresh offline npm install, ESLint, Prettier and VitePress build passed; the final rerun after the compatibility/docs corrections passed. Wire comparator: **25 tests passed**, including two red/green framing-permutation regressions and an explicit-C6 guard regression.
- Final wire evidence: baseline `baseline-v4/{seeded,unseeded}`, independent repeats `baseline-repeat-v4/{seeded,unseeded}`, candidate `after-v3/{seeded,unseeded}`. All **85 seeded + 2 unseeded** comparisons and baseline repeat comparisons pass. Raw framing is separately recorded at `baseline-raw` and `after-v3/raw`. Earlier v3 baseline captures were accidentally unseeded and are retained as failed fixture evidence; they are not acceptance evidence.
- F10 execution correction: shared framing removes/reinserts Content-Length, using HeaderMap's exact last-group swap permutation. The corpus now explicitly declares F10. Comparator tests protect duplicate order, values, cookies, unrelated order and body bytes; no general header sort was introduced. C6 alone permits newly explicit zero lengths. Independent review found and fixed an F10-only scope leak in that existing branch.
- Existing baseline endpoint auction fixture is **57 bytes**; Playwright sends no bid POST bodies. Both are far below 1 MiB. Measurement: `existing-request-size.json`.
- Introspection separately asserts the exact capability declaration and only `openrtb_auction`, `aps_bid`, `adserver_mediate` carry `class = "bid"`, each POST. Evidence: `manifest-assertions.json`.
- Docker **linux/amd64** built and ran: root 200, seeded auction 200/0.20, matching body lengths. Viceroy **0.21.0 production main** passed root, auction, GIF and empty 400/302 delivery. Disposable container and servers were stopped. Evidence: `task8-summary.json`, `task8-docker/`, `task8-fastly/`.
- Spin **4.1.0**: Mocktioneer CLI positive build passed; isolated mismatched hosts failed with `Spin outbound host drift` and the fixture was restored. Live Spin root 200 with matching length passed. Evidence: `task8-spin/`.
- Real Cloudflare/workerd passed all five routes without a FixedLengthStream shim. The retained 2023 compatibility date needed one additional flag, `internal_stream_byob_return_view`: adding only it changed the same seeded auction from 503 to 200/0.20. Public Wrangler HTML rewriting strips root Content-Length; direct workerd origin returned identical full HTML with matching Content-Length. Pixel/auction and empty 400/302 lengths passed through Wrangler. Evidence: `cloudflare-smoke/final-results.json` and `final-verification.log`.
- Cloudflare cleanup removed the temporary KV binding, restored original app-config absence and archived task-owned local state. The committed manifest keeps only the three required flags and no namespace IDs. Correct direct dev invocation uses `wrangler dev --cwd crates/mocktioneer-adapter-cloudflare` so worker-build runs in the package directory.
- Independent reviews covered the API/build delta, JWKS/cache spec and quality, wire harness spec and quality, and remaining runtime/docs/CI. The final runtime review found one CORS documentation overstatement; it is corrected. No material runtime defect remains in those reviews.
- Hosted outbound JWKS behavior remains **unverified**, as explicitly scoped. Local emulators and stubs do not promote provider capability guarantees. The audited upstream pin is not a shipping release; upstream merge and shipping-tag delta review remain open.

### PR handoff

**Title:** Migrate Mocktioneer to EdgeZero's bounded request and outbound lifecycle

**Summary:** Move all four adapters to the audited `f6c708cc` APIs. Bound JWKS fetching and local cache retention, apply route-aware admission/config deadlines, and cap auction/APS JSON at 1 MiB while mediation retains 8 MiB. Preserve prices, creatives, cookies and errors except the numbered migration changes.

**Changes:** Core owns fetch/cache, admission and extraction policy. Adapter changes cover required entrypoints/config mocks and response contracts. Cloudflare retains its compatibility date with three required flags. The checked-in 87-case HTTP corpus protects wire compatibility; PR CI now runs comparator tests and builds docs.

**Validation:** Native 198; target contracts 4/5/2; Playwright 41; comparator 25; 87 before/after wire cases; strict Clippy/fmt/features/config and docs checks; live Docker, Fastly, workerd and Spin checks all passed. Config push 0.35 roundtrip and Spin host-drift rejection passed. All evidence is local in the execution directory above.

**Release/operations:** Keep merge/release gated on upstream #275 merging and review the shipping tag before repinning all six sources and rerunning Task 8. Cloudflare needs a real KV binding, all three flags and config push; Fastly needs dynamic backends; Axum/Docker need restart after config push; Spin needs 4.1+. Blob envelope format is unchanged; retain the baseline artifact/config snapshot for rollback. The branch inherits the still-open PR #118 base; GitHub rejected retargeting PR #131 because it belongs to the existing stack. It retains base `worktree-deps-rust-1.98-align-ez366`; target `main` after #118 lands so the main-only PR workflows run. No other stack or branch was modified. Issue #130 tracks the migration and is linked for closure on merge.

- [x] Prepare the verified implementation for the requested publication using existing Task #130 and draft PR #131; preserve the merge/release gate.
- [ ] Complete the upstream merge/shipping-tag adoption gate and rerun the release matrix.

- Final upstream metadata refresh: #275 is still draft/open/unmerged; head advanced to `619c69bd2b2b24f35210c482b560cb46b1b96888`. This work continues to pin audited `f6c708cc`; no latest-head adoption is implied.

### Publication result (2026-10-21)

- Verified implementation commit `fe8ee46` was pushed without rewriting history.
- Existing Task [#130](https://github.com/stackpop/mocktioneer/issues/130) and draft PR [#131](https://github.com/stackpop/mocktioneer/pull/131) are reused instead of creating duplicates. The issue remains open, assigned to the current user and marked In progress; the PR links it for closure on merge.
- GitHub rejected the requested base change because PR #131 belongs to a stack. The existing stacked base is preserved; main-only PR CI and upstream release qualification remain explicit pending gates. No stack dissolution, unrelated branch update, deployment or merge was performed.


### Self-review follow-up (2026-10-22)

- Fixed a P2 wire-verification gap: matching manifests could omit all requests and still succeed. Comparison now validates each archive's recomputed corpus digest, complete ordered nonempty scenario case list and unique corpus IDs. Recorded request method/path/headers and generated body bytes must match the saved corpus, independently of whether the two captures agree.
- Added seven real CLI regression tests. The old implementation produced 15 failing subcases; the fix passes all **32 comparator tests**. An independent reviewer reran the suite and approved the change.
- Existing historical evidence remains valid: all **85 seeded + 2 unseeded** candidate comparisons and the independent baseline repeats pass under the stricter validation. Archived corpora remain self-contained; they do not need to match the current checkout's corpus.
- Fresh native workspace tests (**198**), strict Clippy, rustfmt and the feature check pass. Application runtime and provider code are unchanged.

### Fifteen-finding review follow-up (2026-10-22, R8)

Runtime, cache and cleanup reviewers independently checked the pasted feedback
against the branch and the pinned framework. The implementation follows the
recommended scope presented in the conversation: fix verified regressions and
local cleanup, preserving the approved timeout and cache policies. These are
deliberate operational tradeoffs, not guarantees of timing-independent success.

| Finding | Disposition and evidence |
| --- | --- |
| 1: error CORS | Fixed as C8. Cors renders matched errors through default `IntoResponse` and decorates them. RequestLogger runs inside Cors and observes errors before rendering. Registered OPTIONS still runs its handler and logger. Routing/head failures remain outside this middleware. |
| 2: slow config reads | Retained the approved 2 s deadline. A successful backend result arriving after an observed expiry can become 503; no new Retry-After policy is introduced. |
| 3: mediation upload budget | Retained the approved 10 s budget and 8 MiB cap. The cap is a size ceiling, not a promise that every connection can upload that size before expiry; 8 MiB over 10 s requires about 6.7 Mbps before overhead. |
| 4: determinism language | Fixed CLAUDE, AGENTS, README and the public guide/spec. Fixed-input pricing and selection remain deterministic; monotonic operational deadlines/cache expiry and wall-clock signature freshness affect operational outcomes. |
| 5: cache churn | Retained the approved oldest-fetched eviction. Thirty-two new valid hosts can evict an entry; LRU alone does not prevent this sequence. Failed key retrieval marks verification failed without rejecting the auction. |
| 6: poisoned lock | Retained the approved bypass-until-restart policy. Recovery/reset would change an explicit policy and its tests; no request-induced panic was identified. |
| 7: provider freshness | Retained PlatformDefault. Provider caching can outlive the local TTL, as already documented. Bypass would change provider semantics and may require Cloudflare cache compatibility changes on the retained date. |
| 8: cloning JWKS under lock | Deferred the pre-existing optimization. Arc requires revisiting retained-allocation accounting and performance validation; the migration already bounds the cache's owned allocations. |
| 9: missing debug logs | Restored cache hit/miss/expiry and fetch start/success events using canonical hosts, without provider diagnostics or credentials. |
| 10: ValidatedHost type | Deferred optional hardening. The private loader is the sole production caller and validates/canonicalizes before cache lookup or URL construction. |
| 11: framing duplication | Removed the unnecessary informational-status constructor guard and asserted final framework egress. Retained the empty-304 exception and the 204 guard: inserting then removing 204 length changes HeaderMap ordering and would alter preflight wire headers. |
| 12: unused dependency | Removed the unused workspace futures-util declaration. Transitive lockfile entries remain required. |
| 13: repeated config mocks | Kept the four thin target-local implementations, as permitted by spec §14. A shared support crate would expand the migration's workspace/API surface. |
| 14: duplicate lookup | Cache hits now use one map lookup, retaining freshness and accounting behavior. |
| 15: policy mapper | Both fixtures now use one private response-policy helper derived from the actual outbound request parts. Rust already rejects a missing field in either exhaustive initializer. |

Verification for this follow-up:

- CORS regression tests first failed for missing headers, then passed for all
  three bid routes' malformed JSON, validator, size and body-expiry errors, plus
  both config routes' complete error matrix. Controlled clocks avoid real sleeps.
- Fresh native workspace **199 tests**, strict Clippy, rustfmt and feature check
  passed. Actual target contracts passed: **Fastly 4, Cloudflare 5, Spin 2**.
  The Cloudflare signed-fetch/cache-hit test remains included.
- Fresh Chromium cross-origin requests read the status and JSON error body for
  **13 seeded errors and 2 unseeded config errors**, including JSON preflights,
  all three body caps and matched asset/query failures. Existing Playwright
  suite: **41 passed**.
- C8 is restricted to 15 named corpus cases/statuses and the exact three added
  fields before F10's framing permutation. Retry-After, unrelated headers and
  their order remain compared. Fresh **85 seeded + 2 unseeded** captures pass
  against baseline and independent baseline repeats, using the new corpus digest:
  `baseline-c8/`, `baseline-repeat-c8/`, `after-c8/` under the evidence directory.
- Comparator **37 tests passed**, including red/green C8 mutation tests and
  historical-corpus compatibility. The earlier 87 saved comparisons still pass
  using their own corpus; old captures were not modified.
- ESLint, Prettier and VitePress build passed after the public docs updates.
  Independent final review found stale middleware examples in two guides; these
  now match Cors → RequestLogger. No introduced production defect was found.
- Follow-up logs use the `feedback-` prefix in the evidence directory. The Rust
  verification summary records source hashes. Earlier Docker/provider live
  smokes remain historical evidence; this follow-up reran native/browser and all
  target contracts rather than repeating deployment qualification. Hosted outbound
  and the upstream shipping-tag adoption gate remain open.

### Second review follow-up (2026-10-23, R9)

Three independent reviewers checked cache/security, wire/provider coverage and
middleware/admission defaults against `205b1b9` and frozen EdgeZero `f6c708cc`.
The selected implementation fixes concrete regressions and adds useful coverage
without changing the user's approved timeout, eviction, cache-TTL, provider-cache
or poisoned-lock policies. Small Arc sharing now avoids the earlier deferred
performance issue without changing insertion charging or public APIs.

| Finding | Disposition |
| --- | --- |
| 1: internal hostname fetches | Confirmed, pre-existing. The new wildcard declaration does not newly grant Axum/Cloudflare/Fastly access; Spin already permitted wildcard HTTPS. Syntax validation cannot establish the resolved destination. No portable DNS resolution/pinning hook exists in the frozen outbound API. A required user choice is pending: default-empty operator-controlled exact hostname admission, or arbitrary publisher hosts with deployment DNS/IP egress controls. Suffix checks alone would leave public-looking internal destinations and rebinding unresolved. No internal-network probe was performed. |
| 2: config deadline | Retained the explicitly approved 2 s policy and its observed-expiry 503 behavior. |
| 3: mediation upload budget | Retained the explicitly approved 10 s budget and 8 MiB size ceiling, including the documented throughput tradeoff. |
| 4: cache churn | Retained oldest-fetched eviction. LRU does not prevent eviction after enough distinct attacker hosts; the bounded-memory policy remains unchanged. |
| 5: provider rotation lag | Retained PlatformDefault and the documented distinction between local TTL and provider freshness. |
| 6: poisoned cache | Retained bypass until restart; added a debug diagnostic for the bypass. Existing poisoned-before-lookup/during-fetch signature tests remain applicable. |
| 7: truncated-response crash | Fixed. Complete JSON validation precedes span walking; malformed/trailing-comma/deeply nested JSON becomes ContractError at the parser boundary. The CLI reports the failed case and continues to later cases. No broad Exception catch or success fallback was added. |
| 8: CORS renderer boundary | No current defect: default rendering is explicitly specified and RequestLogger is inside Cors. The pinned framework exposes no supported middleware accessor for the app renderer. Added source/architecture notes about custom renderer and middleware placement limitations; did not add CORS to routing/head failures. |
| 9: Spin coverage | Replaced routing-only dispatch with SPIN_PLATFORM app assembly, normalized-head validation, admission, admitted dispatch and egress completion. Added target checks for selected 10/30 s deadlines, the 2 s config setting and controlled mediation body expiry yielding finalized 408/CORS. This does not prove native Spin request-handle translation or provider preemption; native core fixtures already exercise actual config expiry. |
| 10: synthetic empty statuses | Kept explicitly scoped provider fixtures for runtime-unreachable statuses. Extended both Fastly and Cloudflare contracts with real Mocktioneer malformed-sync 400 and redirect 302 handlers, asserting zero length, empty body and redirect target. No production test route or public framing helper was introduced. |
| 11: private validated host type | Consider with finding 1's destination-admission boundary after the user chooses the policy. The current single production caller validates before lookup/fetch. A type would enforce syntax/admission, not resolved-address safety. |
| 12: cache copies under lock | Fixed with immutable Arc storage on cache hits. Misses retain the existing owned clone/insertion charge; fixed Arc overhead and in-flight request values remain excluded from the cache budget. A red/green pointer-sharing regression also checks accounting and usability after eviction. |
| 13: string-based redaction | Replaced kind-string matching with typed HTTP 500 classification. An Internal-only match would miss nine internal StoreExtraction reasons. Tests preserve redaction for all ten current internal cases and retain public details for representative 400/502/503 errors. |
| 14: Proxy wording | Kept the explicitly approved wire string. Documented that it means the request has no outbound client; it is not a reference to a current proxy API or specifically a Fastly dynamic-backend failure. |
| 15: default admission | Kept the pinned defaults. Upstream's default policy and alias are private, with no public getter/builder to delegate to. Added a source note; future upstream changes remain subject to shipping-pin review. |
| 16: framing and test duplication | Kept the 204 ordering and empty-304 representation guards for their documented wire behavior. Removed the redundant endpoint MapConfigStore by reusing the existing Blob fixture, without adding a support crate or another target fixture. |

- [ ] Resolve the operator-controlled JWKS destination policy and implement/test
  the selected boundary before treating the second review as complete.
- Fresh validation passed: **201 workspace tests** (174 unit, 12 APS, 14 endpoint,
  1 CLI), strict workspace Clippy, fmt and feature compilation. Actual target
  contracts passed: **Fastly 4, Cloudflare 5, Spin 4**; Spin target Clippy also
  passed. The legacy Spin helper returned 400 in the controlled-expiry regression
  before the lifecycle fix; the final test returns 408.
- Comparator **40 tests passed**, including a real CLI test that reports a
  malformed first case and still passes a valid later case. Historical and C8
  stored comparisons both retain all 87 passing cases. Fresh candidate
  **85 seeded + 2 unseeded** comparisons passed with the same saved C8 corpus.
- Docs ESLint, Prettier and VitePress build passed. Independent review approved
  the cache/redaction/provider changes; cache pointer sharing failed before Arc
  storage and passed afterward, with capacity accounting and post-eviction use
  intact. This follow-up did not rerun Docker/provider live smokes; they remain
  historical qualification evidence, with the native wire/target suites refreshed.
- Evidence directory: `/tmp/mocktioneer-pr131-review2-20261023.x6al8jyr`.
  The raw corpus is unchanged, so historical captures retain their original
  digests. Fresh candidate captures use the unchanged C8 corpus and compare
  against its existing baseline captures.

### Latest upstream adoption (2026-10-23, R10)

Requested action: "update to use latest from edgezero PR 275". Captured PR head:
`d47c8026a941a2361182c9e2042bebdcb8be4d04`, draft/open/unmerged. Independent
core/lifecycle, outbound/cache and provider/build audits compared materialized
source against `f6c708cc`; no speculative API migration or app policy change is
included. Evidence: `/tmp/mocktioneer-pr275-latest-20261023.27rggaxz`.

The first captured head was `7fdef8b0`; a final metadata refresh found one new
commit, `d47c8026`, adding bounded synthesized-response headers. Independent
core/provider delta audits selected pin-only adoption: the new immutable header
policy defaults empty and Mocktioneer does not configure it. No new CORS scope,
comparator exception, API migration, SDK or toolchain change is needed. Earlier
7f test/runtime captures remain historical; final qualification uses d47 and
separate evidence paths.

- [x] Review the changelog and consumed implementation/API delta. App assembly
  now propagates fallible generated routes automatically; adapter entrypoints,
  ConfigStore mocks, CLI wiring, toolchain and SDK versions stay compatible.
- [x] Repin all six workspace dependencies and all eight EdgeZero lockfile
  sources. Probe compilation first reproduced missing framing arguments and
  status/decoding fields; no unrelated lockfile updates were needed.
- [x] Adapt test helpers to explicit framing selection: HostAutomatic for
  Fastly, ExplicitBufferedLength for core/Spin. Preserve production response
  helper, CORS and admission policy. Pass actual reply status and request
  response_decoding into the private outbound policy mapper.
- [x] Adopt upstream typed 413/no-retry overflow without remapping to 400.
  Keep exact 1 MiB auction/APS and 8 MiB mediation caps; malformed JSON 400,
  validator 422, body expiry 408 and the config matrix retain their contracts.
  Existing over-cap tests first failed on 413 versus 400, then passed with exact
  kind/message and absent Retry-After assertions.
- [x] Add F14 only to the three named wire body-cap cases, preserving all
  unrelated ordered headers/body bytes. Support the original baseline's
  200→413 and the preceding `04c3635` implementation's exact 400→413. Keep
  historical no-F14 captures self-contained. Comparator regressions: **49 passed**.
  Corpus SHA-256: `524c2555445b7e57e2af56676902557c97008dae34b0d94afc2968bb9478f1a4`.
- [x] Recapture original and preceding baselines/repeats plus the candidate
  using the new corpus, then compare all 85 seeded + 2 unseeded cases.
  Both baselines restarted independently; final d47 candidate/repeat/self and
  original→candidate/preceding→candidate passed **435 case checks** across ten
  comparisons. Baseline repeats also passed. No archived corpus was rewritten.
- [x] Complete fresh native fmt/Clippy/features, strict config and 0.35 push
  roundtrip, Playwright and final source checks. Final d47: **201 tests**, strict
  workspace Clippy, fmt, locked fetch and `fastly cloudflare spin` feature check;
  strict config and isolated HTTP CPM **0.35** roundtrip; **41 Playwright tests**.
- [x] Run actual target contracts: **Fastly 4, Cloudflare 5, Spin 4** passed.
  See `final-d47/provider-verification.json` for exact commands, tools, artifacts
  and hashes; fetched upstream source matches the reviewed materialization.
- [x] Refresh public API/CLI/capability docs and the active spec. ESLint,
  Prettier and VitePress build passed. The new 206 original-coding behavior is
  documented without changing successful 200 JWKS decoding or the 64 KiB cap.
- [x] Complete real local Docker, Fastly production main, Cloudflare/workerd
  and Spin qualification and record precise evidence boundaries.
  Final d47 rebuilt Docker linux/amd64 and all WASM artifacts: root/seeded
  auction CPM 0.20 passed in all four runtimes; Fastly/workerd additionally
  passed GIF43 and real empty 400/302 handlers. Workerd overflow returned exact
  413/CORS/no-retry; direct workerd origin root matched the public root bytes
  and exact length before Wrangler HTML rewriting. Spin CLI positive build,
  isolated host-drift rejection, restored build and local SQLite push passed.
  All temporary servers/containers stopped; no deployment or registry push.
- [x] Independently review the final change for spec compliance and quality.
  Separate reviews approved d47 with no outstanding findings; earlier review
  corrected stale README overflow status and Fastly assembly-failure wording.
  Publication continues on existing draft PR #131 and issue #130, whose live
  records carry the current remote-head/CI state and remaining gates.

Final evidence indexes under `final-d47/`: `provider-verification.json`,
`wire-qualification/qualification.json`, `live-runtime-summary.json`,
`cloudflare-qualification.json`, `live-cloudflare/results.json`, plus native,
comparator and docs logs. Config/browser details are in the sibling
`final-native-config-browser/`. Initial fixture-only command mistakes are
retained: worker-build from workspace rather than adapter cwd and strict
validation before copying the referenced Spin manifest; corrected isolated
commands passed without application changes. Spin emitted a shutdown-only
worker warning after SIGTERM, with clean pre-cleanup logs and exit 0.

Docker image: `mocktioneer:pr275-final-d47-27rggaxz`,
`sha256:05270942b798979a6cb7e329472b6e77b09c1686c73fcd02ca0756c0ba0ff0d8`.
Provider manifest SHA-256:
`8b86745c4e1b984caaa884ea45fbef87ebb4be6bb8b123ff687d39f83b7fc190`.
All 13 GitHub checks passed for preceding head `04c3635`; the new update
requires fresh CI. Final upstream refresh still observed d47, draft/open/unmerged.

Unchanged decisions: 10/30 s ingress, 2 s config, 5 s JWKS, cache TTL/limits/
oldest-fetch eviction/poison bypass/PlatformDefault/Arc hits, optional outbound,
Cloudflare date and all three compatibility flags. The latest upstream still
delegates destination authorization to applications; it adds no portable
resolved-IP or DNS-pinning control. The user's destination-policy question and
its merge gate remain open. Hosted qualification and shipping-tag adoption
also remain separate; local tests do not promote provider guarantees.

### Node wire tooling conversion (2026-10-25, R11)

The user requires shell or Node scripts instead of committed Python. Replace
the two PR-added Python files with `tests/wire-contract/wire_contract.mjs` and
`wire_contract.test.mjs`, using Node 24 built-ins and curl with no packages.
Keep the exact corpus/digest, archive layout, ordered Buffer comparisons, UUID
masking and all F/C exceptions. Retain all 49 regression cases and their mutation
loops, including malformed-case continuation and strict archived-request checks.

- [x] Port the regression suite first and record expected failures with the
  missing Node implementation. Use the same exported helper names and positional
  optional reference arguments to minimize translation changes.
- [x] Port capture/compare/raw characterization to Node. Preserve strict UTF-8,
  escaped JSON string spans, UUID version/variant and byte/length checks. Use
  bounded JSON scanning and retain raw TCP observations separately.
- [x] Remove both Python files; update CLI examples, CLAUDE CI notes and the
  existing Node-equipped workflow job to `node --test` without npm dependencies.
- [x] Run all regressions, current and historical saved comparisons/mutations,
  plus a fresh 85 seeded + 2 unseeded Node capture and raw framing check. Keep
  prior captures immutable; run required Rust and docs checks.
- [x] Obtain independent spec/quality review. Publish through the same draft
  PR #131 and issue #130; their live records carry remote-head/CI state. Leave
  the d47 pin and application policies intact.

Read-only test/API and CI/docs audits both recommend the built-in Node runner.
Use the existing format-docs Node setup rather than provisioning another job;
converting to shell would add fragile binary/JSON handling and runtime tools.

Final verification: **56 Node tests** (all 49 original cases plus seven port/CLI
guards), **52 archived comparisons / 2,262 case checks**, and **435 fresh case
checks** covering both baselines, archived d47, self and independent restart
repeat comparisons. Each scenario pair retains all **85 seeded + 2 unseeded**
cases. Both raw TCP request byte sequences match the archived baseline. The
corpus is unchanged. Required **201 workspace tests**, Rust fmt/strict Clippy/
feature compilation, docs lint/format/build and Node formatting passed. No Rust
source, dependency or application behavior changed.

Evidence: `/tmp/mocktioneer-node-wire-20261025.W3ShCSqJ/`, especially
`archive-comparisons-final/qualification.json`, `live-final/qualification.json`,
`node-final-green.log` and required Rust/docs logs. The initial repeat capture
properly failed on a curl55 connection reset for the 9 MiB mediation upload; it
remains under `live/`. All request bytes matched the archive, and the unchanged
d47 server logged 413. Sixteen direct identical archived/Node requests returned
413; the full fresh rerun and final run passed. An early-response/unread-upload
transport race is plausible, not proven. No request, timeout, retry or comparator
rule was changed to hide the failure. Final raw observations retain their local
host-parser scope; earlier provider runtime evidence is historical and was not
unnecessarily rerun for this tooling-only conversion.

### Third review follow-up (2026-10-25, R12)

Review target `91ffbd3` precedes the Node conversion at actual head `e119453`.
Read-only cache/security, router/lifecycle and comparator/CI/docs audits verified
the findings before selecting these minimal changes. No Python is introduced.

- [x] Make ordinary F14 candidate comparisons require 413 on the after side.
  Add explicit `compare --baseline-repeat` for exact historical 200/200 and
  400/400 self/repeat captures; reject changed cap statuses in that mode. Keep
  corpus/archive bytes, helper signatures and historical no-F14 contracts.
- [x] Add a signed auction through the actual router with injected client,
  seeded config, real signature, creative verification metadata and captured
  GET/URL/count assertions. Use a separate uncached publisher for the no-client
  negative; cached keys can legitimately verify without a client.
- [x] Share one immutable Arc between miss result, cache and hit; charge actual
  retained allocations. Add pointer/accounting/fetch-count regression, avoid
  unnecessary removal on plain misses, and honestly name the synthetic oversized
  insertion test rather than claiming it exercises the bounded network loader.
- [x] Require a private canonical syntax-validated hostname at fetch_jwks.
  This type is not destination authorization or resolved-IP/DNS safety.
- [x] Separate synthetic provider framing fixtures from real handlers; add real
  OPTIONS/204 coverage and a raw build_response204 no-length assertion. Keep
  the private helper and existing 304/204 wire guards. Remove redundant Spin cfg.
- [x] Correct the Axum architecture snippet, document all review dispositions,
  run relevant Rust/Node/provider/docs and wire checks, obtain independent review
  and publish through the existing draft PR/issue.

Disposition constraints: destination admission remains an unanswered user choice
and merge gate. d47 exposes no public default admission-policy getter/delegate;
the small pinned copy must be reviewed on repin. CORS renderer bypass and legacy
Proxy wording are documented choices; timeout/cache policies remain approved.
The four bounded-read test implementations already call the upstream helper and
have distinct fixtures; do not add production support APIs merely to deduplicate
them. The comparator already runs independently of Rust lint in format-docs.

Third-review dispositions:

| Finding | Disposition |
| --- | --- |
| 1: internal destination fetch | Confirmed and still open pending the user's destination policy; syntax typing does not resolve it. Keep merge gated. |
| 2: copied admission defaults | No public default getter/delegate at d47. Retain the small pinned copy; source comment and shipping-repin audit explicitly require reviewing completion/grant drift. |
| 3: signed router coverage | Added real signed admitted/router dispatch, seeded pricing, verification metadata and outbound request assertions, plus an uncached no-client negative. Removing client injection failed the Verified assertion before restoration. |
| 4: cap 200/200 comparison | Normal CLI candidates now require 413; explicit baseline-repeat permits only unchanged cap statuses. All three cap cases have CLI positive/negative matrix coverage. |
| 5: unchecked fetch string | Added private canonical syntax-validated hostname boundary. Destination authorization remains separate. |
| 6: custom renderer bypass | Retained the documented CORS trade-off; this application does not install a custom renderer. |
| 7: oversized test scope | Renamed as defensive insertion-contract coverage, shares caller-held keys and checks accounting; does not claim to exercise the 64 KiB network loader. |
| 8: synthetic framing | Kept synthetic provider status fixtures separately named. Real 400/302/OPTIONS 204 cover actual handlers; raw constructor 204 assertion catches the pre-egress guard regression. |
| 9: cache miss copies | One Arc is now shared by miss result/cache/hit, with pointer identity, single-fetch and actual-capacity accounting assertions. |
| 10: CI lint visibility | Already resolved by e119453: comparator is in format-docs, independently of the Rust lint job. |
| 11: Axum architecture example | Shows actual print-error/exit 1 startup behavior. |
| 12: Proxy wording | Kept the approved wire string; docs explicitly connect it to an absent optional outbound-http client. |
| 13: repeated test helpers | Kept thin bounded-read wrappers using the public upstream finish helper and distinct fixtures; kept platform-specific completion modes instead of adding production support APIs. |
| 14: miss/remove and Spin cfg | Plain misses no longer remove; expired removal remains. Removed redundant integration-test cfg(test). |

Fresh R12 verification passed: **202 workspace tests** (174 unit, 12 APS, 15
endpoint, 1 CLI), required strict workspace Clippy/fmt/features/build, **61 Node
tests**, **Fastly 5 / Cloudflare 6 / Spin 4** target contracts and docs lint/format/build.
**52 archive comparisons / 2,262 case checks** pass with explicit baseline roles;
ordinary candidate mode rejects all **six** real original/preceding cap cases.
Fresh native **85 seeded + 2 unseeded** cases pass against both baselines and the
archived d47 candidate, plus self and independent restart repeat (**435 checks**).
Raw framing requests remain byte-identical to their archived baselines. The raw
204 guard mutation failed the new assertion; restoring it passed. Independent
spec and quality reviews approved the final delta.

Evidence: `/tmp/mocktioneer-pr131-review3-20261025.FV1e1OlA/`, especially
`workspace-final.log`, `clippy-final.log`, `node-final.log`, provider
`routes-*-contract-final.log`, `archives/qualification.json` and
`live/qualification.json`. All source work uses Rust/Node/shell; corpus and all
six d47 pins remain unchanged. Prior live Docker/workerd/Spin/Fastly and
Playwright qualification is historical, not claimed as a fresh R12 run.

An extra optional WASM-target strict Clippy probe reported 28 Fastly contract-file
lint errors and 3 Cloudflare library lints (wildcard imports, missing error docs,
missing inline). No baseline comparison proved every diagnostic pre-existing;
this is not the required native workspace gate, which passed. The target
contract suites passed. No broad unrelated lint cleanup was added.
