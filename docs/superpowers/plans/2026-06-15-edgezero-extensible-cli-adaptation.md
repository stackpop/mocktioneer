# Edgezero #269 (Extensible CLI) Adaptation — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Adapt Mocktioneer to the breaking edgezero #269 API (extensible CLI, dropped `run_app` manifest arg, Spin SDK 6 / wasip2) and adopt typed `AppConfig` so `bid_cpm` is configurable at runtime, defaulting to `FIXED_BID_CPM`.

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
| `crates/mocktioneer-core/src/routes.rs`                                                                               | `resolve_bid_cpm` + `cpm_from_lookup` helpers; wire both handlers                                        | Modify        |
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
use validator::{Validate, ValidationError};

#[derive(Debug, Deserialize, Serialize, Validate, edgezero_core::AppConfig)]
#[serde(deny_unknown_fields)]
pub struct MocktioneerConfig {
    /// Fixed bid CPM in USD. Validated finite and strictly positive.
    #[validate(custom(function = "validate_bid_cpm"))]
    pub bid_cpm: f64,
}

/// `validator` custom fns take a reference to the field; `range` does not
/// reject NaN / inf for floats, so validate explicitly.
fn validate_bid_cpm(value: &f64) -> Result<(), ValidationError> {
    if value.is_finite() && *value > 0.0 {
        Ok(())
    } else {
        Err(ValidationError::new("bid_cpm_must_be_finite_positive"))
    }
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

**Files:**

- Modify: `crates/mocktioneer-core/src/auction.rs` (`build_openrtb_response`, `build_aps_response`, tests)
- Modify: `crates/mocktioneer-core/src/routes.rs` (`handle_openrtb_auction`, `handle_aps_bid`, new helpers, imports)

- [ ] **Step 1: Write the failing pure-helper tests in `routes.rs`**

In `crates/mocktioneer-core/src/routes.rs`, inside the existing `#[cfg(test)] mod tests { ... }` block, add:

```rust
    #[test]
    fn cpm_from_lookup_falls_back_when_absent() {
        assert_eq!(
            cpm_from_lookup(None).unwrap().to_bits(),
            crate::auction::FIXED_BID_CPM.to_bits()
        );
    }

    #[test]
    fn cpm_from_lookup_parses_valid_value() {
        assert!((cpm_from_lookup(Some("0.35".to_owned())).unwrap() - 0.35).abs() < f64::EPSILON);
    }

    #[test]
    fn cpm_from_lookup_rejects_bad_values() {
        for bad in ["-1", "0", "abc", "inf", "NaN", ""] {
            assert!(
                cpm_from_lookup(Some(bad.to_owned())).is_err(),
                "expected {bad:?} to error"
            );
        }
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p mocktioneer-core routes::tests::cpm_from_lookup 2>&1 | tail -20`
Expected: FAIL — `cannot find function cpm_from_lookup`.

- [ ] **Step 3: Add the resolution helpers + import**

In `crates/mocktioneer-core/src/routes.rs`, extend the auction import to include `FIXED_BID_CPM`:

```rust
use crate::auction::{
    build_aps_response, build_openrtb_response, is_standard_size, standard_sizes, FIXED_BID_CPM,
};
```

Add these two functions near the handlers (module scope, not inside a fn):

```rust
/// Interpret a config-store lookup for `bid_cpm`. Pure (no `ctx`) so it is
/// unit-testable. `None` (store absent / unseeded / key missing) → default;
/// a present-but-unparseable / non-finite / ≤0 value → error.
fn cpm_from_lookup(found: Option<String>) -> Result<f64, EdgeError> {
    match found {
        None => Ok(FIXED_BID_CPM),
        Some(raw) => raw
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite() && *value > 0.0)
            .ok_or_else(|| {
                EdgeError::internal(anyhow::anyhow!(
                    "config store `mocktioneer_config` has malformed bid_cpm: {raw:?}"
                ))
            }),
    }
}

/// Resolve the effective bid CPM from the bound default config store, falling
/// back to `FIXED_BID_CPM` when no store is bound. A backend read error
/// propagates via `From<ConfigStoreError>`.
async fn resolve_bid_cpm(ctx: &RequestContext) -> Result<f64, EdgeError> {
    let Some(store) = ctx.config_store_default() else {
        return Ok(FIXED_BID_CPM);
    };
    cpm_from_lookup(store.get("bid_cpm").await.map_err(EdgeError::from)?)
}
```

- [ ] **Step 4: Run the pure-helper tests (expect PASS)**

Run: `cargo test -p mocktioneer-core routes::tests::cpm_from_lookup`
Expected: PASS.

- [ ] **Step 4b: Add registry-backed `resolve_bid_cpm` tests (spec §5 preferred fixture)**

Exercises the real `RequestContext` → `ConfigRegistry` → `config_store_default()` → `store.get` path with an in-memory store (the app-demo `config_flow.rs` pattern; all types are public, no `test-utils` feature). Add to the `#[cfg(test)] mod tests` block in `crates/mocktioneer-core/src/routes.rs`:

```rust
    // In-memory ConfigStore for tests (mirrors app-demo's MapConfigStore).
    struct MapConfigStore(std::collections::HashMap<String, String>);

    // `ConfigStore` is declared `#[async_trait(?Send)]` in edgezero-core, so
    // the impl MUST use the same `(?Send)` mode or method signatures won't match.
    #[async_trait(?Send)]
    impl edgezero_core::config_store::ConfigStore for MapConfigStore {
        async fn get(
            &self,
            key: &str,
        ) -> Result<Option<String>, edgezero_core::config_store::ConfigStoreError> {
            Ok(self.0.get(key).cloned())
        }
    }

    fn ctx_with_config(pairs: &[(&str, &str)]) -> RequestContext {
        use edgezero_core::config_store::ConfigStoreHandle;
        use edgezero_core::store_registry::{ConfigRegistry, StoreRegistry};
        use std::collections::{BTreeMap, HashMap};
        use std::sync::Arc;

        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        let handle = ConfigStoreHandle::new(Arc::new(MapConfigStore(map)));
        let by_id: BTreeMap<String, ConfigStoreHandle> =
            [("mocktioneer_config".to_owned(), handle)].into_iter().collect();
        let registry: ConfigRegistry =
            StoreRegistry::new(by_id, "mocktioneer_config".to_owned());

        let mut request = request_builder()
            .method(Method::GET)
            .uri("/")
            .body(Body::empty())
            .expect("request");
        request.extensions_mut().insert(registry);
        RequestContext::new(request, PathParams::new(std::collections::HashMap::new()))
    }

    #[test]
    fn resolve_bid_cpm_reads_seeded_store() {
        let ctx = ctx_with_config(&[("bid_cpm", "0.35")]);
        let cpm = futures::executor::block_on(resolve_bid_cpm(&ctx)).unwrap();
        assert!((cpm - 0.35).abs() < f64::EPSILON);
    }

    #[test]
    fn resolve_bid_cpm_falls_back_without_registry() {
        let request = request_builder()
            .method(Method::GET)
            .uri("/")
            .body(Body::empty())
            .expect("request");
        let ctx = RequestContext::new(request, PathParams::new(std::collections::HashMap::new()));
        let cpm = futures::executor::block_on(resolve_bid_cpm(&ctx)).unwrap();
        assert_eq!(cpm.to_bits(), FIXED_BID_CPM.to_bits());
    }

    #[test]
    fn resolve_bid_cpm_errors_on_malformed_value() {
        let ctx = ctx_with_config(&[("bid_cpm", "-1")]);
        assert!(futures::executor::block_on(resolve_bid_cpm(&ctx)).is_err());
    }
```

Note: `async_trait`, `request_builder`, `Method`, `Body`, `PathParams` are already imported in the test module (confirmed). `futures` is a dev-dependency of `mocktioneer-core`.

Run: `cargo test -p mocktioneer-core routes::tests::resolve_bid_cpm`
Expected: PASS (after Step 3's helpers exist).

- [ ] **Step 5: Add `cpm: f64` to the bid builders (write the new builder tests first)**

In `crates/mocktioneer-core/src/auction.rs`, inside its `#[cfg(test)] mod tests`, add:

```rust
    #[test]
    fn openrtb_uses_supplied_cpm() {
        let req = OpenRTBRequest {
            id: "rc".to_owned(),
            imp: vec![OpenrtbImp {
                id: "1".to_owned(),
                banner: Some(Banner {
                    width: Some(300),
                    height: Some(250),
                    ..Default::default()
                }),
                ..Default::default()
            }],
            ..Default::default()
        };
        let resp = build_openrtb_response(&req, "host.test", test_signature(), 0.35);
        assert!((resp.seatbid[0].bid[0].price - 0.35).abs() < f64::EPSILON);
    }

    #[test]
    fn aps_uses_supplied_cpm() {
        let req = ApsBidRequest {
            pub_id: "test".to_owned(),
            slots: vec![ApsSlot {
                slot_id: "slot1".to_owned(),
                sizes: vec![[300, 250]],
                slot_name: None,
            }],
            page_url: None,
            user_agent: None,
            timeout: None,
        };
        let resp = build_aps_response(&req, "mock.test", 0.35);
        let slot = &resp.contextual.slots[0];
        let price = decode_aps_price(slot.amznbid.as_ref().unwrap()).unwrap();
        assert!((price - 0.35).abs() < f64::EPSILON);
    }
```

- [ ] **Step 6: Change the builder signatures + bodies**

In `build_openrtb_response`, add the param and use it:

```rust
pub fn build_openrtb_response(
    req: &OpenRTBRequest,
    base_host: &str,
    signature_status: SignatureStatus,
    cpm: f64,
) -> OpenRTBResponse {
```

Replace the deprecation `log::warn!` argument `FIXED_BID_CPM` with `cpm`, and replace `let price = FIXED_BID_CPM;` (≈line 120) with `let price = cpm;`.

In `build_aps_response`:

```rust
pub fn build_aps_response(req: &ApsBidRequest, base_host: &str, cpm: f64) -> ApsBidResponse {
```

Replace `let price = FIXED_BID_CPM;` (≈line 259) with `let price = cpm;`.

- [ ] **Step 7: Update ALL auction.rs builder call sites in tests**

There are **many** test call sites, not two. Enumerate them:

Run: `grep -rn "build_openrtb_response\|build_aps_response" crates/mocktioneer-core/src/auction.rs | grep -v "pub fn "`
Expected: ~9 test call sites (e.g. lines ~344, 368, 389, 409, 436, 455, 488, 510, 563).

Append `, FIXED_BID_CPM` to every `build_openrtb_response(&req, "host.test", test_signature())` and every `build_aps_response(&req, "mock.test")` call in `auction.rs` tests. They keep asserting against `FIXED_BID_CPM`, which is now the value they pass in.

**Do NOT touch `crates/mocktioneer-core/src/mediation.rs:183/187`** — that is a _separate, private_ `build_openrtb_response(request.id, request.imp, winning_bids, base_host)` for the mediation path. It is fed by request bids, never `FIXED_BID_CPM`, and is out of scope for `cpm`.

Run after editing: `cargo build -p mocktioneer-core 2>&1 | grep -c "this function takes" || echo "no arity errors"` to confirm no missed call sites.

- [ ] **Step 8: Wire the handlers**

In `crates/mocktioneer-core/src/routes.rs`:

`handle_openrtb_auction` — resolve cpm and pass it (it already binds `RequestContext(ctx)`):

```rust
    let cpm = resolve_bid_cpm(&ctx).await?;
    log::info!("auction id={}, imps={}", req.id, req.imp.len());
    let resp = build_openrtb_response(&req, &host, signature_status, cpm);
```

`handle_aps_bid` — add the `RequestContext(ctx)` extractor and resolve cpm:

```rust
#[action]
pub async fn handle_aps_bid(
    RequestContext(ctx): RequestContext,
    ForwardedHost(host): ForwardedHost,
    ValidatedJson(req): ValidatedJson<ApsBidRequest>,
) -> Result<Response, EdgeError> {
    log::info!(
        "APS auction pubId={}, slots={}",
        req.pub_id,
        req.slots.len()
    );

    let cpm = resolve_bid_cpm(&ctx).await?;
    let resp = build_aps_response(&req, &host, cpm);
```

- [ ] **Step 9: Run the full core test suite**

Run: `cargo test -p mocktioneer-core`
Expected: PASS (new builder tests + updated existing tests + config tests).

- [ ] **Step 10: Commit**

```bash
git add crates/mocktioneer-core/src/auction.rs crates/mocktioneer-core/src/routes.rs
git commit -m "feat: resolve bid_cpm from config store at runtime (OpenRTB + APS), default FIXED_BID_CPM"
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
    cargo run -p mocktioneer-cli -- config push --adapter axum --app-config /tmp/seed.toml
    test "$(jq -r '.bid_cpm' .edgezero/local-config-mocktioneer_config.json)" = "0.35"
    rm -f .edgezero/local-config-mocktioneer_config.json
```

A **bare** `config push --adapter axum` would silently seed the root `mocktioneer.toml` default (`0.20`) and a `test -f` only proves a file exists — it would pass even if `bid_cpm` were never wired. Seeding `0.35` via `--app-config` and asserting the JSON value with `jq` proves push writes the _configured_ value. The handler → response half (a seeded store yielding `0.35`) is proven deterministically by the registry-backed `resolve_bid_cpm` test (Task 6, Step 4b), so no flaky serve+curl is needed here. (`jq` is preinstalled on GitHub `ubuntu-latest`.)

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

Run: `printf 'bid_cpm = 0.35\n' > /tmp/seed.toml && cargo run -p mocktioneer-cli -- config push --adapter axum --app-config /tmp/seed.toml && cat .edgezero/local-config-mocktioneer_config.json`
Expected: the JSON contains `bid_cpm` = `0.35`. (Then remove it: `rm -f .edgezero/local-config-mocktioneer_config.json`.)

- [ ] **Step 5: Docs gates (incl. spec/plan exclusion assertion)**

Run: `cd docs && npm run format && npm run lint && npm run build`
Expected: PASS.

Run: `find docs/.vitepress/dist docs/.vitepress/.temp -path '*superpowers*' -print -quit`
Expected: **no output** (specs/plans excluded from the published build).

- [ ] **Step 6: Semantic parity vs `main` (no store bound)**

With no `.edgezero/local-config-*` present, hit `/openrtb2/auction` and `/e/dtb/bid` (via `cargo run -p mocktioneer-adapter-axum`) and confirm prices are `0.20` and the responses match `main` on semantic fields (price, sizes, `cur`, creative URLs, targeting) — IDs differ by design (`Uuid::now_v7()`).

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
- **§3.5 typed struct + runtime contract (incl. malformed-file vs read-error)** → Task 5 (struct) + Task 6 (`cpm_from_lookup`/`resolve_bid_cpm`; `None`→fallback covers absent/empty/missing-key; malformed value→error; read error→`From`). ✓
- **§3.6 config lifecycle (per-adapter backing, spin runtime-config)** → Task 3 (spin runtime-config.toml) + Task 4 (commands) + Task 8 (push) + Task 12 (axum seed). Cloud/Spin provision documented, not CI-stood-up, per spec. ✓
- **§3.7 mocktioneer-cli (metadata/lints) + Dockerfile** → Task 8 + Task 9. ✓
- **§3.8 docs (wasip2, pricing default, CLI story, gitignore, check-ci, prettier/VitePress)** → Task 0 (prettier/VitePress) + Task 10 (gitignore) + Task 11 (rest). ✓
- **§3.9 CI** → Task 12. ✓
- **§5 verification (incl. explicit seeded fixture, docker, prettier)** → Task 13 (+ pure `cpm_from_lookup` tests **and** registry-backed `resolve_bid_cpm` tests in Task 6 Step 4b covering seeded-0.35 / fallback / malformed-error; non-default `0.35` seed + `jq` assert in Tasks 12/13; docs build-exclusion `find` check in Tasks 0/13). ✓

No placeholders; types/functions (`MocktioneerConfig`, `cpm_from_lookup`, `resolve_bid_cpm`, builder signatures with `cpm: f64`) are consistent across tasks.

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-06-15-edgezero-extensible-cli-adaptation.md`. Two execution options:

**1. Subagent-Driven (recommended)** — a fresh subagent per task, two-stage review between tasks, fast iteration.

**2. Inline Execution** — execute tasks in this session with batch checkpoints (executing-plans).

Which approach?
