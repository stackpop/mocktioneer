# Mocktioneer Agents Guide

This guide gives coding agents a concise playbook for contributing to Mocktioneer while keeping the codebase DRY and consistent across edge adapters.

## Standard Workflow

1. First think through the problem, read the codebase for relevant files, and write a plan to the file: TODO.md. If the TODO.md file does not yet exist, go ahead and create it.
2. The plan should have a list of todo items that you can check off as you complete them
3. Before you begin working, show me the full plan of the work to be done and get my approval before commencing any coding work
4. Then, begin working on the todo items, marking them as complete as you go.
5. As you complete each todo item, give me a high level explanation of what changes you made
6. If you cannot complete a todo, mark it as blocked in TODO.md and explain why.
7. Make every task and code change you do as simple as possible. We want to avoid making any massive or complex changes. Every change should impact as little code as possible.
8. Finally, add a review section to the TODO.md file with a summary of the changes you made, assumptions made, any unresolved issues or errors you couldn't fix, and any other relevant information. Add the date and time showing when the job was finished.

## Tools

### Context7 MCP

- Use Context7 MCP to supplement local codebase knowledge with up-to-date library documentation and coding examples

### Playwright MCP

- use the Playwright MCP in order to check and validate any user interface changes you make
- you can also use this MCP to check the web browser console for any error messages which may come up
- iterate up to a maximum of 5 times for any one particular feature; an iteration = one cycle of code change + Playwright re-run
- if you can't get the feature to work after 5 iterations then you're likely stuck in a loop and you should stop and let me know that you've hit your max of 5 iterations for that feature
- Save any failing test cases or console errors in a debug.md file so we can review them together.

## Core Principles

- **Single source of truth lives in `mocktioneer-core`.** All business logic, OpenRTB types, route handlers, render helpers, and tests should be implemented (or reused) from the shared crate.
- **`anyedge.toml` is configuration authority.** Declare new routes, adapter commands, and logging defaults in the manifest so adapters and the CLI stay aligned.
- **Adapters stay thin.** `mocktioneer-adapter-fastly` and `mocktioneer-adapter-cloudflare` should only translate adapter APIs (request/response, logging, config). If you find yourself duplicating logic from `mocktioneer-core`, move it into the core crate first.
- **Templates are canonical.** Update HTML/SVG templates in `crates/mocktioneer-core/static/templates/` and render them through `render.rs`. Do not inline ad markup in handlers.
- **Tests prevent drift.** Extend `crates/mocktioneer-core/tests/endpoints.rs` or add unit tests near the code you touch. Adapter crates should stay free of business logic tests beyond thin smoke checks.
- **Prefer helper functions over copy/paste.** When a handler or renderer grows shared behaviour (e.g., size parsing, query parsing), extract it into `routes.rs` or `render.rs` as a dedicated helper and reuse it everywhere.

## DRY Playbook

- Adding a new endpoint? Implement it in `mocktioneer-core/src/routes.rs` and add it to `anyedge.toml`; the manifest-driven router will pick it up for both adapters.
- Introducing new OpenRTB fields? Update the data structures in `mocktioneer-core/src/openrtb.rs`, adjust serializers once, and reuse them across responses.
- Need shared constants (sizes, cookie names, headers)? Centralize them in the core crate rather than duplicating strings in handlers or tests.
- Shared price/size calculations belong in `auction.rs`; keep creative rendering details in `render.rs` so they are not reimplemented downstream.
- When modifying templates, add regression coverage (e.g., assert substrings in `render.rs` tests) to make sure both edge adapters stay aligned.
- Keep configuration parsing (`config.rs`) authoritative. If a new setting is required, parse and validate it once there and read it from the adapters after validation.

## Workflow Tips

1. Start changes in `mocktioneer-core`; adapters should only require wiring updates when adapter APIs change.
2. Run `cargo fmt` and `cargo test` from the repo root before sending patches.
3. For Fastly-specific checks, run `anyedge-cli serve --adapter fastly` (wraps `fastly compute serve -C crates/mocktioneer-adapter-fastly` via the manifest) so you exercise the same config the Cloudflare adapter reads. If you prefer not to install the binary, use `cargo run --manifest-path ../anyedge/Cargo.toml -p anyedge-cli --features cli -- serve --adapter fastly`.
4. Document new behaviour in `README.md` and update this guide if the best practices change.

Keeping these rules in mind prevents divergence between edge targets and safeguards the deterministic behaviour that Mocktioneer promises to clients.
