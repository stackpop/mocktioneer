// Offline regression tests for the byte-preserving wire comparator.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  symlinkSync,
  unlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { afterEach, beforeEach, test } from "node:test";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const SCRIPT = join(HERE, "wire_contract.mjs");
const UUID_A = "019a8a01000070008000000000000001";
const UUID_B = "019a8a02000070009000000000000002";
const UUID_C = "019a8a0300007000a000000000000003";
const MTKID = "13170e94273c9f84b05e8937f7d4b70b";
const CORS_HEADERS = [
  "access-control-allow-origin: *",
  "access-control-allow-methods: GET, POST, OPTIONS",
  "access-control-allow-headers: *, content-type",
].map((text) => Buffer.from(text));
const EXCESSIVE_DEPTH = 1001;
const temporaryDirectories = [];
let wire;

test("test_cli_help_preserves_global_and_command_usage", () => {
  for (const [args, expected] of [
    [["--help"], "capture"],
    [["capture", "--help"], "--scenario"],
    [["compare", "-h"], "--before"],
    [["characterize-raw", "--help"], "--base-url"],
  ]) {
    const result = spawnSync(process.execPath, [SCRIPT, ...args], {
      encoding: "utf8",
    });
    assert.equal(result.status, 0, result.stderr);
    assert.ok(result.stdout.includes(expected), result.stdout);
    assert.equal(result.stderr, "");
  }
});

test("test_capture_and_raw_refuse_existing_output_directories", () => {
  for (const command of ["capture", "characterize-raw"]) {
    const root = temporary();
    const marker = join(root, "untouched");
    writeFileSync(marker, "preserve archived bytes");
    const args = [
      command,
      "--base-url",
      "http://127.0.0.1:8787",
      "--out",
      root,
    ];
    if (command === "capture") args.push("--scenario", "seeded");
    const result = spawnSync(process.execPath, [SCRIPT, ...args], {
      encoding: "utf8",
    });
    assert.notEqual(result.status, 0);
    assert.equal(readFileSync(marker, "utf8"), "preserve archived bytes");
    assert.equal(existsSync(join(root, "requests.json")), false);
    assert.equal(
      existsSync(join(root, "identical-content-length.request")),
      false,
    );
  }
});

beforeEach(async () => {
  assert.ok(
    existsSync(SCRIPT),
    "wire capture/comparator has not been implemented",
  );
  wire = await import("./wire_contract.mjs");
});

afterEach(() => {
  for (const directory of temporaryDirectories.splice(0)) {
    rmSync(directory, { recursive: true, force: true });
  }
});

function temporary() {
  const directory = mkdtempSync(join(tmpdir(), "mocktioneer-wire-test-"));
  temporaryDirectories.push(directory);
  return directory;
}

function response(body = Buffer.from("body"), status = 200, headers = []) {
  if (!Buffer.isBuffer(body)) body = Buffer.from(JSON.stringify(body));
  const reason = {
    200: "OK",
    204: "No Content",
    400: "Bad Request",
    404: "Not Found",
    405: "Method Not Allowed",
    413: "Payload Too Large",
    414: "URI Too Long",
    422: "Unprocessable Entity",
    503: "Service Unavailable",
  }[status];
  const lines = [
    Buffer.from(`HTTP/1.1 ${status} ${reason}`),
    ...headers.map((line) => Buffer.from(line)),
  ];
  const head = Buffer.concat([
    ...lines.flatMap((line) => [line, Buffer.from("\r\n")]),
    Buffer.from("\r\n"),
  ]);
  return new wire.Response(head, body);
}

function auction(uuid = UUID_A, reference = null) {
  const preview = {
    response: {
      id: "wire-request",
      seatbid: [{ bid: [{ id: reference || uuid, price: 0.35 }] }],
    },
  };
  const adm =
    "<!-- MOCKTIONEER_METADATA\n" + JSON.stringify(preview, null, 2) + "\n-->";
  return {
    id: "wire-request",
    seatbid: [{ bid: [{ id: uuid, impid: "wire-imp", price: 0.35, adm }] }],
  };
}

function compare(caseDefinition, before, after) {
  return wire.compare_response(caseDefinition, before, after);
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function readJson(path) {
  return JSON.parse(readFileSync(path, "utf8"));
}

function writeJson(path, value) {
  writeFileSync(path, JSON.stringify(value));
}

function capture_pair(root = temporary(), scenario = "seeded") {
  const corpus = {
    cases: [
      {
        id: "one",
        scenario: "seeded",
        method: "GET",
        path: "/one",
        headers: [],
      },
      {
        id: "two",
        scenario: "seeded",
        method: "POST",
        path: "/two",
        headers: [],
        body: "{}",
        body_pad_to: 4,
        target_pad: 2,
        extra_headers: 1,
      },
      {
        id: "unseeded",
        scenario: "unseeded",
        method: "GET",
        path: "/unseeded",
        headers: [],
      },
    ],
  };
  const corpusBytes = Buffer.from(JSON.stringify(corpus));
  const requests = {
    one: [{ method: "GET", path: "/one", headers: [] }, Buffer.from("")],
    two: [
      { method: "POST", path: "/twoxx", headers: ["X-Wire-000: x"] },
      Buffer.from("{}  "),
    ],
    unseeded: [
      { method: "GET", path: "/unseeded", headers: [] },
      Buffer.from(""),
    ],
  };
  const ids = corpus.cases
    .filter((item) => item.scenario === scenario)
    .map((item) => item.id);
  const directories = ["before", "after"].map((name) => join(root, name));
  for (const directory of directories) {
    mkdirSync(directory);
    writeFileSync(join(directory, "requests.json"), corpusBytes);
    writeJson(join(directory, "capture.json"), {
      scenario,
      base_url: wire.BASE_URL,
      corpus_sha256: sha256(corpusBytes),
      cases: ids,
    });
    for (const caseId of ids) {
      const destination = join(directory, caseId);
      mkdirSync(destination);
      const [request, body] = requests[caseId];
      writeJson(join(destination, "request.json"), request);
      writeFileSync(join(destination, "request.body"), body);
      writeFileSync(join(destination, "curl.exit"), "0\n");
      writeFileSync(
        join(destination, "response.headers"),
        "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n",
      );
      writeFileSync(join(destination, "response.body"), "ok");
    }
  }
  return directories;
}

function compare_captures(directories) {
  const result = spawnSync(
    process.execPath,
    [SCRIPT, "compare", "--before", directories[0], "--after", directories[1]],
    { encoding: "utf8" },
  );
  assert.ifError(result.error);
  assert.equal(result.signal, null, result.stderr);
  return result;
}

function change_manifest(directories, changes) {
  for (const directory of directories) {
    const path = join(directory, "capture.json");
    writeJson(path, { ...readJson(path), ...changes });
  }
}

function body_cap_f14(name = "auction-over-1mib") {
  const number = name === "mediation-over-8mib" ? "F7" : "C3";
  const caseDefinition = {
    id: name,
    differences: [number, "C8", "F10", "F14"],
    transition: "body-cap",
    mask: "auction",
  };
  let body = Buffer.from(JSON.stringify(auction()));
  const cookies = ["set-cookie: a=1", "set-cookie: b=2"];
  const before = response(body, 200, [
    `content-length: ${body.length}`,
    "content-type: application/json",
    ...cookies,
    ...CORS_HEADERS,
  ]);
  const errors = [];
  for (const [status, kind] of [
    [400, "bad_request"],
    [413, "payload_too_large"],
  ]) {
    body = wire.error_body(status, kind, "request body too large");
    errors.push(
      response(body, status, [
        CORS_HEADERS[2],
        "content-type: application/json",
        ...cookies,
        ...CORS_HEADERS.slice(0, 2),
        `content-length: ${body.length}`,
      ]),
    );
  }
  return [caseDefinition, before, ...errors];
}

test("test_cli_compare_accepts_complete_scenarios", () => {
  for (const [scenario, count] of [
    ["seeded", 2],
    ["unseeded", 1],
  ]) {
    const result = compare_captures(capture_pair(temporary(), scenario));
    assert.equal(result.status, 0, result.stderr);
    assert.equal(result.stdout.split("PASS ").length - 1, count);
  }
});

test("test_cli_compare_runs_through_a_symlink_entrypoint", () => {
  const root = temporary();
  const directories = capture_pair(root);
  const alias = join(root, "wire-contract-alias.mjs");
  symlinkSync(SCRIPT, alias);
  const result = spawnSync(
    process.execPath,
    [alias, "compare", "--before", directories[0], "--after", directories[1]],
    { encoding: "utf8" },
  );
  assert.ifError(result.error);
  assert.equal(result.signal, null, result.stderr);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.stdout, "PASS one\nPASS two\n");
});

test("test_cli_compare_rejects_invalid_case_lists", () => {
  for (const ids of [
    [],
    ["one"],
    ["one", "one", "two"],
    ["two", "one"],
    ["one", "two", "extra"],
    ["unseeded"],
  ]) {
    const directories = capture_pair();
    change_manifest(directories, { cases: ids });
    const result = compare_captures(directories);
    assert.notEqual(result.status, 0);
    assert.ok(result.stderr.includes("case list"), result.stderr);
    assert.ok(!result.stdout.includes("PASS "), result.stdout);
  }
});

test("test_cli_compare_rejects_invalid_manifest_metadata", () => {
  for (const [changes, error] of [
    [{ corpus_sha256: "0".repeat(64) }, "digest"],
    [{ scenario: "unknown" }, "scenario"],
    [{ base_url: "http://127.0.0.1:9999" }, "base URL"],
  ]) {
    const directories = capture_pair();
    change_manifest(directories, changes);
    const result = compare_captures(directories);
    assert.notEqual(result.status, 0);
    assert.ok(result.stderr.includes(error), result.stderr);
  }
});

test("test_cli_compare_rejects_duplicate_corpus_ids", () => {
  const directories = capture_pair();
  for (const directory of directories) {
    const path = join(directory, "requests.json");
    const corpus = readJson(path);
    corpus.cases.push(corpus.cases[0]);
    writeJson(path, corpus);
  }
  change_manifest(directories, {
    corpus_sha256: sha256(readFileSync(join(directories[0], "requests.json"))),
  });
  const result = compare_captures(directories);
  assert.notEqual(result.status, 0);
  assert.ok(result.stderr.includes("duplicate"), result.stderr);
});

test("test_cli_compare_rejects_empty_scenario", () => {
  const directories = capture_pair();
  for (const directory of directories)
    writeFileSync(join(directory, "requests.json"), '{"cases": []}');
  change_manifest(directories, {
    cases: [],
    corpus_sha256: sha256(readFileSync(join(directories[0], "requests.json"))),
  });
  const result = compare_captures(directories);
  assert.notEqual(result.status, 0);
  assert.ok(result.stderr.includes("case list"), result.stderr);
});

test("test_cli_compare_checks_requests_against_corpus", () => {
  for (const [field, value] of [
    ["method", "GET"],
    ["path", "/two"],
    ["headers", []],
    ["body", Buffer.from("{}")],
  ]) {
    const directories = capture_pair();
    for (const directory of directories) {
      if (field === "body") {
        writeFileSync(join(directory, "two/request.body"), value);
      } else {
        const path = join(directory, "two/request.json");
        writeJson(path, { ...readJson(path), [field]: value });
      }
    }
    const result = compare_captures(directories);
    assert.notEqual(result.status, 0);
    assert.ok(result.stderr.includes("request"), result.stderr);
    assert.ok(result.stderr.includes("corpus"), result.stderr);
  }
});

test("test_cli_compare_rejects_missing_response_files", () => {
  const directories = capture_pair();
  unlinkSync(join(directories[1], "two/response.body"));
  const result = compare_captures(directories);
  assert.notEqual(result.status, 0);
  assert.ok(result.stderr.includes("FAIL two"), result.stderr);
});

test("test_string_sites_rejects_malformed_json_without_runtime_exceptions", () => {
  for (const text of [
    "",
    " ",
    "{",
    "[",
    '{"a":',
    '{"a":1',
    '{"a":1,}',
    "[1,]",
    "[".repeat(EXCESSIVE_DEPTH) + "0" + "]".repeat(EXCESSIVE_DEPTH),
  ]) {
    assert.throws(() => wire.string_sites(text), wire.ContractError);
  }
});

test("test_masked_auction_rejects_excessive_metadata_nesting_as_contract_failure", () => {
  const payload = auction();
  payload.seatbid[0].bid[0].adm =
    "<!-- MOCKTIONEER_METADATA\n" +
    "[".repeat(EXCESSIVE_DEPTH) +
    "0" +
    "]".repeat(EXCESSIVE_DEPTH);
  assert.throws(
    () => wire.mask_response({ mask: "auction" }, response(payload)),
    wire.ContractError,
  );
});

test("test_cli_compare_reports_malformed_json_and_continues_to_later_cases", () => {
  for (const malformed of [
    Buffer.from(""),
    Buffer.from('{"a":1'),
    Buffer.from('{"a":1,}'),
    Buffer.from(
      "[".repeat(EXCESSIVE_DEPTH) + "0" + "]".repeat(EXCESSIVE_DEPTH),
    ),
  ]) {
    const directories = capture_pair();
    for (const directory of directories) {
      const path = join(directory, "requests.json");
      const corpus = readJson(path);
      corpus.cases[0].mask = "auction";
      writeJson(path, corpus);
    }
    change_manifest(directories, {
      corpus_sha256: sha256(
        readFileSync(join(directories[0], "requests.json")),
      ),
    });
    for (const [index, body] of [Buffer.from('{"a":1}'), malformed].entries()) {
      writeFileSync(join(directories[index], "one/response.body"), body);
      writeFileSync(
        join(directories[index], "one/response.headers"),
        `HTTP/1.1 200 OK\r\nContent-Length: ${body.length}\r\n\r\n`,
      );
    }
    const result = compare_captures(directories);
    assert.notEqual(result.status, 0);
    assert.ok(result.stderr.includes("FAIL one"), result.stderr);
    assert.ok(result.stdout.includes("PASS two"), result.stdout);
    assert.ok(result.stderr.includes("1 wire case(s) failed"), result.stderr);
    assert.ok(!result.stdout.includes("PASS one"), result.stdout);
    assert.doesNotMatch(result.stderr, /Traceback|\n\s+at /);
  }
});

test("test_identical_bytes_pass_and_dates_are_masked", () => {
  const a = response(undefined, 200, ["Date: Tue, 20 Oct 2026 01:00:00 GMT"]);
  const b = response(undefined, 200, ["Date: Tue, 20 Oct 2026 02:00:00 GMT"]);
  assert.deepEqual(compare({}, a, b), []);
});

test("test_header_values_order_and_duplicate_order_are_protected", () => {
  const a = response(undefined, 200, [
    "Set-Cookie: a=1",
    "Set-Cookie: b=2",
    "X-Test: ok",
  ]);
  const variants = [
    ["Set-Cookie: a=1", "Set-Cookie: b=2", "X-Test: changed"],
    ["Set-Cookie: b=2", "Set-Cookie: a=1", "X-Test: ok"],
    ["X-Test: ok", "Set-Cookie: a=1", "Set-Cookie: b=2"],
  ];
  for (const headers of variants) {
    assert.throws(
      () => compare({}, a, response(undefined, 200, headers)),
      wire.ContractError,
    );
  }
});

test("test_generated_bid_id_and_escaped_preview_use_one_token", () => {
  assert.deepEqual(
    compare(
      { mask: "auction" },
      response(auction()),
      response(auction(UUID_B)),
    ),
    [],
  );
});

test("test_f10_allows_only_exact_content_length_group_swap", () => {
  const old = [
    "Content-Length: 4",
    "Content-Type: text/plain",
    "Set-Cookie: a=1",
    "Set-Cookie: b=2",
  ];
  const current = [
    "Set-Cookie: a=1",
    "Set-Cookie: b=2",
    "Content-Type: text/plain",
    "Content-Length: 4",
  ];
  const caseDefinition = { differences: ["F10"] };
  assert.deepEqual(
    compare(
      caseDefinition,
      response(undefined, 200, old),
      response(undefined, 200, current),
    ),
    ["F10: exact Content-Length group permutation"],
  );
  for (const changed of [
    [...current].reverse(),
    [current[1], current[0], ...current.slice(2)],
    ["Set-Cookie: a=changed", ...current.slice(1)],
    [...current, "X-Extra: yes"],
  ]) {
    assert.throws(
      () =>
        compare(
          caseDefinition,
          response(undefined, 200, old),
          response(undefined, 200, changed),
        ),
      wire.ContractError,
    );
  }
  assert.throws(
    () =>
      compare(
        {},
        response(undefined, 200, old),
        response(undefined, 200, current),
      ),
    wire.ContractError,
  );
});

test("test_c6_empty_length_insertion_has_one_exact_f10_permutation", () => {
  const old = [
    "Location: https://example.test/",
    "Set-Cookie: a=1",
    "Set-Cookie: b=2",
    "Content-Length: 0",
  ];
  const current = [
    "Set-Cookie: a=1",
    "Set-Cookie: b=2",
    "Location: https://example.test/",
    "Content-Length: 0",
  ];
  const caseDefinition = { differences: ["C6", "F10"] };
  assert.deepEqual(
    compare(
      caseDefinition,
      response(Buffer.from(""), 200, old),
      response(Buffer.from(""), 200, current),
    ),
    ["C6/F10: exact empty-response header permutation"],
  );
  for (const changed of [
    [current[1], current[0], ...current.slice(2)],
    [...current.slice(0, -1), "Content-Length: 1"],
    ["Set-Cookie: changed", ...current.slice(1)],
  ]) {
    assert.throws(
      () =>
        compare(
          caseDefinition,
          response(Buffer.from(""), 200, old),
          response(Buffer.from(""), 200, changed),
        ),
      wire.ContractError,
    );
  }
  assert.throws(
    () =>
      compare(
        { differences: ["F10"] },
        response(Buffer.from(""), 200, old),
        response(Buffer.from(""), 200, current),
      ),
    wire.ContractError,
  );
  assert.throws(
    () =>
      compare(
        { differences: ["F10"] },
        response(Buffer.from(""), 200, ["X-Test: same"]),
        response(Buffer.from(""), 200, ["X-Test: same", "Content-Length: 0"]),
      ),
    wire.ContractError,
  );
});

test("test_body_price_caller_ids_whitespace_and_order_are_protected", () => {
  const base = auction();
  const price = structuredClone(base);
  price.seatbid[0].bid[0].price = 0.36;
  const callerId = structuredClone(base);
  callerId.id = UUID_B;
  const variants = [
    price,
    callerId,
    Buffer.from(JSON.stringify(base, null, 2)),
    Buffer.from(
      JSON.stringify(Object.fromEntries(Object.entries(base).reverse())),
    ),
  ];
  for (const body of variants) {
    assert.throws(
      () => compare({ mask: "auction" }, response(base), response(body)),
      wire.ContractError,
    );
  }
});

test("test_mtkid_is_never_masked", () => {
  const a = response(undefined, 200, [`Set-Cookie: mtkid=${MTKID}`]);
  const b = response(undefined, 200, [`Set-Cookie: mtkid=${UUID_B}`]);
  assert.throws(() => compare({}, a, b), wire.ContractError);
});

test("test_caller_ids_and_signed_fields_inside_adm_are_protected", () => {
  const base = auction();
  base.seatbid[0].bid[0].adm +=
    '\n{"request":{"id":"' + UUID_A + '","signature":"fixed"}}';
  const changed = structuredClone(base);
  changed.seatbid[0].bid[0].adm = changed.seatbid[0].bid[0].adm.replace(
    '"signature":"fixed"',
    '"signature":"other"',
  );
  assert.throws(
    () => compare({ mask: "auction" }, response(base), response(changed)),
    wire.ContractError,
  );
});

test("test_uuid_reference_mismatch_fails", () => {
  assert.throws(
    () =>
      compare(
        { mask: "auction" },
        response(auction()),
        response(auction(UUID_B, UUID_C)),
      ),
    wire.ContractError,
  );
});

test("test_uuid_occurrence_count_mismatch_fails", () => {
  const changed = auction(UUID_B);
  changed.seatbid[0].bid[0].adm = "<!-- MOCKTIONEER_METADATA\n{}\n-->";
  assert.throws(
    () => compare({ mask: "auction" }, response(auction()), response(changed)),
    wire.ContractError,
  );
});

test("test_uuid_version_and_variant_are_validated_even_in_self_compare", () => {
  for (const bad of [
    UUID_A.slice(0, 12) + "4" + UUID_A.slice(13),
    UUID_A.slice(0, 16) + "0" + UUID_A.slice(17),
  ]) {
    const body = response(auction(bad));
    assert.throws(
      () => compare({ mask: "auction" }, body, body),
      wire.ContractError,
    );
  }
});

test("test_static_pixel_repeated_reference_equality", () => {
  const creative = (a, b) =>
    Buffer.from(
      `<img src="//127.0.0.1:8787/pixel?pid=${a}" data-static-pid="${b}">` +
        `<script>var jsPid = "${UUID_C}";</script>`,
    );
  assert.deepEqual(
    compare(
      { mask: "creative" },
      response(creative(UUID_A, UUID_A)),
      response(creative(UUID_B, UUID_B)),
    ),
    [],
  );
  assert.throws(
    () =>
      compare(
        { mask: "creative" },
        response(creative(UUID_A, UUID_A)),
        response(creative(UUID_B, UUID_C)),
      ),
    wire.ContractError,
  );
});

test("test_aps_generated_ids_are_masked_but_encoded_price_is_protected", () => {
  const aps = (uuid, price = "MC4zNQ==") => ({
    contextual: {
      slots: [
        {
          slotID: "wire-slot",
          amzniid: uuid,
          crid: uuid + "-mocktioneer",
          amznp: price,
        },
      ],
    },
  });
  assert.deepEqual(
    compare({ mask: "aps" }, response(aps(UUID_A)), response(aps(UUID_B))),
    [],
  );
  assert.throws(
    () =>
      compare(
        { mask: "aps" },
        response(aps(UUID_A)),
        response(aps(UUID_B, "MQ==")),
      ),
    wire.ContractError,
  );
});

test("test_allowed_405_adds_only_exact_sorted_allow_header", () => {
  const body = {
    error: { kind: "method_not_allowed", status: 405, message: "same" },
  };
  const caseDefinition = { differences: ["F4"], allow: "GET, OPTIONS" };
  const a = response(body, 405);
  assert.deepEqual(
    compare(caseDefinition, a, response(body, 405, ["Allow: GET, OPTIONS"])),
    ["F4: Allow"],
  );
  assert.throws(
    () =>
      compare(caseDefinition, a, response(body, 405, ["Allow: OPTIONS, GET"])),
    wire.ContractError,
  );
  assert.throws(
    () =>
      compare(
        caseDefinition,
        a,
        response(Buffer.from("unlisted error"), 405, ["Allow: GET, OPTIONS"]),
      ),
    wire.ContractError,
  );
});

test("test_allowed_cap_status_does_not_allow_arbitrary_error_body", () => {
  const caseDefinition = { differences: ["C3"], transition: "body-cap" };
  const before = response(auction());
  const error = {
    error: {
      kind: "bad_request",
      message: "request body too large",
      status: 400,
    },
  };
  assert.ok(
    compare(caseDefinition, before, response(error, 400)).includes(
      "C3: status/body/Content-Length",
    ),
  );
  error.error.message = "unlisted failure";
  assert.throws(
    () => compare(caseDefinition, before, response(error, 400)),
    wire.ContractError,
  );
});

test("test_c8_adds_exact_cors_before_the_f10_length_permutation", () => {
  const body = wire.error_body(400, "bad_request", "invalid JSON");
  const length = `content-length: ${body.length}`;
  const caseDefinition = {
    id: "auction-malformed",
    differences: ["C8", "F10"],
  };
  const date = "date: Wed, 21 Oct 2026 18:21:58 GMT";
  const before = response(body, 400, [
    length,
    "content-type: application/json",
    date,
  ]);
  const afterHeaders = [
    CORS_HEADERS[2],
    "content-type: application/json",
    ...CORS_HEADERS.slice(0, 2),
    length,
    date,
  ];
  const after = response(body, 400, afterHeaders);
  assert.deepEqual(compare(caseDefinition, before, before), []);
  assert.deepEqual(compare(caseDefinition, after, after), []);
  assert.deepEqual(compare(caseDefinition, before, after), [
    "C8: matched-error CORS",
    "F10: exact Content-Length group permutation",
  ]);
  for (const headers of [
    afterHeaders.slice(1),
    [...afterHeaders, CORS_HEADERS[0]],
    afterHeaders.map((line) =>
      line.toString().replace("GET, POST, OPTIONS", "GET, POST"),
    ),
    [afterHeaders[1], afterHeaders[0], ...afterHeaders.slice(2)],
    [afterHeaders[0], ...afterHeaders.slice(2, 4), afterHeaders[1], length],
    [...afterHeaders, "x-unrelated: added"],
  ]) {
    assert.throws(
      () => compare(caseDefinition, before, response(body, 400, headers)),
      wire.ContractError,
    );
  }
  assert.throws(
    () => compare(caseDefinition, after, before),
    wire.ContractError,
  );
});

test("test_c8_preserves_config_retry_and_all_other_header_order", () => {
  const old = wire.error_body(
    503,
    "config_out_of_date",
    wire.MISSING + " — " + wire.PUSH_HINT,
  );
  const current = wire.error_body(
    503,
    "config_out_of_date",
    wire.MISSING + "; " + wire.PUSH_HINT,
  );
  const caseDefinition = {
    id: "unseeded-auction-missing-config",
    differences: ["F2", "C8", "F10"],
  };
  const before = response(old, 503, [
    `content-length: ${old.length}`,
    "content-type: application/json",
    "retry-after: 60",
  ]);
  const headers = [
    CORS_HEADERS[2],
    "content-type: application/json",
    "retry-after: 60",
    ...CORS_HEADERS.slice(0, 2),
    `content-length: ${current.length}`,
  ];
  assert.ok(
    compare(caseDefinition, before, response(current, 503, headers)).includes(
      "C8: matched-error CORS",
    ),
  );
  for (const changed of [
    [headers[0], headers[2], headers[1], ...headers.slice(3)],
    headers.filter((line) => !line.toString().startsWith("retry-after:")),
    headers.map((line) =>
      line.toString().replace("retry-after: 60", "retry-after: 10"),
    ),
  ]) {
    assert.throws(
      () => compare(caseDefinition, before, response(current, 503, changed)),
      wire.ContractError,
    );
  }
});

test("test_c8_is_limited_to_named_matched_error_statuses", () => {
  const before = response(Buffer.from("same"), 400);
  const after = response(Buffer.from("same"), 400, CORS_HEADERS);
  for (const caseDefinition of [
    { id: "head-target-9kb", differences: ["C8"] },
    { id: "unknown-path", differences: ["C8"] },
    { id: "method-not-allowed-root", differences: ["C8"] },
    { id: "auction-validation", differences: ["C8"] },
    { id: "auction-malformed", differences: [] },
  ]) {
    assert.throws(
      () => compare(caseDefinition, before, after),
      wire.ContractError,
    );
  }
});

test("test_c8_body_cap_transition_preserves_cors_and_the_success_reference", () => {
  const caseDefinition = {
    id: "auction-over-1mib",
    differences: ["C3", "C8", "F10"],
    transition: "body-cap",
    mask: "auction",
  };
  const beforeBody = auction();
  const beforeLength = Buffer.byteLength(JSON.stringify(beforeBody));
  const before = response(beforeBody, 200, [
    `content-length: ${beforeLength}`,
    "content-type: application/json",
    ...CORS_HEADERS,
  ]);
  const body = wire.error_body(400, "bad_request", "request body too large");
  const headers = [
    CORS_HEADERS[2],
    "content-type: application/json",
    ...CORS_HEADERS.slice(0, 2),
    `content-length: ${body.length}`,
  ];
  assert.deepEqual(compare(caseDefinition, before, before), []);
  assert.ok(
    wire
      .compare_response(
        caseDefinition,
        before,
        response(body, 400, headers),
        before,
      )
      .includes("C3: status/body/Content-Length"),
  );
  for (const changed of [
    headers.slice(1),
    [...headers, CORS_HEADERS[1]],
    headers.filter((line) => !line.toString().startsWith("access-control-")),
  ]) {
    assert.throws(
      () =>
        wire.compare_response(
          caseDefinition,
          before,
          response(body, 400, changed),
          before,
        ),
      wire.ContractError,
    );
  }
  assert.throws(
    () =>
      wire.compare_response(
        caseDefinition,
        before,
        response(body, 400, headers),
        response(auction(UUID_B), 200),
      ),
    wire.ContractError,
  );
});

test("test_historical_body_cap_metadata_retains_its_cors_removal_contract", () => {
  const caseDefinition = {
    id: "auction-over-1mib",
    differences: ["C3", "F10"],
    transition: "body-cap",
    mask: "auction",
  };
  const success = auction();
  const length = Buffer.byteLength(JSON.stringify(success));
  const date = "date: Wed, 21 Oct 2026 18:21:58 GMT";
  const before = response(success, 200, [
    `content-length: ${length}`,
    "content-type: application/json",
    ...CORS_HEADERS,
    date,
  ]);
  const body = wire.error_body(400, "bad_request", "request body too large");
  const after = response(body, 400, [
    "content-type: application/json",
    "content-length: 80",
    date,
  ]);
  assert.deepEqual(
    wire.compare_response(caseDefinition, before, after, before),
    [
      "C3: status/body/Content-Length",
      "F10: exact Content-Length group permutation",
    ],
  );
  const currentCase = { ...caseDefinition, differences: ["C3", "C8", "F10"] };
  assert.throws(
    () => wire.compare_response(currentCase, before, after, before),
    wire.ContractError,
  );
});

test("test_f14_body_caps_accept_success_and_previous_error_baselines", () => {
  for (const name of [
    "auction-over-1mib",
    "aps-over-1mib",
    "mediation-over-8mib",
  ]) {
    const [caseDefinition, success, oldError, newError] = body_cap_f14(name);
    for (const before of [success, oldError]) {
      const changes = wire.compare_response(
        caseDefinition,
        before,
        newError,
        success,
      );
      assert.ok(
        changes.some((change) => change.includes("F14")),
        JSON.stringify(changes),
      );
    }
  }
});

test("test_f14_body_caps_accept_exact_baseline_and_candidate_repeats", () => {
  for (const name of [
    "auction-over-1mib",
    "aps-over-1mib",
    "mediation-over-8mib",
  ]) {
    const [caseDefinition, ...responses] = body_cap_f14(name);
    for (const item of responses)
      assert.deepEqual(compare(caseDefinition, item, item), []);
  }
});

test("test_f14_body_cap_rejects_old_status_after_success_and_reverse_transitions", () => {
  const [caseDefinition, success, oldError, newError] = body_cap_f14();
  for (const [before, after] of [
    [success, oldError],
    [oldError, success],
    [newError, success],
    [newError, oldError],
  ]) {
    assert.throws(
      () => wire.compare_response(caseDefinition, before, after, success),
      wire.ContractError,
    );
  }
});

test("test_f14_body_cap_requires_the_success_reference", () => {
  const [caseDefinition, success, , newError] = body_cap_f14();
  assert.throws(
    () => compare(caseDefinition, success, newError),
    wire.ContractError,
  );
  const changed = response(
    Buffer.from(success.body.toString().replaceAll("0.35", "0.36")),
    200,
    success.lines.slice(1),
  );
  assert.throws(
    () => wire.compare_response(caseDefinition, changed, newError, success),
    wire.ContractError,
  );
});

test("test_f14_body_cap_requires_its_flag_named_case_and_transition", () => {
  const [caseDefinition, success, oldError, newError] = body_cap_f14();
  const { id: _id, ...withoutId } = caseDefinition;
  const variants = [
    { ...caseDefinition, differences: ["C3", "C8", "F10"] },
    { ...caseDefinition, id: "auction-malformed" },
    { ...caseDefinition, id: "unlisted-cap" },
    { ...caseDefinition, id: "unlisted-cap", differences: ["F14"] },
    { ...caseDefinition, transition: "unlisted-transition" },
    withoutId,
  ];
  for (const variant of variants) {
    for (const before of [success, oldError, newError]) {
      assert.throws(
        () => wire.compare_response(variant, before, newError, success),
        wire.ContractError,
      );
    }
  }
});

test("test_f14_body_cap_validates_both_exact_error_payloads", () => {
  const [caseDefinition, success, oldError, newError] = body_cap_f14();
  for (const error of [oldError, newError]) {
    const payload = JSON.parse(error.body);
    const mutations = [
      Buffer.concat([error.body, Buffer.from("\n")]),
      Buffer.from(JSON.stringify(payload, null, 2)),
      Buffer.from(
        JSON.stringify({
          error: Object.fromEntries(Object.entries(payload.error).reverse()),
        }),
      ),
    ];
    for (const [field, value] of [
      ["status", error.status === 413 ? 400 : 413],
      ["kind", error.status === 413 ? "bad_request" : "payload_too_large"],
      ["message", "request body too large changed"],
    ]) {
      const changed = structuredClone(payload);
      changed.error[field] = value;
      mutations.push(Buffer.from(JSON.stringify(changed)));
    }
    for (const body of mutations) {
      const headers = error.lines
        .slice(1)
        .map((line) =>
          line.toString().startsWith("content-length:")
            ? `content-length: ${body.length}`
            : line,
        );
      const changed = response(body, error.status, headers);
      const [before, after] =
        error.status === 400 ? [changed, newError] : [oldError, changed];
      assert.throws(
        () => wire.compare_response(caseDefinition, before, after, success),
        wire.ContractError,
      );
      assert.throws(
        () => compare(caseDefinition, changed, changed),
        wire.ContractError,
      );
    }
  }
});

test("test_f14_body_cap_forbids_retry_after_even_in_candidate_repeats", () => {
  const [caseDefinition, success, oldError, newError] = body_cap_f14();
  for (const field of ["retry-after: 60", "Retry-After: 0"]) {
    const changed = response(newError.body, 413, [
      ...newError.lines.slice(1),
      field,
    ]);
    for (const before of [success, oldError, changed]) {
      assert.throws(
        () => wire.compare_response(caseDefinition, before, changed, success),
        wire.ContractError,
      );
    }
  }
});

test("test_f14_body_cap_preserves_cors_cookies_and_other_ordered_headers", () => {
  const [caseDefinition, success, oldError, newError] = body_cap_f14();
  const headers = newError.lines.slice(1);
  const variants = [
    headers.slice(1),
    [...headers, CORS_HEADERS[0]],
    headers.map((line) =>
      line.toString().replace("GET, POST, OPTIONS", "GET, POST"),
    ),
    headers.map((line) =>
      line.toString().replace("set-cookie: a=1", "set-cookie: a=changed"),
    ),
    [headers[0], headers[1], headers[3], headers[2], ...headers.slice(4)],
    [headers[1], headers[0], ...headers.slice(2)],
    [...headers, "x-unrelated: added"],
    [...headers.slice(0, -1), `content-length: ${newError.body.length - 1}`],
  ];
  for (const changed of variants) {
    for (const before of [success, oldError]) {
      assert.throws(
        () =>
          wire.compare_response(
            caseDefinition,
            before,
            response(newError.body, 413, changed),
            success,
          ),
        wire.ContractError,
      );
    }
  }
  for (const item of [success, oldError, newError]) {
    const noCors = response(
      item.body,
      item.status,
      item.lines
        .slice(1)
        .filter((line) => !line.toString().startsWith("access-control-")),
    );
    assert.throws(
      () => compare(caseDefinition, noCors, noCors),
      wire.ContractError,
    );
  }
});

test("test_f14_does_not_relax_other_c8_error_statuses", () => {
  const [, , , newError] = body_cap_f14();
  for (const name of [
    "auction-malformed",
    "aps-malformed",
    "mediation-malformed",
    "auction-validation",
    "aps-validation",
    "mediation-validation",
  ]) {
    const caseDefinition = { id: name, differences: ["C8", "F10"] };
    assert.throws(
      () => compare(caseDefinition, newError, newError),
      wire.ContractError,
    );
  }
});

test("test_corpus_is_explicit_and_has_separate_startup_scenarios", () => {
  const corpus = readJson(join(HERE, "requests.json"));
  const cases = corpus.cases;
  assert.equal(new Set(cases.map((item) => item.id)).size, cases.length);
  assert.deepEqual(
    new Set(cases.map((item) => item.scenario)),
    new Set(["seeded", "unseeded"]),
  );
  assert.deepEqual(
    new Set(Object.keys(corpus.accepted_differences)),
    new Set([
      ...Array.from({ length: 14 }, (_, index) => `F${index + 1}`),
      ...Array.from({ length: 8 }, (_, index) => `C${index + 1}`),
    ]),
  );
  assert.equal(cases.length, 87);
  assert.equal(cases.filter((item) => item.scenario === "seeded").length, 85);
  assert.deepEqual(
    new Set(
      cases
        .filter((item) => item.differences.includes("F14"))
        .map((item) => item.id),
    ),
    new Set(["auction-over-1mib", "aps-over-1mib", "mediation-over-8mib"]),
  );
  assert.deepEqual(
    new Set(
      cases
        .filter((item) => item.differences.includes("C8"))
        .map((item) => item.id),
    ),
    new Set([
      "image-nonstandard",
      "creative-nonstandard",
      "pixel-missing",
      "aps-validation",
      "aps-malformed",
      "auction-malformed",
      "auction-validation",
      "sync-start-missing",
      "mediation-malformed",
      "mediation-validation",
      "auction-over-1mib",
      "aps-over-1mib",
      "mediation-over-8mib",
      "unseeded-auction-missing-config",
      "unseeded-aps-missing-config",
    ]),
  );
  assert.ok(
    cases.every(
      (item) => "method" in item && "path" in item && "headers" in item,
    ),
  );
  for (const scenario of ["seeded", "unseeded"])
    assert.ok(cases.some((item) => item.scenario === scenario));
});

test("test_missing_config_permits_only_exact_message_change", () => {
  const old = wire.error_body(
    503,
    "config_out_of_date",
    wire.MISSING + " — " + wire.PUSH_HINT,
  );
  const current = wire.error_body(
    503,
    "config_out_of_date",
    wire.MISSING + "; " + wire.PUSH_HINT,
  );
  const headers = ["Retry-After: 60"];
  assert.ok(
    compare(
      { differences: ["F2"] },
      response(old, 503, headers),
      response(current, 503, headers),
    ).length,
  );
  assert.throws(
    () =>
      compare(
        { differences: ["F2"] },
        response(old, 503, headers),
        response(
          Buffer.from(
            current
              .toString()
              .replace("config_out_of_date", "service_unavailable"),
          ),
          503,
          headers,
        ),
      ),
    wire.ContractError,
  );
});

test("test_no_content_type_success_must_match_successful_reference", () => {
  const caseDefinition = {
    mask: "auction",
    transition: "no-content-type",
    differences: ["F8"],
  };
  const before = response(
    wire.error_body(400, "bad_request", wire.STREAM_ERROR),
    400,
  );
  const reference = response(auction(UUID_A));
  const after = response(auction(UUID_B));
  assert.ok(
    wire.compare_response(caseDefinition, before, after, null, reference)
      .length,
  );
  assert.ok(
    wire.compare_response(caseDefinition, before, after, reference, reference)
      .length,
  );
  assert.throws(
    () => compare(caseDefinition, before, after),
    wire.ContractError,
  );
  const changed = auction(UUID_B);
  changed.seatbid[0].bid[0].price = 99;
  assert.throws(
    () =>
      wire.compare_response(
        caseDefinition,
        before,
        response(changed),
        null,
        reference,
      ),
    wire.ContractError,
  );
});

test("test_head_target_permits_only_exact_normalized_error", () => {
  const caseDefinition = {
    transition: "head-target",
    differences: ["F5", "F9"],
  };
  const before = response(Buffer.from("root"));
  const after = response(
    wire.error_body(
      414,
      "uri_too_long",
      "normalized request target exceeds configured limit",
    ),
    414,
  );
  assert.ok(
    wire.compare_response(caseDefinition, before, after, before).length,
  );
  assert.throws(
    () => compare(caseDefinition, before, after),
    wire.ContractError,
  );
  assert.throws(
    () =>
      wire.compare_response(
        caseDefinition,
        before,
        response(Buffer.from("wrong"), 414),
        before,
      ),
    wire.ContractError,
  );
});

test("test_deterministic_generators_preserve_explicit_valid_json", () => {
  const caseDefinition = {
    path: "/?wire=",
    headers: ["Host: 127.0.0.1:8787"],
    body: '{"id":"fixed"}',
    body_pad_to: 1153434,
    target_pad: 9216,
    extra_headers: 101,
  };
  const [path, headers, body] = wire.request_bytes(caseDefinition);
  assert.equal(body.length, 1153434);
  assert.deepEqual(JSON.parse(body), { id: "fixed" });
  assert.equal(path.length, caseDefinition.path.length + 9216);
  assert.equal(headers.at(-1), "X-Wire-100: x");
});

test("test_baseline_aps_parameter_order_has_a_finite_exact_allowlist", () => {
  const caseDefinition = {
    id: "aps-validation",
    baseline_exception: "BASELINE_APS_PARAMETER_ORDER",
  };
  const messages = [
    'slots: Validation error: length [{"value": Array [], "min": Number(1)}]',
    'slots: Validation error: length [{"min": Number(1), "value": Array []}]',
  ];
  const before = response(wire.error_body(422, "validation", messages[0]), 422);
  const after = response(wire.error_body(422, "validation", messages[1]), 422);
  assert.ok(
    compare(caseDefinition, before, after).includes(
      "BASELINE_APS_PARAMETER_ORDER: error.message",
    ),
  );
  for (const altered of [
    messages[1].replace("Number(1)", "Number(2)"),
    messages[1].replace("Array []", "Array [1]"),
    messages[1].replace("slots:", "other:"),
    messages[1] + " changed",
  ]) {
    assert.throws(
      () =>
        compare(
          caseDefinition,
          before,
          response(wire.error_body(422, "validation", altered), 422),
        ),
      wire.ContractError,
    );
  }
  assert.throws(
    () =>
      compare(
        caseDefinition,
        before,
        response(wire.error_body(422, "bad_request", messages[1]), 422),
      ),
    wire.ContractError,
  );
});

test("test_empty_response_exception_does_not_add_forbidden_204_length", () => {
  const before = response(Buffer.from(""), 204);
  const after = response(Buffer.from(""), 204, ["Content-Length: 0"]);
  assert.throws(
    () => compare({ differences: ["C6", "F10"] }, before, after),
    wire.ContractError,
  );
});

test("test_boundary_and_fallback_cases_require_expected_status_and_reference", () => {
  for (const [transition, status] of [
    ["body-boundary", 200],
    ["fallback", 404],
    ["fallback", 405],
  ]) {
    const caseDefinition = { transition, required_status: status };
    const reference = response(Buffer.from("fixed response"), status);
    assert.deepEqual(
      wire.compare_response(
        caseDefinition,
        reference,
        reference,
        reference,
        reference,
      ),
      [],
    );
    const wrong = response(Buffer.from("fixed response"), 400);
    assert.throws(
      () =>
        wire.compare_response(
          caseDefinition,
          wrong,
          wrong,
          reference,
          reference,
        ),
      wire.ContractError,
    );
    const changed = response(Buffer.from("mutated response"), status);
    assert.throws(
      () =>
        wire.compare_response(
          caseDefinition,
          reference,
          changed,
          reference,
          reference,
        ),
      wire.ContractError,
    );
    assert.throws(
      () =>
        wire.compare_response(
          caseDefinition,
          changed,
          changed,
          reference,
          reference,
        ),
      wire.ContractError,
    );
  }
});

test("test_mediation_parameter_order_has_a_separate_exact_allowlist", () => {
  const caseDefinition = {
    id: "mediation-validation",
    baseline_exception: "BASELINE_MEDIATION_PARAMETER_ORDER",
  };
  const messages = [
    'imp: Validation error: length [{"value": Array [], "min": Number(1)}]',
    'imp: Validation error: length [{"min": Number(1), "value": Array []}]',
  ];
  const before = response(wire.error_body(422, "validation", messages[0]), 422);
  const after = response(wire.error_body(422, "validation", messages[1]), 422);
  assert.ok(
    compare(caseDefinition, before, after).includes(
      "BASELINE_MEDIATION_PARAMETER_ORDER: error.message",
    ),
  );
  for (const altered of [
    messages[1].replace("Number(1)", "Number(2)"),
    messages[1].replace("Array []", "Array [1]"),
    messages[1].replace("imp:", "other:"),
    messages[1] + " changed",
  ]) {
    assert.throws(
      () =>
        compare(
          caseDefinition,
          before,
          response(wire.error_body(422, "validation", altered), 422),
        ),
      wire.ContractError,
    );
  }
  assert.throws(
    () =>
      compare(
        caseDefinition,
        before,
        response(wire.error_body(422, "bad_request", messages[1]), 422),
      ),
    wire.ContractError,
  );
  assert.throws(
    () => compare(caseDefinition, before, response(after.body, 400)),
    wire.ContractError,
  );
  assert.throws(
    () =>
      compare(
        caseDefinition,
        before,
        response(after.body, 422, ["X-Mutated: yes"]),
      ),
    wire.ContractError,
  );
});

test("test_corpus_has_exact_boundaries_fallbacks_and_mediation_errors", () => {
  const cases = new Map(
    readJson(join(HERE, "requests.json")).cases.map((item) => [item.id, item]),
  );
  const sizes = {
    "auction-exact-1mib": 1024 ** 2,
    "aps-exact-1mib": 1024 ** 2,
    "mediation-exact-8mib": 8 * 1024 ** 2,
    "mediation-between-1-and-8mib": 2 * 1024 ** 2,
    "fallback-large-404": 9 * 1024 ** 2,
    "fallback-large-405": 9 * 1024 ** 2,
  };
  for (const [name, size] of Object.entries(sizes)) {
    assert.ok(cases.has(name), name);
    const [, , body] = wire.request_bytes(cases.get(name));
    assert.equal(body.length, size);
    assert.deepEqual(JSON.parse(body), JSON.parse(cases.get(name).body));
  }
  assert.ok(cases.has("mediation-malformed"));
  assert.ok(cases.has("mediation-validation"));
});

test("test_unicode_and_json_escapes_preserve_auction_uuid_spans", () => {
  const body = (uuid, escaped) => {
    const payload = auction(uuid);
    const metadata = JSON.parse(
      payload.seatbid[0].bid[0].adm.slice(
        "<!-- MOCKTIONEER_METADATA\n".length,
        -4,
      ),
    );
    const prefix = 'é😀\\"\n';
    payload.seatbid[0].bid[0].adm =
      "<!-- MOCKTIONEER_METADATA\n" +
      JSON.stringify({ prefix, ...metadata }, null, 2) +
      "\n-->";
    let text = JSON.stringify({ prefix, ...payload });
    if (escaped)
      text = text.replaceAll("é", "\\u00e9").replaceAll("😀", "\\ud83d\\ude00");
    return Buffer.from(text);
  };
  for (const escaped of [false, true]) {
    const before = response(body(UUID_A, escaped));
    const after = response(body(UUID_B, escaped));
    assert.deepEqual(
      compare({ mask: "auction", uuid_count: 2 }, before, after),
      [],
    );
    const [masked] = wire.mask_response({ mask: "auction" }, before);
    assert.deepEqual(
      masked.body,
      Buffer.from(
        before.body.toString().replaceAll(UUID_A, "1".padStart(32, "0")),
      ),
    );
    const changed = response(
      Buffer.from(after.body.toString().replace("prefix", "changed")),
    );
    assert.throws(
      () => compare({ mask: "auction" }, before, changed),
      wire.ContractError,
    );
  }
});

test("test_masked_responses_reject_invalid_utf8_without_replacing_bytes", () => {
  const invalid = response(Buffer.from([0xff, 0xfe]));
  assert.deepEqual(compare({}, invalid, invalid), []);
  for (const mask of ["auction", "creative", "aps", "mediation"]) {
    assert.throws(
      () => wire.mask_response({ mask }, invalid),
      wire.ContractError,
    );
  }
});

test("test_uuid_trailing_newline_is_not_a_valid_generated_id", () => {
  for (const suffix of ["\n", "\r\n"]) {
    const item = response(auction(UUID_A + suffix));
    assert.throws(
      () => compare({ mask: "auction" }, item, item),
      wire.ContractError,
    );
  }
});

test("test_json_number_spelling_and_unicode_escape_bytes_are_protected", () => {
  const payload = { prefix: "é", ...auction() };
  const before = response(payload);
  for (const text of [
    before.body.toString().replace("0.35", "0.350"),
    before.body.toString().replace("é", "\\u00e9"),
  ]) {
    assert.deepEqual(JSON.parse(text), payload);
    assert.throws(
      () => compare({ mask: "auction" }, before, response(Buffer.from(text))),
      wire.ContractError,
    );
  }
});
