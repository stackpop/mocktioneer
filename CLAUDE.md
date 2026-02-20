# CLAUDE.md — Mocktioneer

## Project Overview

Mocktioneer is a deterministic OpenRTB banner bidder for edge platforms. It lets
you test client integrations (Prebid.js, Prebid Server, custom SDKs) without
depending on third-party bidders or origin backends. Write once, deploy to
Fastly Compute, Cloudflare Workers, or native Axum servers. The codebase is a
Cargo workspace with 4 crates under `crates/`, a VitePress documentation site
under `docs/`, Playwright e2e tests under `tests/playwright/`, and CI workflows
under `.github/workflows/`.

## Workspace Layout

```
crates/
  mocktioneer-core/               # Shared business logic: OpenRTB, APS, auction, render, routes
  mocktioneer-adapter-axum/       # Native Axum HTTP server
  mocktioneer-adapter-cloudflare/ # Cloudflare Workers bridge (wasm32-unknown-unknown)
  mocktioneer-adapter-fastly/     # Fastly Compute bridge (wasm32-wasip1)
docs/                             # VitePress documentation site (Node.js)
examples/                         # curl/shell scripts for endpoint demos
tests/playwright/                 # Playwright e2e tests (creative visibility, sizes)
```

## Toolchain & Versions

- **Rust**: 1.91.1 (from `.tool-versions`)
- **Node.js**: 24.12.0 (for docs site and Playwright tests)
- **Fastly CLI**: v13.0.0
- **Edition**: 2021
- **Resolver**: 2
- **License**: Apache-2.0

## Build & Test Commands

```sh
# Full workspace test (primary CI command)
cargo test --workspace --all-targets

# Test a specific crate
cargo test -p mocktioneer-core

# Lint & format (must pass CI)
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings

# Run the native Axum server locally (port 8787)
cargo run -p mocktioneer-adapter-axum

# Run via EdgeZero CLI
edgezero-cli serve --adapter cloudflare   # Cloudflare on :8787
edgezero-cli serve --adapter fastly       # Fastly on :7676

# Playwright e2e tests
cd tests/playwright && npm test

# Docs site
cd docs && npm ci && npm run dev
```

**Always run `cargo test` after touching code.** Use `-p mocktioneer-core` for
faster iteration since nearly all business logic lives there.

## Compilation Targets

| Adapter    | Target                   | Notes                              |
| ---------- | ------------------------ | ---------------------------------- |
| Fastly     | `wasm32-wasip1`          | Requires Viceroy for local testing |
| Cloudflare | `wasm32-unknown-unknown` | Requires `wrangler` for dev/deploy |
| Axum       | Native (host triple)     | Standard Tokio runtime             |

## Coding Conventions

### Architecture

- **Single source of truth in `mocktioneer-core`.** All business logic, OpenRTB
  types, route handlers, render helpers, and tests live in the core crate.
- **`edgezero.toml` is configuration authority.** Routes, middleware, adapter
  commands, and logging are all manifest-driven.
- **Adapters stay thin.** They only translate adapter APIs (request/response,
  logging, config). If you're duplicating core logic, move it to `mocktioneer-core`.

### Route Handlers

All handlers live in `mocktioneer-core/src/routes.rs`. New routes must be added
to both the handler module and `edgezero.toml`:

```rust
use edgezero_core::{action, Json, Path, Query, Response, EdgeError};

#[action]
async fn my_handler(
    Json(body): Json<MyPayload>,
) -> Result<Response, EdgeError> {
    // handler body
}
```

Use `{id}` brace syntax for path parameters (matchit 0.8+), **not** `:id`.

### Validation

Use the `validator` crate with `#[derive(Validate)]` on request structs.
Custom validation functions for complex rules (e.g., size format). Failed
validation returns 400 Bad Request with error details.

### Error Handling

- Use `EdgeError` with semantic constructors: `EdgeError::validation()`,
  `EdgeError::internal()`, etc.
- Use `thiserror` and `anyhow` for context-rich errors.
- Prefer `Result<Response, EdgeError>` as handler return type.

### Rendering

Templates live in `crates/mocktioneer-core/static/templates/` and are rendered
through `render.rs`. Do not inline ad markup in handlers.

### Logging

- Use the `log` facade, not direct dependencies.
- Adapter-specific init happens at the adapter level.
- Use `simple_logger` for local/Axum builds.

### Style Rules

- **WASM compatibility first**: avoid Tokio and runtime-specific deps in core.
  Use `async-trait` without `Send` bounds.
- **Colocate tests** with implementation modules (`#[cfg(test)]` in the same file).
- **Async tests** use `futures::executor::block_on` (not Tokio) for WASM compat.
- **Minimal changes**: every change should impact as little code as possible.
  Avoid unnecessary refactoring, docstrings on untouched code, or premature abstractions.
- **No direct `http` crate imports** — use `edgezero_core` re-exports.
- **Determinism**: no randomness, no time-dependent pricing. Same input always
  produces the same output.

## Module Structure (mocktioneer-core/src/)

| Module            | Purpose                                          |
| ----------------- | ------------------------------------------------ |
| `lib.rs`          | App bootstrapper via `edgezero_core::app!` macro |
| `routes.rs`       | All HTTP handlers + query struct validation      |
| `auction.rs`      | Size pricing, CPM calculation, standard sizes    |
| `openrtb.rs`      | OpenRTB 2.x request/response types               |
| `aps.rs`          | APS TAM API types & bid handling                 |
| `mediation.rs`    | Multi-bidder mediation logic                     |
| `render.rs`       | Creative HTML/SVG rendering via Handlebars       |
| `verification.rs` | Ed25519 signature validation                     |

## Key Constants

- `DEFAULT_CPM: f64 = 1.50` — base price for non-standard sizes
- `MAX_AREA_BONUS: f64 = 3.00` — area-based bonus cap
- `SIZE_MAP` — 13 standard IAB sizes via `phf::Map` (300x250, 728x90, 320x50, etc.)

## CI Gates

Every PR must pass:

1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace --all-targets --all-features -- -D warnings`
3. `cargo test --workspace --all-targets`
4. `cargo check --workspace --all-targets --features "fastly cloudflare"`
5. Playwright e2e tests (`tests/playwright/`)
6. ESLint + Prettier on `docs/`

Docker image is built and pushed to `ghcr.io/stackpop/mocktioneer` on push to
main and on releases.

## Standard Workflow

1. **Read & plan**: think through the problem, read the codebase for relevant
   files, and present a plan as a checklist inline in the conversation.
2. **Get approval first**: show the full plan and get approval before commencing
   any coding work.
3. **Implement incrementally**: work through the checklist items. Make every
   task and code change as simple as possible — every change should impact as
   little code as possible.
4. **Test after every change**: run `cargo test` (or scoped `-p mocktioneer-core`)
   after touching any code.
5. **Explain as you go**: after completing each item, give a high-level
   explanation of what changes you made.
6. **If blocked**: explain what's blocking and why.

## Verification & Quality

- **Verify, don't assume**: after implementing a change, prove it works. Run
  tests, check `cargo clippy`, and compare behavior against `main` when relevant.
  Don't say "it works" without evidence.
- **Plan review**: for complex tasks, review your own plan as a staff engineer
  would before implementing. Ask: is this the simplest approach? Does it touch
  too many files? Are there edge cases?
- **Escape hatch**: if an implementation is going sideways after multiple
  iterations, step back and reconsider. Scrap the approach and implement the
  simpler solution rather than patching a flawed design.
- **Use subagents**: for tasks spanning multiple crates or requiring broad
  codebase exploration, use subagents to parallelize investigation and keep the
  main context clean.

## Subagents

Specialized agents live in `.claude/agents/`. Use them to distribute work:

| Agent             | Purpose                                                              |
| ----------------- | -------------------------------------------------------------------- |
| `code-simplifier` | Simplifies code after work is done — dead code, duplication, nesting |
| `verify-app`      | End-to-end verification: tests, lint, WASM builds, server smoke test |
| `build-validator` | Validates builds across all targets and feature combinations         |
| `code-architect`  | Architectural review — evaluates designs against project principles  |
| `pr-creator`      | Analyzes changes, runs CI gates, creates/updates GitHub PRs          |
| `issue-creator`   | Creates typed GitHub issues using project templates and GraphQL API  |
| `repo-explorer`   | Read-only codebase exploration — maps components, paths, and risks   |

Invoke with "use subagents" in your prompt or reference a specific agent by name.

### Subagent Workflow (Required for Complex Tasks)

For tasks spanning multiple crates, adapters, or unclear failures, use this flow:

1. **Phase 1 — Parallel investigation (read-only)**:
   launch 2-4 subagents with non-overlapping scopes (tests, diff review, architecture checks).
   Each subagent must return concrete findings with file paths and line references.
2. **Phase 2 — Parallel solution proposals (no edits)**:
   launch at least 2 subagents to propose minimal fix strategies based on Phase 1 findings.
   Compare tradeoffs (scope, risk, CI impact) before coding.
3. **Phase 3 — Single-path implementation**:
   pick one plan (smallest safe change) and implement centrally.
   Do not let multiple subagents edit overlapping files at the same time.
4. **Phase 4 — Verification handoff**:
   run `verify-app` or `build-validator` for broad changes, then summarize pass/fail by step.
5. **Phase 5 — Decision log**:
   report which subagents were used, what each found, and why the chosen plan was selected.

Default trigger:

- If work touches 2+ crates or includes both runtime behavior and build/tooling changes, use this workflow.

### Subagent Selection Matrix

| Situation                  | Use first         | Optional follow-up                  | Expected output                   |
| -------------------------- | ----------------- | ----------------------------------- | --------------------------------- |
| Unfamiliar code area       | `repo-explorer`   | `code-architect`                    | File map and risk hotspots        |
| Multi-crate feature change | `repo-explorer`   | `code-architect`, `build-validator` | Change plan and validation scope  |
| CI/build failures          | `build-validator` | `repo-explorer`                     | Failing combos and fault area     |
| Design/API proposal        | `code-architect`  | `repo-explorer`                     | Architecture concerns and options |
| Cleanup/refactor pass      | `code-simplifier` | `build-validator`                   | Simplification summary and checks |
| Pre-PR readiness           | `build-validator` | `verify-app`, `pr-creator`          | Pass/fail report and PR draft     |

Use at least 2 subagents when:

- The task touches 2+ crates.
- The change affects both runtime behavior and CI/build tooling.

## Slash Commands

Custom commands live in `.claude/commands/`:

| Command           | Purpose                                            |
| ----------------- | -------------------------------------------------- |
| `/check-ci`       | Run all CI gate checks locally                     |
| `/test-all`       | Run full workspace test suite                      |
| `/test-crate`     | Run tests for a specific crate                     |
| `/review-changes` | Staff-engineer-level review of uncommitted changes |
| `/verify`         | Prove current changes work vs main                 |

## Available MCPs

- **Context7 MCP**: use for up-to-date library docs and coding examples.
- **Playwright MCP**: use for UI validation and browser console debugging.
  Iterate up to 5 times per feature; if still stuck, stop and report.

## Key Files Reference

| Purpose            | Path                                               |
| ------------------ | -------------------------------------------------- |
| Workspace manifest | `Cargo.toml`                                       |
| EdgeZero manifest  | `edgezero.toml`                                    |
| Core crate entry   | `crates/mocktioneer-core/src/lib.rs`               |
| Route handlers     | `crates/mocktioneer-core/src/routes.rs`            |
| Auction logic      | `crates/mocktioneer-core/src/auction.rs`           |
| OpenRTB types      | `crates/mocktioneer-core/src/openrtb.rs`           |
| APS types          | `crates/mocktioneer-core/src/aps.rs`               |
| Mediation          | `crates/mocktioneer-core/src/mediation.rs`         |
| Render engine      | `crates/mocktioneer-core/src/render.rs`            |
| Verification       | `crates/mocktioneer-core/src/verification.rs`      |
| Templates          | `crates/mocktioneer-core/static/templates/`        |
| Axum adapter entry | `crates/mocktioneer-adapter-axum/src/main.rs`      |
| Cloudflare adapter | `crates/mocktioneer-adapter-cloudflare/src/lib.rs` |
| Fastly adapter     | `crates/mocktioneer-adapter-fastly/src/main.rs`    |
| Playwright tests   | `tests/playwright/`                                |
| Example scripts    | `examples/`                                        |
| CI tests           | `.github/workflows/test.yml`                       |
| CI format/lint     | `.github/workflows/format.yml`                     |
| Docker build       | `.github/workflows/docker.yml`                     |
| Docs site          | `docs/`                                            |

## Dependencies Philosophy

- Workspace-level dependency management via `[workspace.dependencies]` in root `Cargo.toml`.
- Minimal, carefully curated for WASM compatibility.
- `Cargo.lock` is committed for reproducible builds.
- Key crates: `edgezero-*` (framework), `serde`/`serde_json` (serialization),
  `validator` (input validation), `handlebars` (templates), `phf` (static maps),
  `ed25519-dalek` (signatures), `uuid` (request IDs).
- Optional `.cargo/config.toml.local` for local edgezero development without
  re-publishing.

## What NOT to Do

- Don't use legacy `:id` route syntax — always use `{id}`.
- Don't import from `http` crate directly — use `edgezero_core` re-exports.
- Don't add Tokio dependencies to the core crate.
- Don't write tests that require a network connection or platform credentials.
- Don't duplicate logic from `mocktioneer-core` into adapter crates.
- Don't inline ad markup in handlers — use templates in `render.rs`.
- Don't make large, sweeping refactors — keep changes minimal and focused.
- Don't commit without running `cargo test` first.
- Don't skip `cargo fmt` and `cargo clippy` — CI will reject the PR.
- Don't introduce non-deterministic behavior (randomness, time-dependent logic).
- Don't include `Co-Authored-By` trailers, "Generated with" footers, or any AI bylines in commits or PR bodies.
