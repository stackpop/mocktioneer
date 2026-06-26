# Edgezero #269 (Extensible CLI) Adaptation — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

> **⚠️ Superseded in part by spec R8 (blob app-config sync to edgezero `89f59266`).**
> This plan was executed, then edgezero advanced past the pin with the **blob
> app-config cutover**. The runtime-read design below (Task 6:
> `cpm_from_lookup` / `resolve_bid_cpm` / per-leaf `get("bid_cpm")` / graceful
> `None → FIXED_BID_CPM` fallback) is **obsolete**. As implemented, the handlers
> use the **fail-loud `AppConfig<MocktioneerConfig>` extractor** (config is one
> SHA-gated blob envelope under the store key; a `config push` is required before
> auction/APS serve). The CLI also gained `config diff` (Task 8), and the CI
> assertion reads into the envelope (Task 12). See spec §3.5 (R8) and the source
> for the authoritative behaviour.

**Goal:** Adapt Mocktioneer to the breaking edgezero #269 API (extensible CLI, dropped `run_app` manifest arg, Spin SDK 6 / wasip2) and adopt typed `AppConfig`. (R8: the typed config is read at runtime via the fail-loud `AppConfig` extractor — `bid_cpm` requires a `config push`; `FIXED_BID_CPM` is the builder default, not a runtime fallback.)

**Architecture:** Pin the six `edgezero-*` git deps to `feature/extensible-cli`; fix every adapter entrypoint; migrate the Spin adapter to `spin-sdk ~6.0` / `wasm32-wasip2`; add a `MocktioneerConfig` typed-config struct + `mocktioneer.toml` + a `mocktioneer-cli` crate that mirrors edgezero's generated `<name>-cli`; thread a resolved `cpm` through the OpenRTB and APS bid builders; wire docs/CI/Docker/ignore files.

**Tech Stack:** Rust (edition 2021, workspace), edgezero framework, `spin-sdk` 6, `clap` 4, `validator`, `anyhow`, VitePress docs, GitHub Actions.

**Reference spec:** `docs/superpowers/specs/2026-06-11-edgezero-extensible-cli-adaptation-design.md`

**Branch:** `feature/edgezero-extensible-cli` (already created off `main`).

**Local iteration tip:** building against the unmerged `feature/extensible-cli` pulls git deps over the network. For fast local iteration you may symlink `.cargo/config.toml.local` (which path-patches `../edgezero`) — but **do not commit** it, and ensure the sibling `../edgezero` checkout is on `feature/extensible-cli`. CI uses the git pin with `--locked`.

---

## File Structure

| File                                                                                                                  | Responsibility                                                                                           | Action        |
| --------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- | ------------- |
| `docs/.prettierignore`, `docs/.vitepress/config.*`                                                                    | Keep internal specs/plans out of the docs format gate + published site                                   | Modify        |
| `Cargo.toml`                                                                                                          | Workspace dep pins (`feature/extensible-cli`), `spin-sdk ~6.0`, add `clap`; add `mocktioneer-cli` member | Modify        |
| `crates/mocktioneer-core/Cargo.toml`                                                                                  | Add `anyhow` dep                                                                                         | Modify        |
| `crates/mocktioneer-adapter-{axum,cloudflare,fastly}/src/*`                                                           | Drop `include_str!(manifest)` arg                                                                        | Modify        |
| `crates/mocktioneer-adapter-spin/{src/lib.rs,src/main.rs,Cargo.toml,spin.toml,tests/contract.rs,runtime-config.toml}` | Spin SDK 6 / wasip2 migration + KV config backing                                                        | Modify/Create |
| `edgezero.toml`                                                                                                       | `[stores.config]`, spin `target = wasip2`, spin commands `--runtime-config-file`                         | Modify        |
| `crates/mocktioneer-core/src/config.rs`                                                                               | `MocktioneerConfig` typed config                                                                         | Create        |
| `crates/mocktioneer-core/src/lib.rs`                                                                                  | `pub mod config;`                                                                                        | Modify        |
| `crates/mocktioneer-core/src/auction.rs`                                                                              | Thread `cpm: f64` into bid builders                                                                      | Modify        |
| `crates/mocktioneer-core/src/routes.rs`                                                                               | Fail-loud `AppConfig<MocktioneerConfig>` extractor on both handlers (R8)                                  | Modify        |
| `mocktioneer.toml`                                                                                                    | Typed config values (default `bid_cpm = 0.20`)                                                           | Create        |
| `crates/mocktioneer-cli/{Cargo.toml,src/main.rs}`                                                                     | Custom CLI mirroring edgezero `<name>-cli`                                                               | Create        |
| `Dockerfile`                                                                                                          | Pre-copy `mocktioneer-cli` manifest before `cargo fetch`                                                 | Modify        |
| `.gitignore`                                                                                                          | Ignore `.edgezero/`                                                                                      | Modify        |
| `docs/**`, `CLAUDE.md`, `.claude/...`, `README.md`, `tests/playwright/**`                                             | wasip2 + pricing + CLI-story docs                                                                        | Modify        |
| `.github/workflows/test.yml`                                                                                          | Spin wasip2 matrix + config-validate gate                                                                | Modify        |

---

## Task 0: Keep `docs/superpowers/**` out of the docs format gate & site

**Why first:** the format CI job runs `prettier --check .` inside `docs/`, which fails on the spec/plan markdown; VitePress also builds them into the published site. Do this before anything else so the spec and this plan don't break CI.

**Files:**

- Modify: `docs/.prettierignore`
- Modify: `docs/.vitepress/config.mts` (or `.ts`/`.js` — whichever exists)
- Test: `cd docs && npm run format`

- [ ] **Step 1: Add `superpowers/` AND the build temp dir to the docs Prettier ignore**

`docs/.prettierignore` currently lists `.vitepress/cache`, `.vitepress/dist`, `node_modules`. Append both the specs dir and the VitePress build temp dir (a `git`-ignore does NOT stop Prettier/ESLint from scanning a working-tree dir — the tool ignores must be set explicitly):

```
superpowers/
.vitepress/.temp
```

- [ ] **Step 2: Exclude internal specs/plans from the VitePress build**

Find the config file: `ls docs/.vitepress/config.*`. In its `defineConfig({ ... })` object, add a top-level `srcExclude` key (merge if one already exists):

```ts
  srcExclude: ['**/superpowers/**'],
```

- [ ] **Step 3: Ignore the VitePress build temp dir in ESLint AND git**

`npm run build` generates `.vitepress/.temp/` (≈31 JS files) which **ESLint will scan and fail on** (≈700+ errors) unless the flat config ignores it — and the ESLint ignore list is separate from `.prettierignore` and `.gitignore`. In `docs/eslint.config.js`, extend the `ignores` array (currently `['.vitepress/cache/**', '.vitepress/dist/**', 'node_modules/**']`) to add `.vitepress/.temp/**`:

```js
    ignores: [
      '.vitepress/cache/**',
      '.vitepress/dist/**',
      '.vitepress/.temp/**',
      'node_modules/**',
    ],
```

Also add `.vitepress/.temp` to `docs/.gitignore` (currently `node_modules`, `.vitepress/dist`, `.vitepress/cache`):

```
.vitepress/.temp
```

`docs/.vitepress/dist` is already gitignored and untracked — nothing to remove. Confirm: `git check-ignore docs/.vitepress/dist && echo ignored` → `ignored`.

- [ ] **Step 4: Build FIRST, then verify format + lint pass with generated files present**

Generate the temp/dist dirs, THEN run the gates (the prior revision only checked format _before_ a build, so it missed the generated-file failures):

Run: `cd docs && npm ci >/dev/null 2>&1 && npm run build && npm run format && npm run lint`
Expected: all PASS (the `.temp`/`dist`/`superpowers` ignores hold).

Assert no spec/plan leaked into the build output:

Run: `find docs/.vitepress/dist docs/.vitepress/.temp -path '*superpowers*' -print -quit`
Expected: **no output**. If anything prints, `srcExclude` is mis-scoped — fix the glob (e.g. `'**/superpowers/**'` relative to the VitePress `srcDir`) and rebuild.

- [ ] **Step 5: Commit**

```bash
git add docs/.prettierignore docs/eslint.config.js docs/.vitepress docs/.gitignore .gitignore
git commit -m "docs: exclude superpowers + vitepress build temp from prettier/eslint/vitepress"
```

---

## Task 1: Pin deps to `feature/extensible-cli` + workspace deps

**Files:**

- Modify: `Cargo.toml` (`[workspace.dependencies]`)
- Modify: `crates/mocktioneer-core/Cargo.toml`

- [ ] **Step 1: Repin the six edgezero deps and bump `spin-sdk`**

In `Cargo.toml` `[workspace.dependencies]`, change every `branch = "main"` on the `edgezero-*` lines to `branch = "feature/extensible-cli"` (6 lines: adapter-axum, adapter-cloudflare, adapter-fastly, adapter-spin, cli, core). Then replace the `spin-sdk` line:

```toml
spin-sdk = { version = "~6.0", default-features = false, features = ["http", "key-value", "variables"] }
```

- [ ] **Step 2: Add `clap` to workspace deps**

Add to `[workspace.dependencies]` (alphabetical, near `base64`):

```toml
clap = { version = "4", features = ["derive"] }
```

- [ ] **Step 3: Add `anyhow` to core**

In `crates/mocktioneer-core/Cargo.toml`, add to `[dependencies]` (alphabetical, before `async-trait`):

```toml
anyhow = { workspace = true }
```

- [ ] **Step 4: Resolve deps & regenerate `Cargo.lock`**

Run: `cargo check -p mocktioneer-core 2>&1 | tail -30`
Expected: dependency **resolution succeeds** (git deps fetched, `Cargo.lock` rewritten) and `mocktioneer-core` compiles (core doesn't touch `run_app`). If you instead see resolution errors (e.g. branch not found), fix the branch name before continuing.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/mocktioneer-core/Cargo.toml Cargo.lock
git commit -m "build: pin edgezero deps to feature/extensible-cli, bump spin-sdk to 6, add clap/anyhow"
```

---

## Task 2: Fix axum/cloudflare/fastly entrypoints (drop manifest arg)

**Files:**

- Modify: `crates/mocktioneer-adapter-axum/src/main.rs:7`
- Modify: `crates/mocktioneer-adapter-cloudflare/src/lib.rs:11-17`
- Modify: `crates/mocktioneer-adapter-fastly/src/main.rs:16`

- [ ] **Step 1: Axum — remove the manifest arg**

In `crates/mocktioneer-adapter-axum/src/main.rs`, change:

```rust
    if let Err(err) = run_app::<MocktioneerApp>(include_str!("../../../edgezero.toml")) {
```

to:

```rust
    if let Err(err) = run_app::<MocktioneerApp>() {
```

- [ ] **Step 2: Cloudflare — remove the manifest arg**

In `crates/mocktioneer-adapter-cloudflare/src/lib.rs`, change the call to:

```rust
    edgezero_adapter_cloudflare::run_app::<MocktioneerApp>(req, env, ctx).await
```

- [ ] **Step 3: Fastly — remove the manifest arg**

In `crates/mocktioneer-adapter-fastly/src/main.rs`, change:

```rust
    edgezero_adapter_fastly::run_app::<MocktioneerApp>(include_str!("../../../edgezero.toml"), req)
```

to:

```rust
    edgezero_adapter_fastly::run_app::<MocktioneerApp>(req)
```

- [ ] **Step 4: Verify native + wasm targets type-check (Spin still broken — that's Task 3)**

Run: `cargo check -p mocktioneer-adapter-axum`
Expected: PASS.

Run: `cargo check -p mocktioneer-adapter-fastly --features fastly --target wasm32-wasip1`
Expected: PASS.

Run: `cargo check -p mocktioneer-adapter-cloudflare --features cloudflare --target wasm32-unknown-unknown`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/mocktioneer-adapter-axum crates/mocktioneer-adapter-cloudflare crates/mocktioneer-adapter-fastly
git commit -m "feat: drop run_app manifest arg for axum/cloudflare/fastly (edgezero #269)"
```

---

## Task 3: Migrate the Spin adapter to spin-sdk 6 / wasm32-wasip2

**Files:**

- Modify: `crates/mocktioneer-adapter-spin/src/lib.rs`
- Modify: `crates/mocktioneer-adapter-spin/spin.toml`
- Create: `crates/mocktioneer-adapter-spin/runtime-config.toml`
- Modify: `crates/mocktioneer-adapter-spin/src/main.rs`
- Modify: `crates/mocktioneer-adapter-spin/tests/contract.rs`

- [ ] **Step 1: Rewrite `src/lib.rs` for `#[http_service]` + `Request`**

Replace the entire file with (mirrors edgezero's `templates/src/lib.rs.hbs`, dropping `no_main` and the manifest arg):

```rust
#![cfg_attr(
    target_arch = "wasm32",
    allow(
        unsafe_code,
        reason = "spin's #[http_service] macro generates the unsafe wasm export"
    )
)]

#[cfg(target_arch = "wasm32")]
use mocktioneer_core::MocktioneerApp;
#[cfg(target_arch = "wasm32")]
use spin_sdk::http::{IntoResponse, Request};
#[cfg(target_arch = "wasm32")]
use spin_sdk::http_service;

#[cfg(target_arch = "wasm32")]
#[http_service]
async fn handle(req: Request) -> anyhow::Result<impl IntoResponse> {
    edgezero_adapter_spin::run_app::<MocktioneerApp>(req).await
}
```

- [ ] **Step 2: Point `spin.toml` at wasip2 + declare the config KV store**

In `crates/mocktioneer-adapter-spin/spin.toml`, replace the `[component.mocktioneer]` and build blocks with:

```toml
[component.mocktioneer]
source = "../../target/wasm32-wasip2/release/mocktioneer_adapter_spin.wasm"
allowed_outbound_hosts = ["https://*:*"]
key_value_stores = ["mocktioneer_config"]

[component.mocktioneer.build]
command = "cargo build --target wasm32-wasip2 --release"
watch = ["src/**/*.rs", "Cargo.toml"]
```

- [ ] **Step 3: Create `runtime-config.toml`**

Create `crates/mocktioneer-adapter-spin/runtime-config.toml`:

```toml
# Spin runtime config: declares the KV labels the component may open.
# The `mocktioneer_config` label backs the EdgeZero `[stores.config]`
# store; the default SQLite backend persists to `.spin/sqlite_key_value.db`.
[key_value_store.mocktioneer_config]
type = "spin"
```

- [ ] **Step 4: Update the host-side stub message in `src/main.rs`**

Replace both `wasm32-wasip1` mentions with `wasm32-wasip2`:

```rust
#[expect(
    clippy::print_stderr,
    reason = "host-side stub that exists solely to remind the operator to target wasm32-wasip2"
)]
fn main() {
    eprintln!("Run `spin up` or target wasm32-wasip2 to execute mocktioneer-adapter-spin.");
}
```

- [ ] **Step 5: Update the contract-test comment to wasip2**

In `crates/mocktioneer-adapter-spin/tests/contract.rs`, change the doc-comment line `end-to-end under \`wasm32-wasip1\` via the \`wasmtime\` runner`to`wasm32-wasip2`. The `#![cfg(all(feature = "spin", target_arch = "wasm32"))]` gate is unchanged (covers wasip2).

- [ ] **Step 6: Verify the Spin wasm build + native workspace check**

Build `--release` to match the `spin.toml` `source` path (`target/wasm32-wasip2/release/...`):

Run: `rustup target add wasm32-wasip2 >/dev/null 2>&1; cargo build --release -p mocktioneer-adapter-spin --features spin --target wasm32-wasip2`
Expected: PASS — produces `target/wasm32-wasip2/release/mocktioneer_adapter_spin.wasm`. If the SDK-6 macro needs a tweak, fix per the compiler.

If the `spin` CLI is installed, also prove the manifest's source path resolves:

Run: `command -v spin >/dev/null && spin build --from crates/mocktioneer-adapter-spin/spin.toml || echo "spin CLI not installed — skipping"`
Expected: PASS or the skip note.

Run: `cargo check --workspace`
Expected: PASS (native).

- [ ] **Step 7: Commit**

```bash
git add crates/mocktioneer-adapter-spin
git commit -m "feat: migrate spin adapter to spin-sdk 6 / wasm32-wasip2 (edgezero #269)"
```

---

## Task 4: Manifest — declare config store, spin wasip2 target, runtime-config

**Files:**

- Modify: `edgezero.toml`

- [ ] **Step 1: Declare the config store**

Add a top-level `[stores.config]` section to `edgezero.toml` (place it after `[app]`, before the triggers):

```toml
[stores.config]
ids = ["mocktioneer_config"]
```

- [ ] **Step 2: Spin build target → wasip2**

In `edgezero.toml`, in `[adapters.spin.build]`, change `target = "wasm32-wasip1"` to:

```toml
target = "wasm32-wasip2"
```

- [ ] **Step 3: Spin serve/deploy commands → pass `--runtime-config-file`**

In `[adapters.spin.commands]`, change `serve` and `deploy` to reference the runtime config:

```toml
build = "spin build --from crates/mocktioneer-adapter-spin/spin.toml"
deploy = "spin deploy --from crates/mocktioneer-adapter-spin/spin.toml --runtime-config-file crates/mocktioneer-adapter-spin/runtime-config.toml"
serve = "spin up --from crates/mocktioneer-adapter-spin/spin.toml --runtime-config-file crates/mocktioneer-adapter-spin/runtime-config.toml"
```

- [ ] **Step 4: Verify the manifest still compiles (validated at compile time by `app!`)**

Run: `cargo check -p mocktioneer-core`
Expected: PASS. If `manifest.validate()` rejects a table, fix per its error message.

- [ ] **Step 5: Commit**

```bash
git add edgezero.toml
git commit -m "feat: declare [stores.config] + spin wasip2/runtime-config in manifest"
```

---

## Task 5: `MocktioneerConfig` typed config struct

**Files:**

- Create: `crates/mocktioneer-core/src/config.rs`
- Modify: `crates/mocktioneer-core/src/lib.rs`

- [ ] **Step 1: Write the failing validation tests**

Create `crates/mocktioneer-core/src/config.rs`:

```rust
//! Typed application config, loaded from `mocktioneer.toml`. The TOML file
//! maps 1:1 onto this struct (no `[config]` wrapper). v1 carries a single
//! `bid_cpm` field; `config validate --strict` enforces the rules below.

use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Deserialize, Serialize, Validate, edgezero_core::AppConfig)]
#[serde(deny_unknown_fields)]
pub struct MocktioneerConfig {
    /// Fixed bid CPM in USD. Strictly positive (`exclusive_min` rejects 0.0,
    /// negatives, and NaN; non-finite floats also rejected by edgezero's
    /// loader). `_f64` suffix satisfies the strict `default_numeric_fallback`
    /// clippy lint.
    #[validate(range(exclusive_min = 0.0_f64))]
    pub bid_cpm: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_positive_finite_cpm() {
        let cfg = MocktioneerConfig { bid_cpm: 0.20 };
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn rejects_zero_negative_and_non_finite() {
        for bad in [0.0_f64, -1.0, f64::NAN, f64::INFINITY] {
            let cfg = MocktioneerConfig { bid_cpm: bad };
            assert!(cfg.validate().is_err(), "expected {bad} to be rejected");
        }
    }
}
```

- [ ] **Step 2: Wire the module**

In `crates/mocktioneer-core/src/lib.rs`, add to the top module list (alphabetical, after `aps`):

```rust
pub mod config;
```

- [ ] **Step 3: Run the tests (expect FAIL first if the derive isn't wired, then PASS)**

Run: `cargo test -p mocktioneer-core config::tests`
Expected: PASS. (If the `edgezero_core::AppConfig` derive path is wrong, the compile error tells you — it is re-exported from `edgezero-core`, no `edgezero-macros` dep needed.)

- [ ] **Step 4: Commit**

```bash
git add crates/mocktioneer-core/src/config.rs crates/mocktioneer-core/src/lib.rs
git commit -m "feat: add MocktioneerConfig typed config (bid_cpm, validated)"
```

---

## Task 6: Thread `cpm` through the bid builders + runtime resolution

> **⚠️ Read model below is OBSOLETE (spec R8).** Keep the `cpm: f64` builder
> parameter threading, but the runtime read is NOT `cpm_from_lookup` /
> `resolve_bid_cpm` / `get("bid_cpm")`. As implemented: `handle_openrtb_auction`
> and `handle_aps_bid` take `AppConfig(cfg): AppConfig<MocktioneerConfig>` and
> pass `cfg.bid_cpm` to the builders — fail-loud (errors with no pushed config).
> Tests seed a **blob envelope** via a `ConfigRegistry` fixture
> (`StoreRegistry::single_id` + `ConfigStoreBinding` + `BlobEnvelope::new`).

**Files:**

- Modify: `crates/mocktioneer-core/src/auction.rs` (`build_openrtb_response`, `build_aps_response`, tests)
- Modify: `crates/mocktioneer-core/src/routes.rs` (`handle_openrtb_auction`, `handle_aps_bid`, new helpers, imports)

### As built (R8 fail-loud blob model)

> The original step-by-step (pure `cpm_from_lookup`, `resolve_bid_cpm`, and a
> `None → FIXED_BID_CPM` fallback) was **removed** — it predates the blob
> cutover and must not be implemented. The behaviour that shipped:

1. **Builders take `cpm: f64`.** `build_openrtb_response(req, host, sig, cpm)`
   and `build_aps_response(req, host, cpm)` replace their direct `FIXED_BID_CPM`
   reads with the parameter; `FIXED_BID_CPM` stays the builders' default arg +
   the shipped `mocktioneer.toml` value. Every `auction.rs` /
   `tests/aps_endpoints.rs` call site passes `FIXED_BID_CPM` explicitly (and do
   **not** touch the separate `mediation.rs::build_openrtb_response`).

2. **Handlers read typed config via the fail-loud `AppConfig` extractor.** Add
   `use edgezero_core::extractor::AppConfig;` and
   `use crate::config::MocktioneerConfig;`, then:

   ```rust
   #[action]
   pub async fn handle_openrtb_auction(
       RequestContext(ctx): RequestContext,
       ForwardedHost(host): ForwardedHost,
       ValidatedJson(req): ValidatedJson<OpenRTBRequest>,
       AppConfig(cfg): AppConfig<MocktioneerConfig>,
   ) -> Result<Response, EdgeError> { /* … build_openrtb_response(&req, &host, sig, cfg.bid_cpm) */ }

   #[action]
   pub async fn handle_aps_bid(
       ForwardedHost(host): ForwardedHost,
       ValidatedJson(req): ValidatedJson<ApsBidRequest>,
       AppConfig(cfg): AppConfig<MocktioneerConfig>,
   ) -> Result<Response, EdgeError> { /* … build_aps_response(&req, &host, cfg.bid_cpm) */ }
   ```

   With no store bound / no blob pushed, the extractor errors
   (`config_out_of_date`) — auction/APS require a `config push`. `FIXED_BID_CPM`
   is dropped from `routes.rs` imports (no longer referenced there).

3. **Tests seed a blob.** The `routes.rs` test `ctx()` helper seeds the default
   config store with a blob envelope (`StoreRegistry::single_id` +
   `ConfigStoreBinding` + `BlobEnvelope::new` over an in-memory `MapConfigStore`
   holding `{ "mocktioneer_config": "<envelope for {bid_cpm}>" }`), so the
   existing handler tests keep exercising their 400/422 paths. Add
   `auction_uses_seeded_cpm` (seed `0.35` → assert bid `price == 0.35`) and
   `auction_without_config_errors` (no registry → handler errors). The
   `tests/endpoints.rs` router auction test inserts the same registry.

4. **Verify + commit.**

   Run: `cargo test -p mocktioneer-core && cargo clippy -p mocktioneer-core --all-targets --all-features -- -D warnings`
   Expected: PASS.

   ```bash
   git add crates/mocktioneer-core/src/auction.rs crates/mocktioneer-core/src/routes.rs crates/mocktioneer-core/tests/endpoints.rs
   git commit -m "feat: resolve bid_cpm from typed config (fail-loud AppConfig extractor, blob model)"
   ```

---

## Task 7: `mocktioneer.toml` typed config file

**Files:**

- Create: `mocktioneer.toml` (repo root, next to `edgezero.toml`)

- [ ] **Step 1: Create the config file**

`config validate`/`push` resolve `<app_name>.toml` next to the manifest from `[app].name = "mocktioneer"`. Create `mocktioneer.toml`:

```toml
# Typed application config for Mocktioneer (maps 1:1 onto MocktioneerConfig).
# `bid_cpm` is the default; override per-environment via `config push` or the
# `MOCKTIONEER__BID_CPM` env overlay. Default keeps historical $0.20 behavior.
bid_cpm = 0.20
```

- [ ] **Step 2: Commit (validation happens in Task 8 once the CLI exists)**

```bash
git add mocktioneer.toml
git commit -m "feat: add mocktioneer.toml typed config (bid_cpm default 0.20)"
```

---

## Task 8: `mocktioneer-cli` crate

> **R8 addition:** the crate also wires the new `config diff` command —
> `MocktioneerConfigCmd::Diff(ConfigDiffArgs)` dispatching
> `edgezero_cli::run_config_diff_typed::<MocktioneerConfig>` (returns `DiffExit`;
> non-zero codes `process::exit`, all errors exit `2`). `new` is intentionally
> omitted (see R7).

**Files:**

- Create: `crates/mocktioneer-cli/Cargo.toml`
- Create: `crates/mocktioneer-cli/src/main.rs`
- Modify: `Cargo.toml` (`[workspace].members`)

- [ ] **Step 1: Add the crate to the workspace members**

In root `Cargo.toml`, add to `[workspace].members` (after the spin adapter line):

```toml
  "crates/mocktioneer-cli",
```

- [ ] **Step 2: Create `crates/mocktioneer-cli/Cargo.toml`**

```toml
[package]
name = "mocktioneer-cli"
version = "0.1.0"
edition = "2021"
publish = false
license.workspace = true

[dependencies]
mocktioneer-core = { workspace = true }
edgezero-cli = { workspace = true }
clap = { workspace = true }
log = { workspace = true }

[lints]
workspace = true
```

- [ ] **Step 3: Create `crates/mocktioneer-cli/src/main.rs`**

Instantiates edgezero's `<name>-cli` template (name=`mocktioneer`, struct=`MocktioneerConfig`):

```rust
//! Mocktioneer CLI — built on the `edgezero-cli` library.
//!
//! Reuses every built-in edgezero command and adds the **typed** `config`
//! arms parameterised over `MocktioneerConfig`, so `validator` rules run on
//! `config validate` / `config push`.

use clap::{Parser, Subcommand};
use edgezero_cli::args::{
    AuthArgs, BuildArgs, ConfigPushArgs, ConfigValidateArgs, DeployArgs, ProvisionArgs, ServeArgs,
};
use mocktioneer_core::config::MocktioneerConfig;

#[derive(Parser, Debug)]
#[command(name = "mocktioneer-cli", about = "mocktioneer edge CLI")]
struct Args {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Sign in / out / status against the adapter's native CLI.
    Auth(AuthArgs),
    /// Build the project for a target edge.
    Build(BuildArgs),
    /// Inspect or mutate the typed `mocktioneer.toml` app config.
    #[command(subcommand)]
    Config(MocktioneerConfigCmd),
    /// Deploy to a target edge.
    Deploy(DeployArgs),
    /// Create the platform resources backing the declared store ids.
    Provision(ProvisionArgs),
    /// Run a local simulation (adapter-specific).
    Serve(ServeArgs),
}

/// Dispatches `validate`/`push` to the typed entry points over
/// `MocktioneerConfig`.
#[derive(Subcommand, Debug)]
enum MocktioneerConfigCmd {
    /// Push `mocktioneer.toml` (flattened) to the adapter's config store.
    Push(ConfigPushArgs),
    /// Validate `edgezero.toml` + `mocktioneer.toml` against `MocktioneerConfig`.
    Validate(ConfigValidateArgs),
}

fn main() {
    use std::process;

    edgezero_cli::init_cli_logger();
    let result = match Args::parse().cmd {
        Cmd::Auth(args) => edgezero_cli::run_auth(&args),
        Cmd::Build(args) => edgezero_cli::run_build(&args),
        Cmd::Config(MocktioneerConfigCmd::Push(args)) => {
            edgezero_cli::run_config_push_typed::<MocktioneerConfig>(&args)
        }
        Cmd::Config(MocktioneerConfigCmd::Validate(args)) => {
            edgezero_cli::run_config_validate_typed::<MocktioneerConfig>(&args)
        }
        Cmd::Deploy(args) => edgezero_cli::run_deploy(&args),
        Cmd::Provision(args) => edgezero_cli::run_provision(&args),
        Cmd::Serve(args) => edgezero_cli::run_serve(&args),
    };
    if let Err(err) = result {
        log::error!("[mocktioneer] {err}");
        process::exit(1);
    }
}
```

- [ ] **Step 4: Build the CLI**

Run: `cargo build -p mocktioneer-cli`
Expected: PASS. If `run_auth`/`run_provision` names differ, check `edgezero-cli`'s `lib.rs` re-exports and adjust (the generated template uses exactly these names).

- [ ] **Step 5: Validate the typed config end to end**

Run: `cargo run -p mocktioneer-cli -- config validate --strict`
Expected: PASS (manifest + `mocktioneer.toml` validate against `MocktioneerConfig`).

- [ ] **Step 6: Sanity-check rejection**

Run: `printf 'bid_cpm = -1\n' > /tmp/bad-mocktioneer.toml && cargo run -p mocktioneer-cli -- config validate --strict --app-config /tmp/bad-mocktioneer.toml; echo "exit=$?"`
Expected: non-zero exit with a validation error about `bid_cpm`.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock crates/mocktioneer-cli
git commit -m "feat: add mocktioneer-cli with typed config validate/push"
```

---

## Task 9: Dockerfile — keep the dependency-cache layer correct

**Why (accuracy note):** the build already does `COPY crates ./crates` **before** `cargo fetch --locked` (Dockerfile:21/24), so the workspace fetch resolves regardless of the per-crate manifest pre-copies (lines 16-19). Those pre-copies exist only to create a **cache layer** (manifests change rarely → fetch is cached across source edits). That layer is currently already incomplete — it omits the existing **spin** member. So this task is **cache hygiene**, not a correctness fix: bring the pre-copy list in line with the actual member set.

**Files:**

- Modify: `Dockerfile`

- [ ] **Step 1: Add the missing member manifests to the cache layer**

In `Dockerfile`, after the existing per-crate manifest COPY lines (after the fastly line, before `COPY crates ./crates`), add the two members currently missing from the pre-copy list:

```dockerfile
COPY crates/mocktioneer-adapter-spin/Cargo.toml crates/mocktioneer-adapter-spin/Cargo.toml
COPY crates/mocktioneer-cli/Cargo.toml crates/mocktioneer-cli/Cargo.toml
```

- [ ] **Step 2: Verify the build (if Docker is available)**

Run: `docker build -t mocktioneer:plan-check . 2>&1 | tail -20`
Expected: `cargo fetch --locked` resolves and the build completes. (If Docker isn't available, note it and rely on CI; the build is correct either way because `COPY crates` precedes `cargo fetch`.)

- [ ] **Step 3: Commit**

```bash
git add Dockerfile
git commit -m "build: copy missing adapter manifests before cargo fetch in Docker"
```

---

## Task 10: Ignore `.edgezero/`

**Files:**

- Modify: `.gitignore`

- [ ] **Step 1: Add the ignore entry**

Add `.edgezero/` to `.gitignore` (next to the existing `.spin/` line):

```
.edgezero/
```

- [ ] **Step 2: Verify the worktree no longer shows it**

Run: `git status --porcelain | grep edgezero || echo clean`
Expected: `clean`.

- [ ] **Step 3: Commit**

```bash
git add .gitignore
git commit -m "chore: gitignore .edgezero/ (local config/kv state)"
```

---

## Task 11: Docs, agents, CI-command docs

**Files:**

- Modify (Spin wasip1→wasip2): `CLAUDE.md`, `docs/guide/getting-started.md`, `docs/guide/configuration.md`, `.claude/agents/code-architect.md`, `.claude/agents/build-validator.md`, `.claude/agents/verify-app.md`, `.cargo/config.toml` (comment only)
- Modify (pricing default): `docs/guide/what-is-mocktioneer.md`, `docs/guide/architecture.md`, `docs/integrations/prebidjs.md`, `docs/integrations/prebid-server.md`, `docs/integrations/index.md`, `docs/api/openrtb-auction.md`, `docs/api/aps-bid.md`, `docs/api/index.md`
- Modify (CLI story): `README.md`, `docs/guide/adapters/index.md`, `docs/guide/adapters/axum.md`, `docs/guide/adapters/cloudflare.md`, `docs/guide/adapters/fastly.md`, `docs/guide/getting-started.md`, `tests/playwright/README.md`, `tests/playwright/playwright.config.ts`
- Modify (CI gate docs): `.claude/commands/check-ci.md`, `CLAUDE.md`

- [ ] **Step 1: Spin target references → wasip2**

In each file listed under "Spin wasip1→wasip2", change Spin-context `wasm32-wasip1` → `wasm32-wasip2`. **Do not** touch Fastly's `wasm32-wasip1` references. Verify scope first:

Run: `grep -rn "wasip1" CLAUDE.md docs/guide/getting-started.md docs/guide/configuration.md .claude/agents`
For each hit, confirm whether it's Spin (change) or Fastly (leave). In `CLAUDE.md` update the layout comment `Spin / Fermyon bridge (wasm32-wasip1)` and the adapter targets table Spin row to wasip2.

- [ ] **Step 2: Pricing docs — "$0.20 always/fixed" → "$0.20 default (configurable)"**

In each pricing doc, reword the fixed-price claim. Example for `docs/api/openrtb-auction.md`:

> Every bid is priced at a **default** CPM of **$0.20**, configurable via the `bid_cpm` key in `mocktioneer.toml` (pushed to the adapter's config store with `mocktioneer-cli config push`).

Apply the equivalent one-line edit to each of the 8 files. Verify you caught them:

Run: `grep -rn "0.20\|fixed price\|always" docs/guide/what-is-mocktioneer.md docs/guide/architecture.md docs/integrations docs/api`

- [ ] **Step 3: CLI story — distinguish `edgezero-cli` vs `mocktioneer-cli`**

- `README.md`, `docs/guide/adapters/index.md`, `docs/guide/getting-started.md`: note that `config validate`/`config push` are typed and live in the in-repo `mocktioneer-cli` (`cargo run -p mocktioneer-cli -- …`); `serve`/`build`/`deploy` work from either the external `edgezero-cli` or `mocktioneer-cli`. Drop "optional, not vendored" framing for the config commands.
- **Per-adapter pages** — `docs/guide/adapters/axum.md` (≈L22), `docs/guide/adapters/cloudflare.md` (≈L47), `docs/guide/adapters/fastly.md` (≈L36) each show `edgezero-cli serve/build/deploy` examples. Keep `edgezero-cli` as valid but add a one-line "or, in-repo: `cargo run -p mocktioneer-cli -- <same args>`" alongside, so the vendored CLI is mentioned consistently. Find them first:

  Run: `grep -rn "edgezero-cli" docs/guide/adapters/`

- `tests/playwright/README.md` and `tests/playwright/playwright.config.ts`: change the `webServer` launch command to `cargo run -p mocktioneer-cli -- serve --adapter cloudflare` (no external install). Confirm the exact current command first:

  Run: `grep -n "edgezero-cli\|webServer\|command" tests/playwright/playwright.config.ts`

- [ ] **Step 4: Add the config-validate gate to local CI docs**

In `.claude/commands/check-ci.md`, add a step 5:

```markdown
5. `cargo run -p mocktioneer-cli -- config validate --strict`
```

In `CLAUDE.md` "CI Gates" list, add the same `config validate --strict` gate.

- [ ] **Step 5: Verify docs formatter still passes**

Run: `cd docs && npm run format`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add CLAUDE.md docs .claude README.md tests/playwright .cargo/config.toml
git commit -m "docs: spin wasip2, bid_cpm-default pricing, edgezero-cli vs mocktioneer-cli, config gate"
```

---

## Task 12: CI — Spin wasip2 matrix + config-validate gate

**Files:**

- Modify: `.github/workflows/test.yml`

- [ ] **Step 1: Inspect the current Spin matrix entry**

Run: `grep -n "wasip1\|wasip2\|RUNNER\|target:\|spin" .github/workflows/test.yml`
Identify the Spin matrix row (currently `target: wasm32-wasip1`, `runner_env: CARGO_TARGET_WASM32_WASIP1_RUNNER`).

- [ ] **Step 2: Switch the Spin matrix row to wasip2**

For the Spin entry only (leave the Fastly entry on wasip1): change `target: wasm32-wasip1` → `target: wasm32-wasip2` and `runner_env: CARGO_TARGET_WASM32_WASIP1_RUNNER` → `CARGO_TARGET_WASM32_WASIP2_RUNNER`. Ensure the rustup target install step uses the matrix `target` (so it adds `wasm32-wasip2`). Keep the pinned Wasmtime install.

- [ ] **Step 3: Add the typed-config gate job/step**

Add a step (in the existing native test job, after `cargo test`):

```yaml
- name: Validate typed app config
  run: cargo run -p mocktioneer-cli -- config validate --strict

- name: Seed a NON-default cpm and assert it round-trips (axum)
  run: |
    printf 'bid_cpm = 0.35\n' > /tmp/seed.toml
    cargo run -p mocktioneer-cli -- config push --adapter axum --yes --app-config /tmp/seed.toml
    # R8 blob model: bid_cpm is inside the envelope under the store key.
    test "$(jq -r '.mocktioneer_config | fromjson | .data.bid_cpm' .edgezero/local-config-mocktioneer_config.json)" = "0.35"
    rm -f .edgezero/local-config-mocktioneer_config.json
```

A **bare** `config push --adapter axum` would silently seed the root `mocktioneer.toml` default (`0.20`) and a `test -f` only proves a file exists — it would pass even if `bid_cpm` were never wired. Seeding `0.35` via `--app-config` and asserting the JSON value with `jq` proves push writes the _configured_ value. The handler → response half (a seeded store yielding `0.35`) is proven deterministically by the registry-backed `auction_uses_seeded_cpm` handler test (R8), so no flaky serve+curl is needed here. (`jq` is preinstalled on GitHub `ubuntu-latest`.)

- [ ] **Step 4: Lint the workflow locally**

Prefer `actionlint` (validates GitHub Actions schema, not just YAML):

Run: `command -v actionlint >/dev/null && actionlint .github/workflows/test.yml || echo "actionlint not installed"`
Expected: PASS, or the not-installed note.

Fallback YAML well-formedness check **only if PyYAML is available** (`pip install pyyaml`; it is not in the stdlib, so don't rely on it in a clean env):

Run: `python -c "import importlib.util,sys; sys.exit(0) if importlib.util.find_spec('yaml') is None else __import__('yaml').safe_load(open('.github/workflows/test.yml'))" && echo "yaml-ok-or-skipped"`

- [ ] **Step 5: Commit**

```bash
git add .github/workflows/test.yml
git commit -m "ci: spin wasip2 matrix + typed config validate/seed gate"
```

---

## Task 13: Full verification pass

**Files:** none (verification only)

- [ ] **Step 1: Format + lint**

Run: `cargo fmt --all -- --check`
Expected: PASS.

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Expected: PASS.

- [ ] **Step 2: Tests**

Run: `cargo test --workspace --all-targets`
Expected: PASS.

- [ ] **Step 3: Feature/target matrix**

Run: `cargo check --workspace --all-targets --features "fastly cloudflare"`
Expected: PASS.

Run: `cargo build -p mocktioneer-adapter-fastly --features fastly --target wasm32-wasip1`
Expected: PASS.

Run: `cargo build --release -p mocktioneer-adapter-spin --features spin --target wasm32-wasip2`
Expected: PASS.

Run: `cargo build -p mocktioneer-adapter-cloudflare --features cloudflare --target wasm32-unknown-unknown`
Expected: PASS.

- [ ] **Step 4: Typed config + seeded read-back**

Run: `cargo run -p mocktioneer-cli -- config validate --strict`
Expected: PASS.

Run: `printf 'bid_cpm = 0.35\n' > /tmp/seed.toml && cargo run -p mocktioneer-cli -- config push --adapter axum --yes --app-config /tmp/seed.toml && jq -r '.mocktioneer_config | fromjson | .data.bid_cpm' .edgezero/local-config-mocktioneer_config.json`
Expected: prints `0.35` (R8 blob envelope). (Then remove it: `rm -f .edgezero/local-config-mocktioneer_config.json`.)

- [ ] **Step 5: Docs gates (incl. spec/plan exclusion assertion)**

Run: `cd docs && npm run format && npm run lint && npm run build`
Expected: PASS.

Run: `find docs/.vitepress/dist docs/.vitepress/.temp -path '*superpowers*' -print -quit`
Expected: **no output** (specs/plans excluded from the published build).

- [ ] **Step 6: Semantic parity vs `main` (R8: seed config first)**

Under R8 the auction/APS endpoints are fail-loud, so seed first:
`cargo run -p mocktioneer-cli -- config push --adapter axum --yes`. Then (via
`cargo run -p mocktioneer-adapter-axum`) hit `/openrtb2/auction` and
`/e/dtb/bid` and confirm prices are `0.20` and the responses match `main` on
semantic fields (price, sizes, `cur`, creative URLs, targeting) — IDs differ by
design (`Uuid::now_v7()`). Without a pushed blob both endpoints return
`config_out_of_date` (expected).

- [ ] **Step 7: Final commit (if any verification fixups were needed)**

```bash
git add -A
git commit -m "chore: verification fixups for edgezero #269 adaptation"
```

---

## Self-Review (plan vs spec)

- **§3.1 deps/anyhow/clap/spin-sdk** → Task 1. ✓
- **§3.2 adapter entrypoints** → Tasks 2 (axum/cf/fastly) + 3 (spin). ✓
- **§3.3 Spin wasip2 + runner** → Task 3 (adapter) + Task 12 (CI runner env). ✓
- **§3.4 manifest `[stores.config]`** → Task 4. ✓
- **§3.5 typed struct + runtime contract** → Task 5 (struct) + Task 6 (R8: fail-loud `AppConfig` extractor; no pushed blob → error; invalid value → validation error). ✓
- **§3.6 config lifecycle (per-adapter backing, spin runtime-config)** → Task 3 (spin runtime-config.toml) + Task 4 (commands) + Task 8 (push) + Task 12 (axum seed). Cloud/Spin provision documented, not CI-stood-up, per spec. ✓
- **§3.7 mocktioneer-cli (metadata/lints) + Dockerfile** → Task 8 + Task 9. ✓
- **§3.8 docs (wasip2, pricing default, CLI story, gitignore, check-ci, prettier/VitePress)** → Task 0 (prettier/VitePress) + Task 10 (gitignore) + Task 11 (rest). ✓
- **§3.9 CI** → Task 12. ✓
- **§5 verification (incl. explicit seeded fixture, docker, prettier)** → Task 13 (+ R8 registry-backed handler tests in Task 6 covering seeded-0.35 and no-config-error; non-default `0.35` seed + `jq` envelope assert in Tasks 12/13; docs build-exclusion `find` check in Tasks 0/13). ✓

No placeholders; types/functions (`MocktioneerConfig`, the `AppConfig` extractor, builder signatures with `cpm: f64`) are consistent across tasks (R8).

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-06-15-edgezero-extensible-cli-adaptation.md`. Two execution options:

**1. Subagent-Driven (recommended)** — a fresh subagent per task, two-stage review between tasks, fast iteration.

**2. Inline Execution** — execute tasks in this session with batch checkpoints (executing-plans).

Which approach?
