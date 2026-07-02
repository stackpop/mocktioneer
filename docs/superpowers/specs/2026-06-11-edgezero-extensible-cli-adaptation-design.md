# Design: Adapt Mocktioneer to edgezero #269 (extensible CLI)

- **Date:** 2026-06-11
- **Branch:** `feature/edgezero-extensible-cli` (off `main`)
- **Upstream:** [stackpop/edgezero#269](https://github.com/stackpop/edgezero/pull/269)
  — "EdgeZero CLI Extensions". Head `feature/extensible-cli`, base **`main`**
  (rebased off `chore/strict-clippy`), **OPEN / unmerged** (re-verified via
  `gh pr view 269` on 2026-06-15: state OPEN, base `main`, updated
  2026-06-12; re-check at implementation).
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
  **R5** — folded all prior review findings into the body: anyhow-in-core
  (§3.1), Dockerfile manifest COPY (§3.7/§5), cli-crate metadata/lints (§3.7),
  mandatory docs-formatter + VitePress exclusion (§3.8), the wider
  edgezero-cli→mocktioneer-cli story incl. README/Playwright (§3.8), the public
  "$0.20 always" → "$0.20 default" pricing-doc rewrite (§3.8), the Spin
  `runtime-config.toml`/`--runtime-config-file` requirement (§3.6), the
  malformed-config-FILE-vs-read-error nuance (§3.5), and an explicit seeded
  test fixture (§5). **R6 (PR #110 review)** — CI seed gate now seeds a
  **non-default** `0.35` and `jq`-asserts it (a bare push silently seeds the
  `0.20` default); the §5 runtime fixture is a **registry-backed
  `resolve_bid_cpm`** test (public `StoreRegistry`/`ConfigStoreHandle`, app-demo
  `config_flow.rs` pattern), not just the pure parser; Dockerfile reframed as
  cache hygiene (+ add the missing spin manifest); CLI-story widened to the
  per-adapter doc pages; docs `srcExclude` made verifiable + `.vitepress/.temp`
  ignored; plan call-site note corrected (mediation has a _separate_
  `build_openrtb_response`). **R7 (during implementation)** — dropped the `new`
  subcommand from `mocktioneer-cli`: scaffolding a brand-new EdgeZero app from
  within Mocktioneer's own project CLI is nonsensical, so the crate exposes
  `auth`/`build`/`deploy`/`provision`/`serve` + typed `config` only.
  **R8 (sync to edgezero `89f59266`)** — supersedes the §3.5 read model. edgezero
  advanced past the pin with a **blob app-config cutover**: the whole typed
  config is stored as one canonical-JSON **blob envelope** (SHA-gated) under the
  store's key, read via the new **`AppConfig<C>` extractor**, not per-leaf
  `get("bid_cpm")`. Per the user's call, the handlers now use the **fail-loud**
  bare `AppConfig(cfg): AppConfig<MocktioneerConfig>` extractor (dropping the
  graceful `resolve_bid_cpm` wrapper): OpenRTB/APS **require a `config push`**
  before serving (they error otherwise; `FIXED_BID_CPM` is the builder default /
  shipped value, no longer a runtime fallback). Also: added the new `config diff`
  command (`DiffExit`) to `mocktioneer-cli`; `config push` now needs `--yes` in
  CI; the local-config file is `{ "mocktioneer_config": "<envelope>" }`, so the
  CI assertion is `jq -r '.mocktioneer_config | fromjson | .data.bid_cpm'`; and
  endpoint/handler tests seed a blob via a `ConfigRegistry` fixture
  (`StoreRegistry` + `ConfigStoreBinding` + `BlobEnvelope::new`).

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
  both currently emit `FIXED_BID_CPM`, read via the typed config (R8: fail-loud
  `AppConfig` extractor — a `config push` is required before serving).

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
- Add `anyhow = { workspace = true }` to **`crates/mocktioneer-core/Cargo.toml`**
  (the §3.5 helper uses `anyhow::anyhow!`; core does not currently depend on
  anyhow — only the workspace table does). `anyhow` is WASM-compatible.
- Regenerate `Cargo.lock`.

### 3.2 Adapter entrypoints — drop the manifest arg

| File                                        | After                                                            |
| ------------------------------------------- | ---------------------------------------------------------------- |
| `mocktioneer-adapter-axum/src/main.rs`      | `run_app::<MocktioneerApp>()`                                    |
| `mocktioneer-adapter-cloudflare/src/lib.rs` | `run_app::<MocktioneerApp>(req, env, ctx)`                       |
| `mocktioneer-adapter-fastly/src/main.rs`    | `run_app::<MocktioneerApp>(req)`                                 |
| `mocktioneer-adapter-spin/src/lib.rs`       | `#[http_service]` + `Request` + `run_app::<MocktioneerApp>(req)` |

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
use validator::Validate;

#[derive(Debug, Deserialize, Serialize, Validate, edgezero_core::AppConfig)]
#[serde(deny_unknown_fields)]
pub struct MocktioneerConfig {
    /// Fixed bid CPM in USD. Strictly positive — `exclusive_min` rejects
    /// `0.0`, negatives, and `NaN`; non-finite floats are also rejected by
    /// edgezero's typed loader. (`_f64` suffix satisfies the strict
    /// `default_numeric_fallback` clippy lint.)
    #[validate(range(exclusive_min = 0.0_f64))]
    pub bid_cpm: f64,
}
```

- `creative_label` (R1 draft) removed — YAGNI/unused.
- `pub mod config;` in `crates/mocktioneer-core/src/lib.rs`.
- Root **`mocktioneer.toml`** (1:1, no `[config]` wrapper): `bid_cpm = 0.20`.

**Runtime resolution — `AppConfig` extractor, fail-loud (R8 / blob model).**

> Superseded the original per-leaf `resolve_bid_cpm`/`get("bid_cpm")` design.
> edgezero `89f59266` stores the whole typed config as one canonical-JSON
> **blob envelope** (SHA-gated) under the store's key, read via the
> `AppConfig<C>` extractor — there is no per-key `get("bid_cpm")`.

`auction.rs`'s `build_openrtb_response` and `build_aps_response` take a
`cpm: f64` parameter (replacing direct `FIXED_BID_CPM` reads). The two handlers
read the typed config with the **bare, fail-loud extractor** and pass
`cfg.bid_cpm` in:

```rust
// routes.rs
use edgezero_core::extractor::AppConfig;
use crate::config::MocktioneerConfig;

#[action]
pub async fn handle_openrtb_auction(
    RequestContext(ctx): RequestContext,
    ForwardedHost(host): ForwardedHost,
    ValidatedJson(req): ValidatedJson<OpenRTBRequest>,
    AppConfig(cfg): AppConfig<MocktioneerConfig>,
) -> Result<Response, EdgeError> {
    // ... signature handling ...
    let resp = build_openrtb_response(&req, &host, signature_status, cfg.bid_cpm);
}

#[action]
pub async fn handle_aps_bid(
    ForwardedHost(host): ForwardedHost,
    ValidatedJson(req): ValidatedJson<ApsBidRequest>,
    AppConfig(cfg): AppConfig<MocktioneerConfig>,
) -> Result<Response, EdgeError> {
    let resp = build_aps_response(&req, &host, cfg.bid_cpm);
}
```

**Contract (fail-loud — the approved trade-off):** the `AppConfig` extractor
fetches the blob at the bound store's `default_key`, verifies the envelope SHA,
deserialises into `MocktioneerConfig`, and runs `validator`. Outcomes:

| Runtime situation                                        | Result                                                                 |
| -------------------------------------------------------- | ---------------------------------------------------------------------- |
| No `[stores.config]` / no config store bound             | **error** (`EdgeError::internal` "no default config store registered") |
| Store bound but **no blob pushed** yet                   | **error** (`config_out_of_date` — "run `config push`")                 |
| Blob present, valid                                      | typed `cfg.bid_cpm`                                                    |
| Blob present, value invalid (`bid_cpm` ≤ 0 / non-finite) | **error** (validation)                                                 |

So OpenRTB/APS **require a `config push` per deploy** before they serve;
`FIXED_BID_CPM` is the builders' default arg + the shipped `mocktioneer.toml`
value, **not** a runtime fallback. The static/creative/pixel endpoints don't
use the extractor and are unaffected. (The earlier graceful-fallback wrapper
`resolve_bid_cpm` was dropped per the user's R8 decision.)

### 3.6 Config lifecycle (declare → seed → bind → read)

Binding on the **serve path is automatic** once the store is declared and the
backing exists — `edgezero-adapter-axum/src/dev_server.rs:333 run_app` →
`build_config_registry(stores.config)` reads
`.edgezero/local-config-<id>.json`; Cloudflare (`request.rs:362`), Fastly
(`request.rs:382`), Spin (`config_store.rs` via `request::build_config_registry`)
have equivalents. The `app!` macro emits `stores().config` from
`[stores.config]`. Axum binds an empty store when the file is missing (it does
not skip the id); under the R8 fail-loud model the `AppConfig` extractor then
errors `config_out_of_date` (blob absent) — so the store must be **seeded with
a `config push`** before the auction/APS routes serve.

Per-adapter backing + seed step:

| Adapter                          | Backing for `mocktioneer_config`                                                                                                                                                                                                                                                      | Seed                                                                                                  |
| -------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| **Axum** (primary local/CI path) | `.edgezero/local-config-mocktioneer_config.json`                                                                                                                                                                                                                                      | `mocktioneer-cli config push --adapter axum` writes it.                                               |
| **Cloudflare**                   | KV namespace (config moved `[vars]`→KV in #269)                                                                                                                                                                                                                                       | `provision --adapter cloudflare` (writes id to `wrangler.toml`) + `config push --adapter cloudflare`. |
| **Fastly**                       | config store + `[setup]`/`[local_server]` in `fastly.toml`                                                                                                                                                                                                                            | `provision --adapter fastly` + `config push --adapter fastly`.                                        |
| **Spin**                         | **KV-backed** (`key_value_stores = ["mocktioneer_config"]` in `spin.toml` + a **`runtime-config.toml`** declaring the `[key_value_store.mocktioneer_config]` backend) — config is KV-backed in #269; `[variables]` is secrets-only now (`edgezero-adapter-spin/src/config_store.rs`). | `provision --adapter spin` + `config push --adapter spin`.                                            |

**Spin runtime-config gap (must add):** Mocktioneer's Spin adapter has only
`spin.toml` and **no `runtime-config.toml`**, and `edgezero.toml`'s spin
commands (`spin up/build/deploy --from …/spin.toml`) pass **no
`--runtime-config-file`**. KV-backed config needs both: add
`crates/mocktioneer-adapter-spin/runtime-config.toml` with a
`[key_value_store.mocktioneer_config]` entry, and update the
`[adapters.spin.commands]` `serve`/`deploy` lines (and docs) to pass
`--runtime-config-file crates/mocktioneer-adapter-spin/runtime-config.toml`.

**Pragmatics:** the **axum** path is the one exercised locally and in CI.
Cloud/Spin adapters get the manifest declaration + required native-backing
tables (incl. the Spin `runtime-config.toml`) so they _build and validate_;
push/provision are documented but live cloud/Spin stores are not stood up in
CI. Handler tests wire a `ConfigRegistry` fixture directly (the app-demo
`handlers.rs` test pattern) to cover the seeded-value, no-config (error),
and malformed-value (error) branches without a live backend.

### 3.7 `mocktioneer-cli` crate

`crates/mocktioneer-cli/`, mirroring `edgezero-cli/src/templates/cli/`:

- `Cargo.toml`: deps `mocktioneer-core`, `edgezero-cli`,
  `clap = { workspace = true }`, `log`. **Package metadata matching the other
  crates:** `publish = false`, `license.workspace = true`, and
  `[lints] workspace = true` (cf. `mocktioneer-core/Cargo.toml`).
- `src/main.rs`: clap `Args`/`Cmd` flattening
  `edgezero_cli::run_{auth,build,deploy,provision,serve}` + a typed
  `Config` subcommand dispatching `run_config_validate_typed`,
  `run_config_push_typed`, and (R8) `run_config_diff_typed::<MocktioneerConfig>`
  (the new `config diff` command — returns `DiffExit`; non-zero codes
  `process::exit`, all errors exit `2`).
- Add `crates/mocktioneer-cli` to root `[workspace].members`.
- **Dockerfile (cache hygiene, not a correctness fix):** the build already does
  `COPY crates ./crates` **before** `cargo fetch --locked` (Dockerfile:21/24),
  so the workspace fetch resolves regardless of the per-crate manifest
  pre-copies (Dockerfile:14-19). Those pre-copies exist only to create a
  dependency **cache layer**, and that layer is already incomplete — it omits
  the existing **spin** member. Bring it in line by adding the two missing
  member manifests:
  `COPY crates/mocktioneer-adapter-spin/Cargo.toml …` and
  `COPY crates/mocktioneer-cli/Cargo.toml …`. (The image still ships only the
  axum binary.)

### 3.8 Docs, agents, ignore files (verified surface)

- **Spin wasip1 → wasip2** + spin-sdk-6 note, across the files that reference
  it (rg-verified): `CLAUDE.md`, `edgezero.toml`, the spin crate files (§3.3),
  `.claude/agents/{code-architect,build-validator,verify-app}.md`,
  `docs/guide/getting-started.md`, `docs/guide/configuration.md`,
  `.cargo/config.toml` (comment), `.github/workflows/test.yml` (§3.9).
  **Leave Fastly wasip1 intact.** No `docs/guide/adapters/spin.md` exists.
- **Pricing docs — "$0.20 always/fixed" → "$0.20 default (configurable via
  `bid_cpm`)".** This change makes the fixed CPM a _default_, not an invariant,
  so the public claims must be reworded: `docs/guide/what-is-mocktioneer.md`,
  `docs/guide/architecture.md`, `docs/integrations/prebidjs.md`,
  `docs/integrations/prebid-server.md`, `docs/integrations/index.md`,
  `docs/api/openrtb-auction.md`, `docs/api/aps-bid.md`, `docs/api/index.md`
  (rg-verified). Mention the config + push path briefly; keep `0.20` as the
  shipped default.
- **edgezero-cli → mocktioneer-cli story (wider than VitePress docs).** After
  adding the in-repo `mocktioneer-cli`, distinguish the two everywhere they're
  referenced: `README.md`, `docs/guide/getting-started.md`,
  `docs/guide/adapters/index.md`, the **per-adapter pages** that show
  `edgezero-cli` examples — `docs/guide/adapters/axum.md`,
  `docs/guide/adapters/cloudflare.md`, `docs/guide/adapters/fastly.md` (keep
  `edgezero-cli` valid, add the in-repo `mocktioneer-cli` alternative
  alongside) — `tests/playwright/README.md`, and
  `tests/playwright/playwright.config.ts` (the `webServer` command). Rule:
  **`config validate`/`config push` are typed and live only in
  `mocktioneer-cli`**; `serve`/`build`/`deploy`/`auth`/`provision` work from
  either the external `edgezero-cli` or the vendored `mocktioneer-cli`. Update
  the "optional, not vendored" framing — `mocktioneer-cli` _is_ in-repo.
  Playwright's `webServer` should use `cargo run -p mocktioneer-cli -- serve
--adapter …` (no external install needed).
- **`.gitignore`:** add `.edgezero/` (currently only `.spin/` is ignored; the
  worktree already has an untracked `.edgezero/`).
- **`.claude/commands/check-ci.md`** and **`CLAUDE.md` "CI Gates"**: add the
  new `config validate --strict` gate (exact command in §3.9) so local CI docs
  aren't stale.
- **Docs formatter + VitePress exclusion (mandatory — this spec lives under
  `docs/`).** `docs/package.json`'s `format` runs `prettier --check .` and the
  format CI job runs it; it **fails on this spec file** today, and `npm run
build` renders specs/plans into `docs/.vitepress/dist/…/superpowers/…` (and a
  `.vitepress/.temp/`). Required: (a) add `superpowers/` to
  `docs/.prettierignore`; (b) add `srcExclude: ['**/superpowers/**']` to the
  VitePress config so internal specs aren't published; (c) ignore the build
  temp dir in **all three** tools — `.vitepress/.temp` in `docs/.prettierignore`
  AND `.vitepress/.temp/**` in `docs/eslint.config.js` `ignores` (ESLint's flat
  config has its own ignore list; a `.gitignore`/`.prettierignore` entry does
  not stop ESLint scanning it) AND `.vitepress/.temp` in `docs/.gitignore`;
  (d) **verify after a build, not before** — run `npm run build` first, then
  `npm run format && npm run lint`, and assert
  `find docs/.vitepress/dist docs/.vitepress/.temp -path '*superpowers*' -print
-quit` yields nothing. (The prior revision only checked `format` pre-build, so
  it missed the ~700 ESLint errors generated files produce.)

### 3.9 CI — `.github/workflows/test.yml`

- Spin matrix: target `wasip1` → `wasip2`; install the `wasm32-wasip2` rustup
  target; set `CARGO_TARGET_WASM32_WASIP2_RUNNER`; keep the pinned Wasmtime
  install and confirm it runs wasip2 components.
- `mocktioneer-cli` covered by `--workspace`.
- **Required gate** (also mirror `config validate --strict` into
  `.claude/commands/check-ci.md` and the `CLAUDE.md` CI-gates list). Seed a
  **non-default** value so the gate actually proves `bid_cpm` is wired — a bare
  `config push --adapter axum` would seed the root default (`0.20`) and a
  `test -f` would pass even if nothing were wired:
  ```sh
  cargo run -p mocktioneer-cli -- config validate --strict
  printf 'bid_cpm = 0.35\n' > /tmp/seed.toml
  cargo run -p mocktioneer-cli -- config push --adapter axum --yes --app-config /tmp/seed.toml
  # R8 blob model: bid_cpm lives inside the envelope under the store key.
  test "$(jq -r '.mocktioneer_config | fromjson | .data.bid_cpm' .edgezero/local-config-mocktioneer_config.json)" = "0.35"
  ```
  The handler → response half (seeded store → `0.35`) is proven deterministically
  by the registry-backed handler-dispatch test (§5, `auction_uses_seeded_cpm`),
  so no flaky serve+curl is needed in CI.
- **Docker:** `.github/workflows/docker.yml` builds the image. The Dockerfile
  change (§3.7) is cache hygiene only — `COPY crates` already precedes
  `cargo fetch`, so the build resolves regardless. A `docker build` smoke is a
  nice-to-have, not a gate.
- **Docs formatter is mandatory, not conditional:** the format CI job already
  fails on this spec, so the `docs/.prettierignore` + VitePress `srcExclude`
  changes (§3.8) must land in this branch.

## 4. Risks & mitigations

| Risk                                                       | Mitigation                                                                                                                                                          |
| ---------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| New parser rejects an existing `[adapters.*]` table        | Compile-time `manifest.validate()` surfaces it; fix per upstream error.                                                                                             |
| Spin SDK 6 macro/type churn beyond template                | Mirror edgezero's `edgezero-adapter-spin` verbatim; build wasip2.                                                                                                   |
| Wasmtime can't run the wasip2 component                    | Wasmtime 45.0.0 supports it; set `CARGO_TARGET_WASM32_WASIP2_RUNNER`; match edgezero's contract-test config.                                                        |
| Fresh dev errors before any push (R8 fail-loud)            | Intended: auction/APS require `config push` once per deploy; documented in `configuration.md`/README (§3.5).                                                        |
| Broken/malformed pushed _value_ masked as $0.20            | Read errors propagate; malformed present value errors (§3.5). Note a malformed _file_ degrades to fallback (bind-time drop), by design.                             |
| `bid_cpm` never exercised (store unseeded)                 | CI seeds a non-default `0.35` via `--app-config` and `jq`-asserts it round-trips; registry-backed `auction_uses_seeded_cpm` handler test covers the read path (§5). |
| Spin KV config silently empty (no `runtime-config.toml`)   | Add `runtime-config.toml` + `--runtime-config-file` to spin commands (§3.6).                                                                                        |
| Docker dependency-cache layer stale/incomplete             | Cache hygiene only — `COPY crates` precedes `cargo fetch`; add the missing spin + cli manifests to the pre-copy list (§3.7).                                        |
| Spec under `docs/` fails the format CI gate                | Mandatory `docs/.prettierignore` + VitePress `srcExclude` (§3.8).                                                                                                   |
| Pinning to an unmerged branch                              | Documented; re-pin to edgezero `main` post-merge.                                                                                                                   |
| `.cargo/config.toml.local` patch drift (`edgezero-macros`) | Already lists it; verify it patches cleanly.                                                                                                                        |

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
9. Runtime exercise — **explicit fixtures** (the root `mocktioneer.toml` ships
   `bid_cpm = 0.20`, so 0.35 must come from a seeded store, not the default):
   - **Registry-backed handler unit (R8):** build a `RequestContext` with a
     `ConfigRegistry` whose default store holds a **blob envelope** for
     `{ "bid_cpm": 0.35 }` (`StoreRegistry::single_id` + `ConfigStoreBinding` +
     `BlobEnvelope::new` over an in-memory `MapConfigStore`, inserted via
     `request.extensions_mut().insert(registry)` — the app-demo
     `config_flow.rs` pattern). Dispatch `handle_openrtb_auction` and assert the
     bid `price` is `0.35`; dispatch with **no** registry and assert the handler
     **errors** (fail-loud). The `0.20` default `ctx()` seeds the same way.
   - **CI seed path (axum):** `printf 'bid_cpm = 0.35\n' > /tmp/seed.toml` then
     `config push --adapter axum --yes --app-config /tmp/seed.toml`, and assert
     `jq -r '.mocktioneer_config | fromjson | .data.bid_cpm' …` == `0.35`
     (the blob envelope; not a bare push + `test -f`).
10. `docs/` gates pass: `cd docs && npm run format && npm run lint && npm run
build`, **and** `find docs/.vitepress/dist docs/.vitepress/.temp -path
'*superpowers*' -print -quit` produces no output (specs/plans excluded).
11. `docker build` succeeds with `mocktioneer-cli` in the workspace
    (nice-to-have; not a gate — the build resolves regardless per §3.7).

## 6. Rollback / follow-ups

- Rollback = revert the branch; deps return to edgezero `branch = "main"`.
- Follow-up: re-pin all `edgezero-*` deps to `main` once #269 merges.
- Pre-existing tech-debt (not fixed here): bid IDs use `Uuid::now_v7()`, which
  is non-deterministic despite the project's determinism guarantee.
