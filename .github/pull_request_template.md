## Summary

<!-- 1-3 bullet points describing what this PR does and why -->

-

## Changes

<!-- Which crates/files were modified and what changed in each -->

| Crate / File | Change |
| ------------ | ------ |
|              |        |

## Closes

<!-- Link to the issue this PR resolves. Every PR should have a ticket. -->
<!-- Use "Closes #123" syntax to auto-close the issue when merged. -->

Closes #

## Test plan

<!-- How did you verify this works? Check all that apply -->

- [ ] `cargo test --workspace --all-targets`
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- [ ] `cargo check --workspace --all-targets --features "fastly cloudflare"`
- [ ] Manual testing via `cargo run -p mocktioneer-adapter-axum`
- [ ] Playwright e2e tests (`cd tests/playwright && npm test`)
- [ ] Other: <!-- describe -->

## Checklist

- [ ] Changes follow [CLAUDE.md](/CLAUDE.md) conventions
- [ ] Business logic lives in `mocktioneer-core`, not in adapter crates
- [ ] New routes added to both `routes.rs` and `edgezero.toml`
- [ ] Determinism preserved — no randomness or time-dependent logic
- [ ] Ad markup rendered via templates in `render.rs`, not inlined in handlers
- [ ] New code has tests (colocated `#[cfg(test)]` or in `tests/`)
- [ ] No secrets or credentials committed
