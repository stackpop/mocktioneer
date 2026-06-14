# Design: Adapt Mocktioneer to edgezero #269 (extensible CLI)

- **Date:** 2026-06-11
- **Branch:** `feature/edgezero-extensible-cli` (off `main`)
- **Upstream:** [stackpop/edgezero#269](https://github.com/stackpop/edgezero/pull/269)
  — "EdgeZero CLI Extensions". Head `feature/extensible-cli`, base
  `chore/strict-clippy`, **OPEN / unmerged** (re-verified via
  `gh pr view 269` on 2026-06-11; re-check at implementation).
- **Scope (approved):** Compat + typed config, with `bid_cpm` consumed at
  runtime. Pin deps to `feature/extensible-cli` now; re-pin to `main` after
  #269 merges.
- **Revision history:** R1 initial; R2 review (config lifecycle, validators,
  scope); **R3 second review** — corrects two errors I made: (a) the **APS
  path also uses the fixed CPM** (`build_aps_response` lives in `auction.rs`
  and reads `FIXED_BID_CPM`), so `handle_aps_bid` is in scope; (b) Axum binds
  an **empty** store when the local file is missing, so `Ok(None)` must
  **fall back**, not error. Also: Spin config is KV-backed; `EdgeError`/
  validator signatures corrected; no `.tool-versions` bump needed; gitignore +
  check-ci updates added. **R4 base change** — rebased onto `main`: PR #108
  ("Adopt edgezero strict-clippy gate and PR #257 API") merged the prerequisite
  strict-clippy/#257 adaptation into `main`, and `main` pins all `edgezero-*`
  deps at `branch = "main"`, so the dep repin is now `main` →
  `feature/extensible-cli` (no longer stacked on `chore/edgezero-strict-clippy`).
  R4 review findings (anyhow-in-core, Dockerfile manifest COPY, docs
  edgezero-cli vs mocktioneer-cli story, exact check-ci commands, cli-crate
  metadata/lints, docs-formatter/VitePress exclusion) are tracked but not yet
  folded into the body below.

## 1. Problem & context

Mocktioneer consumes six `edgezero-*` crates from git, pinned (on `main`) to
edgezero's `branch = "main"`. PR #269 is a large, intentionally
backward-incompatible refactor. We adapt to the new APIs ahead of merge and
adopt the typed app-config surface, with `bid_cpm` wired through at request
time. (Base note: `main` already carries the strict-clippy + PR #257 edgezero
adaptation via PR #108, so this branch starts from that merged state.)

### What #269 changes that actually reaches this repo

From the sibling checkout `../edgezero` (`feature/extensible-cli`):

- Mocktioneer has **no `[stores.*]`** today and makes **no
  `kv_store`/`config_store`/`secret_store` calls**, so the store breakages are
  inert until we opt in.
- `ctx.path()?` (with a `let x: T =` annotation), the `RequestContext(ctx)`
  destructure, `RequestContext::new`, `ctx.request()`, and
  `edgezero_core::context::RequestContext` **all still compile**.
- `app!` **still accepts the 2-arg custom-name form** → `MocktioneerApp`
  retained, **no app rename**.
- `[adapters.*.build]`/`[adapters.*.commands]` remain valid; only legacy
  `[stores.*]` and `[adapters.*.stores]` shapes are rejected — neither present.

Parts that **do** reach this repo:

1. Every adapter's `run_app` **dropped the `include_str!(manifest)` arg**.
2. **Spin → `spin-sdk ~6.0` / `wasm32-wasip2`** (`#[http_service]`). Fastly
   stays `wasm32-wasip1`.
3. New **typed `AppConfig`** + generated `<name>-cli` shape + per-adapter
   config-store **lifecycle** (declare → seed → auto-bind on serve → read).

## 2. Goals / non-goals

**Goals**

- Builds, lints (`-D warnings`), tests green against `feature/extensible-cli`
  on all targets (native, Fastly `wasm32-wasip1`, Spin `wasm32-wasip2`,
  Cloudflare `wasm32-unknown-unknown`).
- Typed config: validated `MocktioneerConfig`, `mocktioneer.toml`,
  `mocktioneer-cli`, and a **required** `config validate --strict` CI gate.
- `bid_cpm` consumed at runtime by **both** the OpenRTB auction path
  (`handle_openrtb_auction`) **and** the APS path (`handle_aps_bid`), since
  both currently emit `FIXED_BID_CPM`, with a defined fallback/error contract.

**Non-goals**

- KV or secret stores.
- Re-pinning to `main` (post-merge follow-up).
- Threading any other config field through rendering/business logic (config v1
  is `bid_cpm` only).
- Fixing pre-existing `Uuid::now_v7()` non-determinism in bid IDs (§5; out of
  scope).

## 3. Design

### 3.1 Dependency pin — `Cargo.toml`

- Six `edgezero-*` git deps: `branch = "main"` → `branch = "feature/extensible-cli"`.
- `spin-sdk = "5.2"` → `{ version = "~6.0", default-features = false,
  features = ["http", "key-value", "variables"] }`.
- Add `clap = { version = "4", features = ["derive"] }` to
  `[workspace.dependencies]` (consumed as `clap = { workspace = true }`).
- Regenerate `Cargo.lock`.

### 3.2 Adapter entrypoints — drop the manifest arg

| File | After |
| --- | --- |
| `mocktioneer-adapter-axum/src/main.rs` | `run_app::<MocktioneerApp>()` |
| `mocktioneer-adapter-cloudflare/src/lib.rs` | `run_app::<MocktioneerApp>(req, env, ctx)` |
| `mocktioneer-adapter-fastly/src/main.rs` | `run_app::<MocktioneerApp>(req)` |
| `mocktioneer-adapter-spin/src/lib.rs` | `#[http_service]` + `Request` + `run_app::<MocktioneerApp>(req)` |

Spin `lib.rs` swaps `expect(unsafe_code, …)` → `allow(unsafe_code, reason =
"spin's #[http_service] macro generates the unsafe wasm export")`; final
import/`no_main` shape follows the SDK-6 template (compile-driven).

### 3.3 Spin → `wasm32-wasip2`

- `crates/mocktioneer-adapter-spin/spin.toml`: `source` path + `build.command`
  `wasip1` → `wasip2`.
- `edgezero.toml` `[adapters.spin.build].target`: `wasip1` → `wasip2`.
- `crates/mocktioneer-adapter-spin/src/main.rs` host-stub messages.
- `crates/mocktioneer-adapter-spin/tests/contract.rs` comments + runner
  invocation (now a `wasm32-wasip2` component), mirroring edgezero's spin
  contract test.
- **Fastly unchanged** (`wasm32-wasip1`).

**`.tool-versions` — no change needed (verified).** Mocktioneer's
`.tool-versions` has no Spin pin; the only deltas vs edgezero are Fastly
(13.0.0 vs 15.1.0) and Wasmtime (45.0.0 vs 44.0.1). Mocktioneer's
**Wasmtime 45.0.0 already supports wasip2/component-model** (newer than
edgezero's pin), so running wasip2 Spin components needs no bump. Confirm at
implementation that 45.0.0 runs the component; only then revisit.

**Runner config:** `.cargo/config.toml` defines only `[target.wasm32-wasip1]`
(Viceroy, Fastly). Spin wasip2 contract tests use
`CARGO_TARGET_WASM32_WASIP2_RUNNER` (e.g. `wasmtime run`) set **per-job in
CI** and documented for local runs — **not** a global config entry that would
disturb Fastly's wasip1 Viceroy runner.

### 3.4 Manifest — `edgezero.toml`

- Existing `[[triggers.http]]` / `[adapters.*]` tables accepted as-is.
- Add:
  ```toml
  [stores.config]
  ids = ["mocktioneer_config"]
  ```
- Add per-adapter native-backing entries where the new schema requires them
  for non-axum adapters (see §3.6; compile/validate-driven).
- `manifest.validate()` runs at compile time inside `app!`.

### 3.5 Typed config struct + runtime contract

**Struct** — `crates/mocktioneer-core/src/config.rs`:

```rust
use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError};

#[derive(Debug, Deserialize, Serialize, Validate, edgezero_core::AppConfig)]
#[serde(deny_unknown_fields)]
pub struct MocktioneerConfig {
    /// Fixed bid CPM in USD. Validated finite and strictly positive.
    #[validate(custom(function = "validate_bid_cpm"))]
    pub bid_cpm: f64,
}

// `validator` custom fns take a reference to the field (cf.
// routes.rs `validate_static_asset_size(value: &str)`); `range` does not
// reject NaN / inf for floats, so validate explicitly.
fn validate_bid_cpm(value: &f64) -> Result<(), ValidationError> {
    if value.is_finite() && *value > 0.0 {
        Ok(())
    } else {
        Err(ValidationError::new("bid_cpm_must_be_finite_positive"))
    }
}
```

- `creative_label` (R1 draft) removed — YAGNI/unused.
- `pub mod config;` in `crates/mocktioneer-core/src/lib.rs`.
- Root **`mocktioneer.toml`** (1:1, no `[config]` wrapper): `bid_cpm = 0.20`.

**Runtime resolution — shared helper, used by BOTH handlers.** Both
`build_aps_response` (in `auction.rs`, reads `FIXED_BID_CPM` at line ~259) and
the OpenRTB bid builder (`auction.rs` ~116/120) emit the fixed CPM, so both
`handle_openrtb_auction` (already binds `RequestContext(ctx)`) and
`handle_aps_bid` (gains `RequestContext(ctx)`, alongside its existing
`ForwardedHost`/`ValidatedJson` — the multi-extractor pattern
`handle_openrtb_auction` already uses) resolve `cpm` once and thread it in:

```rust
// routes.rs
async fn resolve_bid_cpm(ctx: &RequestContext) -> Result<f64, EdgeError> {
    // No config store bound at all → compile-time default.
    let Some(store) = ctx.config_store_default() else {
        return Ok(auction::FIXED_BID_CPM);
    };
    // Store read errors map through `From<ConfigStoreError>` (→ 400/503/500);
    // do not mask a broken backend as a $0.20 bid.
    match store.get("bid_cpm").await.map_err(EdgeError::from)? {
        // Declared but unseeded: Axum binds an EMPTY store when
        // `.edgezero/local-config-mocktioneer_config.json` is absent
        // (dev_server.rs `from_local_file` → `Ok(empty)`), so a missing key
        // is the normal "not yet pushed" state → fall back, not error.
        None => Ok(auction::FIXED_BID_CPM),
        // Present but malformed → real misconfiguration, surface it.
        Some(raw) => raw
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite() && *v > 0.0)
            .ok_or_else(|| {
                EdgeError::internal(anyhow::anyhow!(
                    "config store `mocktioneer_config` has malformed bid_cpm: {raw:?}"
                ))
            }),
    }
}
```

`auction.rs` bid builders and `build_aps_response` take a `cpm: f64` parameter
(replacing direct `FIXED_BID_CPM` reads); `FIXED_BID_CPM` stays as the
fallback constant and the value existing tests pass explicitly. **Contract
summary:** no store → fallback; bound + key absent → fallback; bound + read
error → propagate; bound + present-but-malformed → error. `EdgeError::internal`
takes `Into<anyhow::Error>` (error.rs:63), hence `anyhow::anyhow!`, not a bare
`String`. **Determinism preserved** on the fallback path (semantic outputs
match `main`).

### 3.6 Config lifecycle (declare → seed → bind → read)

Binding on the **serve path is automatic** once the store is declared and the
backing exists — `edgezero-adapter-axum/src/dev_server.rs:333 run_app` →
`build_config_registry(stores.config)` reads
`.edgezero/local-config-<id>.json`; Cloudflare (`request.rs:362`), Fastly
(`request.rs:382`), Spin (`config_store.rs` via `request::build_config_registry`)
have equivalents. The `app!` macro emits `stores().config` from
`[stores.config]`. **Axum binds an empty store when the file is missing** (it
does not skip the id), which is exactly why §3.5's contract falls back on
`Ok(None)`.

Per-adapter backing + seed step:

| Adapter | Backing for `mocktioneer_config` | Seed |
| --- | --- | --- |
| **Axum** (primary local/CI path) | `.edgezero/local-config-mocktioneer_config.json` | `mocktioneer-cli config push --adapter axum` writes it. |
| **Cloudflare** | KV namespace (config moved `[vars]`→KV in #269) | `provision --adapter cloudflare` (writes id to `wrangler.toml`) + `config push --adapter cloudflare`. |
| **Fastly** | config store + `[setup]`/`[local_server]` in `fastly.toml` | `provision --adapter fastly` + `config push --adapter fastly`. |
| **Spin** | **KV-backed** (`key_value_stores = ["mocktioneer_config"]` + `[key_value_store.mocktioneer_config]` runtime config) — config is KV-backed in #269; `[variables]` is secrets-only now (`edgezero-adapter-spin/src/config_store.rs`). | `provision --adapter spin` + `config push --adapter spin`. |

**Pragmatics:** the **axum** path is the one exercised locally and in CI.
Cloud adapters get the manifest declaration + required native-backing tables so
they *build and validate*; push/provision are documented but live cloud stores
are not part of CI. Handler tests wire a `ConfigRegistry` fixture directly (the
app-demo `handlers.rs` test pattern) to cover the seeded-value, empty-store
(fallback), and malformed-value (error) branches without a live backend.

### 3.7 `mocktioneer-cli` crate

`crates/mocktioneer-cli/`, mirroring `edgezero-cli/src/templates/cli/`:

- `Cargo.toml`: `mocktioneer-core`, `edgezero-cli`, `clap = { workspace = true }`,
  `log`.
- `src/main.rs`: clap `Args`/`Cmd` flattening
  `edgezero_cli::run_{auth,build,deploy,new,provision,serve}` + a typed
  `Config` subcommand dispatching `run_config_validate_typed::<MocktioneerConfig>`
  and `run_config_push_typed::<MocktioneerConfig>`.
- Add `crates/mocktioneer-cli` to root `[workspace].members`.

### 3.8 Docs, agents, ignore files (verified surface)

- `wasip1` → `wasip2` for **Spin only** + spin-sdk-6 note, across the files
  that reference it (rg-verified): `CLAUDE.md`, `edgezero.toml`, the spin crate
  files (§3.3), `.claude/agents/{code-architect,build-validator,verify-app}.md`,
  `docs/guide/getting-started.md`, `docs/guide/configuration.md`,
  `.cargo/config.toml` (comment), `.github/workflows/test.yml` (§3.9).
  **Leave Fastly wasip1 intact.** No `docs/guide/adapters/spin.md` exists;
  `check-ci.md`/playwright README have no spin/wasip refs.
- **`.gitignore`:** add `.edgezero/` (currently only `.spin/` is ignored; the
  worktree already has an untracked `.edgezero/`).
- **`.claude/commands/check-ci.md`** and **`CLAUDE.md` "CI Gates"**: add the
  new `config validate --strict` gate so local CI docs aren't stale.

### 3.9 CI — `.github/workflows/test.yml`

- Spin matrix: target `wasip1` → `wasip2`; install the `wasm32-wasip2` rustup
  target; set `CARGO_TARGET_WASM32_WASIP2_RUNNER`; keep the pinned Wasmtime
  install and confirm it runs wasip2 components.
- `mocktioneer-cli` covered by `--workspace`.
- **Required gate:** `cargo run -p mocktioneer-cli -- config validate --strict`,
  plus a **real** `config push --adapter axum` (not dry-run — dry-run does not
  seed) followed by an assertion that a seeded `bid_cpm` flows through (an
  integration test or a serve smoke), so the seed → bind → read path is
  actually exercised, not just validated.
- If the docs Prettier/ESLint gate scopes `docs/**`, exclude
  `docs/superpowers/**`.

## 4. Risks & mitigations

| Risk | Mitigation |
| --- | --- |
| New parser rejects an existing `[adapters.*]` table | Compile-time `manifest.validate()` surfaces it; fix per upstream error. |
| Spin SDK 6 macro/type churn beyond template | Mirror edgezero's `edgezero-adapter-spin` verbatim; build wasip2. |
| Wasmtime can't run the wasip2 component | Wasmtime 45.0.0 supports it; set `CARGO_TARGET_WASM32_WASIP2_RUNNER`; match edgezero's contract-test config. |
| Fresh dev errors before any push | Resolved: empty/absent → fallback to `FIXED_BID_CPM` (§3.5). |
| Broken/malformed pushed config masked as $0.20 | Read errors propagate; malformed present value errors (§3.5). |
| `bid_cpm` never exercised (store unseeded) | CI does a real `config push --adapter axum` + asserts the value flows; fixtures cover all branches. |
| Pinning to an unmerged branch | Documented; re-pin to `main` post-merge. |
| `.cargo/config.toml.local` patch drift (`edgezero-macros`) | Already lists it; verify it patches cleanly. |

## 5. Verification

1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace --all-targets --all-features -- -D warnings`
3. `cargo test --workspace --all-targets`
4. `cargo check --workspace --all-targets --features "fastly cloudflare"`
5. Wasm builds: Fastly (`wasm32-wasip1`), Spin (`wasm32-wasip2`), Cloudflare
   (`wasm32-unknown-unknown`).
6. `cargo run -p mocktioneer-cli -- config validate --strict` passes; negative
   / non-finite `bid_cpm` fail validation.
7. Spin `contract.rs` green under the wasip2 wasmtime runner.
8. **Semantic** comparison of OpenRTB **and APS** responses vs `main` with no
   config store bound: bid/slot `price` (and APS `encoded_price`), sizes,
   `cur`, creative markup/URLs, targeting match; volatile fields (IDs from
   `Uuid::now_v7()`, auction.rs:51 / mediation.rs:102 / APS `new_id()`) are
   excluded — byte equality is impossible even on `main`.
9. Runtime exercise: with `.edgezero/local-config-mocktioneer_config.json` =
   `{"bid_cpm":"0.35"}`, OpenRTB and APS emit `0.35`; with the file absent or
   an empty object, they emit `0.20` (no error); with `{"bid_cpm":"-1"}` the
   handler returns an error.

## 6. Rollback / follow-ups

- Rollback = revert the branch; deps return to edgezero `branch = "main"`.
- Follow-up: re-pin all `edgezero-*` deps to `main` once #269 merges.
- Pre-existing tech-debt (not fixed here): bid IDs use `Uuid::now_v7()`, which
  is non-deterministic despite the project's determinism guarantee.
