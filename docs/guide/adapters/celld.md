# celld (local experiment)

Mocktioneer's celld adapter is a local-only experiment. It runs the real
Mocktioneer handlers as a Worker WASM module through `edgezero-adapter-celld`.
It does not run Axum and does not support cloud deployment, remote config
publication, or secret delivery.

All EdgeZero packages are pinned to commit
`332092e931821fc89961f85276af214e960cf606` from the draft celld adapter PR.
No sibling checkout or machine-specific Cargo patch is required.

The setup expects celld 0.6.2, `worker-build` 0.8.5, Rust's
`wasm32-unknown-unknown` target, and Worker SDK 0.8.5. The coordinator prepared
tools under `~/.cache/edgezero-celld-demo/bin`; prepend that directory:

```sh
export PATH="$HOME/.cache/edgezero-celld-demo/bin:$PATH"
```

`worker-build` also downloads wasm-bindgen, wasm-opt, and esbuild into
`~/.cache/worker-build`. Their provenance was not verified separately from the
build tool.

Build the generated Worker shim and stage it inside each celld project. celld
requires the configured main path to stay inside that manifest's project root.
The generator copies build output; it does not replace the generated shim.

```sh
cd crates/mocktioneer-adapter-celld
worker-build --release
cd ../..
cargo run -p mocktioneer-adapter-celld --bin prepare-celld-demo
```

Both manifests leave celld's default bundling enabled. The generated workers-rs
shim re-exports JavaScript in `build/index.js`; `no_bundle: true` prevents
celld from resolving that module. The staged demo wrapper imports the staged
original shim, so it also relies on default bundling. Staged runtime files and
the demo `.dev.vars` are ignored build/dev output.

`celld/normal/wrangler.json` points directly at the staged generated shim. Its
project directory has no `.dev.vars` and starts with an empty local KV
namespace. Use it to verify that a config-dependent auction fails through
Mocktioneer's regular typed-config path:

```sh
celld dev crates/mocktioneer-adapter-celld/celld/normal/wrangler.json \
  --host 127.0.0.1 --port 9876 --logs --no-watch
```

In another terminal, request the real OpenRTB route. With the fresh normal
manifest state, the request should fail because no typed config has been
published:

```sh
curl -i http://127.0.0.1:9876/openrtb2/auction \
  -H 'content-type: application/json' \
  -d '{"id":"demo-auction","imp":[{"id":"demo-imp","banner":{"w":300,"h":250}}]}'
```

Stop that process before starting the seeded demo. The two manifests live in
separate directories, so celld keeps their local KV state separate. The
preparation command generates the public CPM 0.37 fixture from Mocktioneer's
typed config loader and EdgeZero's `BlobEnvelope` constructor.

```sh
celld dev crates/mocktioneer-adapter-celld/celld/demo/wrangler.json \
  --host 127.0.0.1 --port 9876 --logs --no-watch
```

The generator writes `.dev.vars` only next to the demo manifest. The JS wrapper
requires `MOCKTIONEER_DEMO_SEED=1`, awaits a write to the actual
`mocktioneer_config` KV binding under key `mocktioneer_config`, then forwards
the request to the original worker-build shim. Concurrent first requests share
the pending write within each Worker isolate. Separate isolates or restarts may
write the same public fixture again. A failed write returns an error and does
not reach the app; a later request can retry. The wrapper adds no HTTP seeding
endpoint. The normal manifest does not load the wrapper or fixture variables.

The celld runtime selects the Worker default-stream reader for typed KV config.
celld's KV stream returned the correct text through its default reader, while a
BYOB reader reported end-of-stream with zero bytes on the same 99-byte value.
This is a host compatibility difference, not malformed fixture JSON; the
Cloudflare adapter retains its BYOB mode. The celld reader checks each host
chunk against the configured value limit before copying it into a Rust-owned
buffer and observes the read deadline. This bounds Rust-side value storage,
but not the host's allocation of an individual stream chunk; host chunk
allocation bounds are unsupported/unverified.

The same curl request should return a bid priced at 0.37, which proves the
real handler read the fixture rather than a runtime fallback. Price and
input-related fields are stable; generated bid IDs remain UUIDs. `GET
/` is config-free and should work with either manifest. Use
`GET /_mocktioneer/manifest` to inspect compiled app metadata, but note that
adapter allowlists are manifest metadata and are not currently runtime route
filters. The wasm event entrypoint itself selects the celld runner.

Run the wrapper tests with:

```sh
node --test crates/mocktioneer-adapter-celld/tests/demo-seeder.test.mjs
```

The sync routes rely on `std::env` guards that are unavailable in Worker WASM;
this demo does not establish their auth/domain protections. Signature-based
JWKS fetch and verification is also not proved by the unsigned auction demo.
Trusted Server identity/CAS contracts, secrets, remote config push, fleet
durability, and Cloudflare parity are not supported claims. `celld dev` binds to loopback here. Check port
9876 before launch and do not change `--host` to a public address.
