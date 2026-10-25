# HTTP/1 wire contract

This Node built-in harness implements migration design §8. The committed
`requests.json` contains 85 seeded cases and two unseeded cases with fixed caller
IDs, literal UTF-8 bodies and ordered request headers. Large requests use the
documented deterministic generators. Exact 1 MiB auction/APS and 8 MiB mediation
bodies, a 2 MiB mediation success, and 9 MiB 404/405 fallback requests exercise
the size boundaries. Padding is JSON whitespace, so response metadata stays small.
Captures belong outside the repository,
for example under a fresh `/tmp/mocktioneer-wire/` directory.

Use the Node version in `.tool-versions` and `curl`. No npm packages are needed. All traffic
stays at `http://127.0.0.1:8787`; redirects are captured without following them.

Prepare separate disposable baseline and candidate trees. For **each revision**,
prepare and start each scenario separately; the server snapshots config at
startup. Stop it before changing scenarios. Clear ambient `FASTLY_*`,
`MOCKTIONEER_*` and `EDGEZERO__*` overrides, then set exactly:

```sh
export MOCKTIONEER_TS_DOMAINS=ts.publisher.com
export MOCKTIONEER_PULL_TOKEN=wire-test-token
```

For the seeded startup, copy `mocktioneer.toml.example` to `mocktioneer.toml`,
run `cargo run -p mocktioneer-cli -- config push --adapter axum --yes`, then
`cargo run -p mocktioneer-adapter-axum`. The seeded CPM is `0.20`. For the
unseeded startup, ensure `.edgezero/local-config-mocktioneer_config.json` is
absent in that disposable tree before starting the server; do not push config.
Keep crate versions, host, port and environment identical across revisions.

```sh
node tests/wire-contract/wire_contract.mjs capture \
  --base-url http://127.0.0.1:8787 --scenario seeded --out /tmp/mocktioneer-wire/before/seeded
node tests/wire-contract/wire_contract.mjs capture \
  --base-url http://127.0.0.1:8787 --scenario unseeded --out /tmp/mocktioneer-wire/before/unseeded
```

Run those commands against the candidate with `after` destinations. Output
directories must not already exist. Each case retains `response.headers`
(raw status line and every header in order, including duplicates),
`response.body` (raw bytes), request metadata and body, and curl exit/stderr.
Capture uses `curl --http1.1 --raw -sS -D ... -o ...`; no output passes through
`jq`, JSON formatting or decompression. The corpus snapshot and its SHA-256 are
saved with every capture, so different scenarios or request lists cannot be
compared accidentally.

Before comparing responses, the comparator recomputes each saved corpus digest
and requires its complete, ordered scenario case list. Empty, missing, duplicate
or extra case IDs fail. Recorded methods, paths, headers and body bytes must
match the saved corpus and its deterministic generators, even when both
captures contain the same incorrect request. Historical captures are validated
against their saved corpus rather than the current checkout's request list.

```sh
node tests/wire-contract/wire_contract.mjs compare \
  --before /tmp/mocktioneer-wire/before/seeded --after /tmp/mocktioneer-wire/after/seeded
node tests/wire-contract/wire_contract.mjs compare \
  --before /tmp/mocktioneer-wire/before/unseeded --after /tmp/mocktioneer-wire/after/unseeded
```

First self-compare the baseline capture by passing its directory to both flags.
An independent second baseline capture is required for both startup scenarios:
repeat the capture commands into `before-repeat/seeded` and
`before-repeat/unseeded`, then require both comparisons below to pass. This
detects nondeterminism beyond generated IDs and `Date` before comparing revisions.

```sh
node tests/wire-contract/wire_contract.mjs compare \
  --before /tmp/mocktioneer-wire/before/seeded --after /tmp/mocktioneer-wire/before-repeat/seeded
node tests/wire-contract/wire_contract.mjs compare \
  --before /tmp/mocktioneer-wire/before/unseeded --after /tmp/mocktioneer-wire/before-repeat/unseeded
```

Comparator failures exit nonzero and report the case. Accepted differences print
their F/C number and changed fields. All F1–F14
and C1–C8 contracts are recorded in `accepted_differences`, with each one's exact
local scope; listing a number does not waive comparisons on unrelated responses.

Only `Date` values and UUIDv7 values at known emission sites are masked. Auction
and mediation bid IDs, APS `amzniid`/generated `crid`, and static creative pixel
IDs are checked for version 7 and RFC variant. Auction IDs supplied by callers
stay exact. Escaped copies of generated bid IDs in each `adm` metadata response
must refer to the corresponding bid. HTML pixel references must agree. Mask
counts, locations and repeated-reference identity must match. JSON string spans
are replaced directly in the original body; its whitespace, key order and
escaping are retained. mtkid, caller IDs, signed fields and all other content
remain byte-sensitive.

F2 permits only the exact missing-config message change and its length change;
F4 adds only the route's exact sorted `Allow`; C6/F10 adds only a missing
`Content-Length: 0` on an ordinary empty response. F10 also permits the exact
length-group removal/reinsertion permutation: the last application header group
moves into the former length slot, then length appends. C6 empty responses first
insert length before the application fields and undergo that same permutation.
Names, values and duplicate order stay exact; host-added `Date` remains last.
No general header sorting is permitted. F14 applies only to the three named
body-cap cases with an explicit F14 flag: `auction-over-1mib`, `aps-over-1mib`
and `mediation-over-8mib`. Auction/APS retain their 1 MiB cap and mediation
retains its 8 MiB cap. Overflow must return the exact 413 `payload_too_large`
payload with message `request body too large` and no `Retry-After`, including
in candidate self-comparisons. A 200 baseline is verified against its successful
small-body reference before accepting 200→413. An immediately preceding
baseline's exact 400 `bad_request` overflow payload may become that exact 413
payload. Only status, literal body and its length may change; CORS, cookies and
all other ordered headers stay exact. Exact 200, 400 and 413 repeats remain
comparable, while 200→400 fails with F14. Malformed JSON stays 400, validation
stays 422 and deadline expiry stays 408.
F8 requires the exact baseline streaming-body error and checks the new success
against the matching successful reference. F5 target errors require their exact
error payload and a baseline root reference. These transitions allow only their
listed status, body, length, content type and CORS changes. A different error at
an otherwise permitted status still fails. Boundary successes and large-body
fallbacks must retain their required 200/404/405 status and match their small-body
reference in each revision.

C8 covers exactly 15 named matched error cases and their fixed statuses:
nonstandard image and creative, missing pixel query, malformed and invalid
auction/APS/mediation JSON, missing sync query, both missing-config cases, and
the three body-cap cases. Only those three use 413 when F14 is present; their
exact old 400 baseline repeats also remain valid. Existing errors may gain
exactly one of each field:

```text
access-control-allow-origin: *
access-control-allow-methods: GET, POST, OPTIONS
access-control-allow-headers: *, content-type
```

The comparator appends these exact native header lines before applying F10's
length-group permutation; it does not remove CORS or sort other headers.
Missing, duplicate or changed fields, unrelated header permutations, and changes
to `Retry-After` fail. Body-cap transitions retain and compare the CORS fields
already present on the successful baseline. Routing 404/405 and normalized-head
errors have no C8 exception; a 404 from the matched creative handler does.
Baseline repeats without CORS and candidate repeats with the complete fields
remain comparable. An identical pair without CORS cannot establish that a
candidate implements C8; the admitted-dispatch tests assert that requirement.

The corpus digest changes when C8 or F14 is added. Capture the pre-PR baseline,
the immediately preceding PR revision, an independent repeat of each baseline,
and the candidate again, using this checkout's `wire_contract.mjs` for every
capture so each saves the same revised `requests.json`. Require both baseline
repeat comparisons and each baseline→candidate comparison to pass for both
startup scenarios. Keep prior captures intact; their saved corpus and digests
are historical evidence. Saved body-cap cases without F14 retain the exact
historical 200→400 contract, including C8's historical 400 allowlist. Saved cases
without C8 also retain their original, narrowly scoped CORS removal exception;
current F14 cap cases require the complete fields on both responses.

Record raw framing **separately** for each revision:

```sh
node tests/wire-contract/wire_contract.mjs characterize-raw \
  --base-url http://127.0.0.1:8787 --out /tmp/mocktioneer-wire/before/raw
```

This TCP client sends identical duplicate `Content-Length: 0` fields and CL plus
chunked TE with a complete terminating chunk. Raw request/response bytes and the
observed status or socket outcome are retained. The report makes no universal
400 assertion. Hyper may discard or normalize framing before edgezero sees it.
The >100-header case may likewise return Hyper's empty 431 before normalized
validation; a raw result does not establish admitted-dispatch validation.

```sh
node --test tests/wire-contract/wire_contract.test.mjs
```

The offline tests cover generated-ID/reference counts, UUID version/variant,
escaped `adm`, mtkid, caller IDs, signed fields, prices, body whitespace/order,
header values, duplicate-header order and narrow accepted differences. Runtime
deadline/config/cache injection, public signed success, external registration
and introspection are covered elsewhere or excluded by §8. The APS empty-slots
and mediation empty-impressions validators have pre-existing `min`/`value`
map-order nondeterminism. Their separate `BASELINE_APS_PARAMETER_ORDER` and
`BASELINE_MEDIATION_PARAMETER_ORDER` exceptions each accept exactly two complete
literal 422 payloads listed in the corpus and preserve every other byte. They
reject different minimums, values, paths, kinds, text, statuses and headers.
These are reported separately from migration changes. Auction validation uses
a parameterless missing-media error. There is no general normalization of
validation messages.
