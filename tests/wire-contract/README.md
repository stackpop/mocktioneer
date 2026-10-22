# HTTP/1 wire contract

This Python standard-library harness implements migration design §8. The committed
`requests.json` contains 85 seeded cases and two unseeded cases with fixed caller
IDs, literal UTF-8 bodies and ordered request headers. Large requests use the
documented deterministic generators. Exact 1 MiB auction/APS and 8 MiB mediation
bodies, a 2 MiB mediation success, and 9 MiB 404/405 fallback requests exercise
the size boundaries. Padding is JSON whitespace, so response metadata stays small.
Captures belong outside the repository,
for example under a fresh `/tmp/mocktioneer-wire/` directory.

Use Python 3.9+ and `curl`. There are no Python packages to install. All traffic
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
python3 tests/wire-contract/wire_contract.py capture \
  --base-url http://127.0.0.1:8787 --scenario seeded --out /tmp/mocktioneer-wire/before/seeded
python3 tests/wire-contract/wire_contract.py capture \
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
python3 tests/wire-contract/wire_contract.py compare \
  --before /tmp/mocktioneer-wire/before/seeded --after /tmp/mocktioneer-wire/after/seeded
python3 tests/wire-contract/wire_contract.py compare \
  --before /tmp/mocktioneer-wire/before/unseeded --after /tmp/mocktioneer-wire/after/unseeded
```

First self-compare the baseline capture by passing its directory to both flags.
An independent second baseline capture is required for both startup scenarios:
repeat the capture commands into `before-repeat/seeded` and
`before-repeat/unseeded`, then require both comparisons below to pass. This
detects nondeterminism beyond generated IDs and `Date` before comparing revisions.

```sh
python3 tests/wire-contract/wire_contract.py compare \
  --before /tmp/mocktioneer-wire/before/seeded --after /tmp/mocktioneer-wire/before-repeat/seeded
python3 tests/wire-contract/wire_contract.py compare \
  --before /tmp/mocktioneer-wire/before/unseeded --after /tmp/mocktioneer-wire/before-repeat/unseeded
```

Comparator failures exit nonzero and report the case. Accepted differences print
their F/C number and changed fields. All F1–F13
and C1–C7 contracts are recorded in `accepted_differences`, with each one's exact
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
No general header sorting is permitted. Cap cases require the exact
400 payload and verify the baseline against its successful small-body reference.
F8 requires the exact baseline streaming-body error and checks the new success
against the matching successful reference. F5 target errors require their exact
error payload and a baseline root reference. These transitions allow only their
listed status, body, length, content type and CORS changes. A different error at
an otherwise permitted status still fails. Boundary successes and large-body
fallbacks must retain their required 200/404/405 status and match their small-body
reference in each revision.

Record raw framing **separately** for each revision:

```sh
python3 tests/wire-contract/wire_contract.py characterize-raw \
  --base-url http://127.0.0.1:8787 --out /tmp/mocktioneer-wire/before/raw
```

This TCP client sends identical duplicate `Content-Length: 0` fields and CL plus
chunked TE with a complete terminating chunk. Raw request/response bytes and the
observed status or socket outcome are retained. The report makes no universal
400 assertion. Hyper may discard or normalize framing before edgezero sees it.
The >100-header case may likewise return Hyper's empty 431 before normalized
validation; a raw result does not establish admitted-dispatch validation.

```sh
python3 -m unittest discover -s tests/wire-contract -p 'test_*.py'
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
