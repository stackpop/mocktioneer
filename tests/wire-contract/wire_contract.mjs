#!/usr/bin/env node
// Capture local HTTP/1 wire contracts; compare bytes with narrow, named exceptions.
import { createHash } from "node:crypto";
import {
  readFileSync,
  writeFileSync,
  mkdirSync,
  existsSync,
  realpathSync,
} from "node:fs";
import { spawnSync } from "node:child_process";
import { createConnection } from "node:net";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { isDeepStrictEqual, parseArgs } from "node:util";

const CORPUS = join(dirname(fileURLToPath(import.meta.url)), "requests.json");
export const BASE_URL = "http://127.0.0.1:8787";
export const MISSING =
  "missing typed app-config blob at key `mocktioneer_config`";
export const PUSH_HINT = "run `<app-cli> config push` for this deploy";
export const STREAM_ERROR =
  "invalid JSON payload: streaming body cannot be materialised as JSON";
const CORS_HEADERS = [
  "access-control-allow-origin: *",
  "access-control-allow-methods: GET, POST, OPTIONS",
  "access-control-allow-headers: *, content-type",
].map((line) => Buffer.from(line));
const CORS_ERROR_STATUSES = {
  "image-nonstandard": 422,
  "creative-nonstandard": 404,
  "pixel-missing": 400,
  "aps-validation": 422,
  "aps-malformed": 400,
  "auction-malformed": 400,
  "auction-validation": 422,
  "sync-start-missing": 400,
  "mediation-malformed": 400,
  "mediation-validation": 422,
  "auction-over-1mib": 400,
  "aps-over-1mib": 400,
  "mediation-over-8mib": 400,
  "unseeded-auction-missing-config": 503,
  "unseeded-aps-missing-config": 503,
};
const BODY_CAP_CASES = new Set([
  "auction-over-1mib",
  "aps-over-1mib",
  "mediation-over-8mib",
]);
const UTF8 = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true });
const MAX_JSON_DEPTH = 1000;
const equal = isDeepStrictEqual;
const digest = (bytes) => createHash("sha256").update(bytes).digest("hex");
const json_file = (path) => JSON.parse(UTF8.decode(readFileSync(path)));
const json_write = (path, value) =>
  writeFileSync(path, JSON.stringify(value, null, 2) + "\n");
const head_bytes = (lines) =>
  Buffer.from(
    lines.map((line) => line.toString("latin1")).join("\r\n") + "\r\n\r\n",
    "latin1",
  );

export class ContractError extends Error {}

function require(condition, message) {
  if (!condition) throw new ContractError(message);
}

export class Response {
  constructor(head, body) {
    this.head = head;
    this.body = body;
  }

  get lines() {
    require(this.head
      .subarray(-4)
      .equals(Buffer.from("\r\n\r\n")), "missing complete response head");
    const lines = this.head.subarray(0, -4).toString("latin1").split("\r\n");
    require(lines[0].startsWith("HTTP/1.1 "), "expected one HTTP/1.1 response");
    require(lines
      .slice(1)
      .every((line) =>
        line.includes(":"),
      ), "unexpected interim/malformed head");
    return lines.map((line) => Buffer.from(line, "latin1"));
  }

  get status() {
    const token = this.lines[0].toString("latin1").split(" ")[1];
    require(/^\d+$/.test(token), "invalid response status");
    return Number(token);
  }
}

function header_name(line) {
  return line
    .toString("latin1")
    .split(":", 1)[0]
    .replace(/[A-Z]/g, (letter) => letter.toLowerCase());
}

function field_value(line) {
  return line
    .subarray(line.indexOf(58) + 1)
    .toString("latin1")
    .replace(/^[\t\n\v\f\r ]+|[\t\n\v\f\r ]+$/g, "");
}

// Return original string spans without serializing or reordering JSON containers.
function scan_json_value(text, initial = 0) {
  const sites = [];
  const skip = (pos) => {
    while (pos < text.length && " \t\r\n".includes(text[pos])) pos++;
    return pos;
  };
  const string_end = (start) => {
    require(text[start] === '"', "malformed JSON string");
    let pos = start + 1;
    while (pos < text.length) {
      if (text[pos] === '"') return pos + 1;
      if (text[pos] === "\\") pos++;
      pos++;
    }
    throw new ContractError("unterminated JSON string");
  };
  function walk(start, path, depth) {
    require(depth <= MAX_JSON_DEPTH, "JSON nesting exceeds 1000 levels");
    let pos = skip(start);
    require(pos < text.length, "incomplete JSON value");
    if (text[pos] === "{" || text[pos] === "[") {
      const object = text[pos] === "{";
      const closing = object ? "}" : "]";
      pos = skip(pos + 1);
      if (text[pos] === closing) return pos + 1;
      let index = 0;
      while (true) {
        let key = index;
        if (object) {
          const end = string_end(pos);
          key = JSON.parse(text.slice(pos, end));
          pos = skip(end);
          require(text[pos] === ":", "malformed JSON object");
          pos++;
        }
        pos = skip(walk(pos, [...path, key], depth + 1));
        index++;
        if (text[pos] === closing) return pos + 1;
        require(text[pos] === ",", "malformed JSON container");
        pos = skip(pos + 1);
        require(text[pos] !== closing, "trailing JSON comma");
      }
    }
    if (text[pos] === '"') {
      const end = string_end(pos);
      sites.push([path, pos, end, JSON.parse(text.slice(pos, end))]);
      return end;
    }
    const scalar =
      /^(?:null|true|false|-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?)/.exec(
        text.slice(pos),
      );
    require(scalar, "malformed JSON scalar");
    return pos + scalar[0].length;
  }
  try {
    return { sites, end: walk(initial, [], 0), skip };
  } catch (error) {
    if (error instanceof ContractError) throw error;
    if (error instanceof SyntaxError || error instanceof RangeError) {
      throw new ContractError(`invalid response JSON: ${error.message}`);
    }
    throw error;
  }
}

export function string_sites(text) {
  const { sites, end, skip } = scan_json_value(text);
  require(skip(end) === text.length, "trailing JSON data");
  return sites;
}

function bid_id_path(path) {
  return (
    path.length === 5 &&
    path[0] === "seatbid" &&
    Number.isInteger(path[1]) &&
    path[2] === "bid" &&
    Number.isInteger(path[3]) &&
    path[4] === "id"
  );
}

// Both offsets count UTF-16 units, including each half of a surrogate pair.
function raw_string_offset(text, token_start, decoded_offset) {
  let pos = token_start + 1;
  for (let index = 0; index < decoded_offset; index++) {
    pos += text[pos] === "\\" ? (text[pos + 1] === "u" ? 6 : 2) : 1;
  }
  return pos;
}

export function mask_response(case_, response) {
  const lines = response.lines.map((line) =>
    header_name(line) === "date"
      ? Buffer.concat([
          line.subarray(0, line.indexOf(58)),
          Buffer.from(": <DATE>"),
        ])
      : line,
  );
  const mode = case_.mask;
  if (!mode || response.status !== 200)
    return [new Response(head_bytes(lines), response.body), []];
  let text;
  try {
    text = UTF8.decode(response.body);
  } catch (error) {
    throw new ContractError(`invalid response UTF-8: ${error.message}`);
  }
  const replacements = [],
    signature = [],
    tokens = new Map();
  function add(start, end, value, label, suffix = "") {
    const uuid = suffix ? value.slice(0, -suffix.length) : value;
    require(uuid.length === 32 &&
      /^[0-9a-f]{12}7[0-9a-f]{3}[89ab][0-9a-f]{15}$/.test(
        uuid,
      ), `invalid UUIDv7 at ${label}: ${JSON.stringify(uuid)}`);
    if (!tokens.has(uuid))
      tokens.set(uuid, (tokens.size + 1).toString(16).padStart(32, "0"));
    const token = tokens.get(uuid);
    require(text.slice(start, end) ===
      uuid, `unexpected UUID encoding at ${label}`);
    replacements.push([start, end, token]);
    signature.push([label, token]);
  }
  if (mode === "creative") {
    const html_ids = [];
    for (const [pattern, label] of [
      [/\/pixel\?pid=([^"\s]+)/g, "pixel-html"],
      [/data-static-pid="([^"\s]+)"/g, "pixel-html-reference"],
      [/var jsPid = "([^"\s]+)"/g, "pixel-js"],
    ]) {
      for (const match of text.matchAll(pattern)) {
        const start = match.index + match[0].indexOf(match[1]);
        add(start, start + match[1].length, match[1], label);
        if (label.startsWith("pixel-html")) html_ids.push(match[1]);
      }
    }
    require(!html_ids.length ||
      (html_ids.length === 2 &&
        html_ids[0] === html_ids[1]), "HTML pixel ID/reference mismatch");
  } else {
    const sites = string_sites(text),
      primary = new Map();
    for (const [path, start, end, value] of sites) {
      let suffix = "";
      let selected =
        ["auction", "mediation"].includes(mode) && bid_id_path(path);
      if (
        mode === "aps" &&
        path.length === 4 &&
        equal(path.slice(0, 2), ["contextual", "slots"])
      ) {
        selected = ["amzniid", "crid"].includes(path.at(-1));
        suffix = path.at(-1) === "crid" ? "-mocktioneer" : "";
      }
      if (selected) {
        add(
          start + 1,
          end - 1 - suffix.length,
          value,
          JSON.stringify(path),
          suffix,
        );
        primary.set(JSON.stringify(path), value);
      }
    }
    if (mode === "auction") {
      const marker = "<!-- MOCKTIONEER_METADATA\n";
      for (const [path, start, , adm] of sites) {
        if (
          path.length !== 5 ||
          path.at(-1) !== "adm" ||
          !bid_id_path([...path.slice(0, -1), "id"])
        )
          continue;
        require(adm.startsWith(marker), "missing auction metadata");
        const { end } = scan_json_value(adm, marker.length);
        const metadata = adm.slice(marker.length, end);
        for (const [nested, begin, , value] of string_sites(metadata)) {
          if (nested[0] !== "response" || !bid_id_path(nested.slice(1)))
            continue;
          require(primary.get(JSON.stringify(nested.slice(1))) ===
            value, "preview UUID reference mismatch");
          const offset = raw_string_offset(
            text,
            start,
            marker.length + begin + 1,
          );
          add(
            offset,
            offset + value.length,
            value,
            `${JSON.stringify(path)}/${JSON.stringify(nested)}`,
          );
        }
      }
    }
  }
  replacements.sort((a, b) => b[0] - a[0] || b[1] - a[1]);
  for (const [start, end, token] of replacements)
    text = text.slice(0, start) + token + text.slice(end);
  if ("uuid_count" in case_)
    require(signature.length ===
      case_.uuid_count, "unexpected generated UUID occurrence count");
  return [new Response(head_bytes(lines), Buffer.from(text)), signature];
}

export function error_body(status, kind, message) {
  return Buffer.from(JSON.stringify({ error: { kind, message, status } }));
}

function remove_field(lines, name, expected = null) {
  const found = lines.slice(1).filter((line) => header_name(line) === name);
  if (expected !== null)
    require(found.length === 1 &&
      field_value(found[0]) === expected, `unexpected ${name} header`);
  return lines.filter((line) => header_name(line) !== name);
}

function normalize_length(lines, original) {
  const fields = lines
    .slice(1)
    .filter((line) => header_name(line) === "content-length");
  if (fields.length)
    require(fields.length === 1 &&
      field_value(fields[0]) ===
        String(
          original.length,
        ), "Content-Length does not match original body bytes");
  return lines.map((line) =>
    header_name(line) === "content-length"
      ? Buffer.concat([
          line.subarray(0, line.indexOf(58)),
          Buffer.from(": <LENGTH>"),
        ])
      : line,
  );
}

function remove_cors(lines) {
  for (const field of CORS_HEADERS) {
    const name = header_name(field);
    if (lines.slice(1).some((line) => header_name(line) === name))
      lines = remove_field(lines, name, field_value(field));
  }
  return lines;
}

function require_cors(lines) {
  for (const field of CORS_HEADERS) {
    const found = lines
      .slice(1)
      .filter((line) => header_name(line) === header_name(field));
    require(equal(found, [
      field,
    ]), `expected one exact ${header_name(field)} header`);
  }
}

// Reproduce only HeaderMap's CL group swap/remove/reinsert, before host Date.
function canonical_length_order(lines, empty_insertion = false) {
  const tail = header_name(lines.at(-1)) === "date" ? lines.slice(-1) : [];
  const fields = tail.length ? lines.slice(1, -1) : lines.slice(1),
    groups = [];
  for (const field of fields) {
    if (groups.length && header_name(groups.at(-1)[0]) === header_name(field))
      groups.at(-1).push(field);
    else groups.push([field]);
  }
  const names = groups.map((group) => header_name(group[0]));
  if (
    new Set(names).size !== names.length ||
    names.filter((name) => name === "content-length").length !== 1
  )
    return null;
  let index = names.indexOf("content-length");
  const length = groups[index];
  if (length.length !== 1) return null;
  if (empty_insertion) {
    if (field_value(length[0]) !== "0") return null;
    groups.splice(index, 1);
    groups.unshift(length);
    index = 0;
  }
  groups[index] = groups.at(-1);
  groups.pop();
  groups.push(length);
  return [lines[0], ...groups.flat(), ...tail];
}

export function compare_response(
  case_,
  before,
  after,
  reference_before = null,
  reference_after = null,
) {
  let [left, left_ids] = mask_response(case_, before);
  const [right, right_ids] = mask_response(case_, after),
    changes = [];
  let a = left.lines,
    b = right.lines;
  const allowed = case_.differences ?? [],
    transition = case_.transition;
  const body_cap = BODY_CAP_CASES.has(case_.id) && transition === "body-cap";
  const f14 = allowed.includes("F14");
  if (f14) {
    require(body_cap, "F14 used outside its three named body-cap cases");
    require([
      [200, 200],
      [400, 400],
      [413, 413],
      [200, 413],
      [400, 413],
    ].some((pair) =>
      equal(pair, [before.status, after.status]),
    ), "unexpected F14 body-cap status transition");
    require_cors(a);
    require_cors(b);
    for (const response of [before, after]) {
      if ([400, 413].includes(response.status)) {
        const kind =
          response.status === 413 ? "payload_too_large" : "bad_request";
        require(response.body.equals(
          error_body(response.status, kind, "request body too large"),
        ), "unlisted F14 body-cap error");
        normalize_length(response.lines, response.body);
      }
      if (response.status === 413)
        require(!response.lines
          .slice(1)
          .some(
            (line) => header_name(line) === "retry-after",
          ), "F14 body-cap 413 must not include Retry-After");
    }
  }
  if (allowed.includes("C8")) {
    const expected = f14 ? 413 : CORS_ERROR_STATUSES[case_.id];
    require(expected !== undefined &&
      ((before.status === expected && after.status === expected) ||
        (body_cap &&
          before.status === 200 &&
          [200, expected].includes(after.status)) ||
        (f14 &&
          before.status === 400 &&
          [400, 413].includes(
            after.status,
          ))), "C8 used outside its named matched-error status");
    const cors_names = new Set(CORS_HEADERS.map(header_name));
    const before_cors = a
      .slice(1)
      .some((line) => cors_names.has(header_name(line)));
    const after_cors = b
      .slice(1)
      .some((line) => cors_names.has(header_name(line)));
    if (before_cors || body_cap) {
      require_cors(a);
      require_cors(b);
    } else if (after_cors) {
      require_cors(b);
      const tail = header_name(a.at(-1)) === "date" ? a.slice(-1) : [];
      a = [...(tail.length ? a.slice(0, -1) : a), ...CORS_HEADERS, ...tail];
      changes.push("C8: matched-error CORS");
    }
  }
  const baseline_exception = case_.baseline_exception;
  if (
    [
      "BASELINE_APS_PARAMETER_ORDER",
      "BASELINE_MEDIATION_PARAMETER_ORDER",
    ].includes(baseline_exception)
  ) {
    const [id, field] =
      baseline_exception === "BASELINE_APS_PARAMETER_ORDER"
        ? ["aps-validation", "slots"]
        : ["mediation-validation", "imp"];
    require(case_.id === id &&
      before.status === 422 &&
      after.status ===
        422, "parameter-order exception used outside its single case");
    const bodies = [
      `${field}: Validation error: length [{"value": Array [], "min": Number(1)}]`,
      `${field}: Validation error: length [{"min": Number(1), "value": Array []}]`,
    ].map((message) => error_body(422, "validation", message));
    require(bodies.some((body) => body.equals(before.body)) &&
      bodies.some((body) =>
        body.equals(after.body),
      ), "unlisted baseline validation error");
    if (!before.body.equals(after.body)) {
      left = new Response(left.head, right.body);
      changes.push(`${baseline_exception}: error.message`);
    }
  }
  if (["body-boundary", "fallback"].includes(transition)) {
    require(before.status === after.status &&
      before.status ===
        case_.required_status, "unexpected boundary/fallback status");
    require(reference_before !== null &&
      reference_after !==
        null, "boundary/fallback requires both small-body reference captures");
    compare_response({ mask: case_.mask }, reference_before, before);
    compare_response({ mask: case_.mask }, reference_after, after);
  }
  if (
    reference_before !== null &&
    ["body-cap", "head-target"].includes(transition) &&
    before.status === 200 &&
    before.status !== after.status
  ) {
    compare_response({ mask: case_.mask }, reference_before, before);
  }
  if (transition === "no-content-type" && before.status !== after.status) {
    require(allowed.includes("F8") &&
      before.status === 400 &&
      after.status === 200, "unexpected Content-Type status transition");
    require(before.body.equals(
      error_body(400, "bad_request", STREAM_ERROR),
    ), "unlisted old Content-Type error");
    require(reference_after !==
      null, "F8 requires the successful reference capture");
    compare_response(
      {
        mask: case_.mask,
        ...("uuid_count" in case_ ? { uuid_count: case_.uuid_count } : {}),
      },
      reference_after,
      after,
    );
    a = normalize_length(a, before.body);
    b = normalize_length(remove_cors(b), after.body);
    a[0] = b[0];
    left = new Response(left.head, right.body);
    left_ids = right_ids;
    changes.push(
      "F8: status/body/Content-Length/CORS (successful reference verified)",
    );
  }
  if (transition === "head-target" && before.status !== after.status) {
    require(allowed.includes("F5") &&
      before.status === 200 &&
      after.status === 414, "unexpected normalized-target status transition");
    require(reference_before !==
      null, "F5 requires the baseline root reference capture");
    require(after.body.equals(
      error_body(
        414,
        "uri_too_long",
        "normalized request target exceeds configured limit",
      ),
    ), "unlisted normalized-target error");
    a = normalize_length(remove_cors(a), before.body);
    b = normalize_length(b, after.body);
    if (
      [...a.slice(1), ...b.slice(1)].some(
        (line) => header_name(line) === "content-type",
      )
    ) {
      a = remove_field(a, "content-type", "text/html; charset=utf-8");
      b = remove_field(b, "content-type", "application/json");
    }
    a[0] = b[0];
    left = new Response(left.head, right.body);
    changes.push("F5: status/body/Content-Length/Content-Type/CORS");
  }
  if (allowed.includes("F4") && before.status === 405 && after.status === 405) {
    const old_allow = a
      .slice(1)
      .filter((line) => header_name(line) === "allow");
    const new_allow = b
      .slice(1)
      .filter((line) => header_name(line) === "allow");
    if (!equal(old_allow, new_allow)) {
      require(!old_allow.length, "F4 only permits adding the missing Allow field");
      b = remove_field(b, "allow", case_.allow);
      changes.push("F4: Allow");
    }
  }
  if (allowed.includes("F2") && !left.body.equals(right.body)) {
    const old = error_body(
      503,
      "config_out_of_date",
      `${MISSING} — ${PUSH_HINT}`,
    );
    const next = error_body(
      503,
      "config_out_of_date",
      `${MISSING}; ${PUSH_HINT}`,
    );
    require(before.status === 503 &&
      after.status === 503 &&
      left.body.equals(old) &&
      right.body.equals(next), "unlisted config error change");
    left = new Response(left.head, next);
    a = normalize_length(a, before.body);
    b = normalize_length(b, after.body);
    changes.push("F2: error.message/Content-Length");
  }
  if (transition === "body-cap" && before.status !== after.status) {
    let number = allowed.includes("C3") ? "C3" : "F7";
    if (f14 && before.status === 400) number = "F14";
    const [status, kind] = f14
      ? [413, "payload_too_large"]
      : [400, "bad_request"];
    require(allowed.includes(number) &&
      (f14 ? [200, 400] : [200]).includes(before.status) &&
      after.status === status, "unexpected body-cap status transition");
    require(after.body.equals(
      error_body(status, kind, "request body too large"),
    ), "unlisted body-cap error");
    if (f14 && before.status === 200) {
      require(reference_before !==
        null, "F14 requires the successful baseline reference capture");
      number += "/F14";
    }
    if (!allowed.includes("C8") && !f14) a = remove_cors(a);
    a = normalize_length(a, before.body);
    b = normalize_length(b, after.body);
    a[0] = b[0];
    left = new Response(left.head, right.body);
    left_ids = [];
    changes.push(`${number}: status/body/Content-Length`);
  }
  const empty =
    !before.body.length &&
    !after.body.length &&
    before.status === after.status &&
    before.status >= 200 &&
    ![204, 304].includes(before.status);
  if (allowed.includes("C6") && empty) {
    const old = a
      .slice(1)
      .filter((line) => header_name(line) === "content-length");
    const next = b
      .slice(1)
      .filter((line) => header_name(line) === "content-length");
    if (!old.length && next.length) {
      b = remove_field(b, "content-length", "0");
      changes.push("C6: Content-Length: 0");
    }
  }
  if (!equal(a, b) && allowed.includes("F10")) {
    if (equal(canonical_length_order(a), b)) {
      a = b;
      changes.push("F10: exact Content-Length group permutation");
    } else if (
      allowed.includes("C6") &&
      empty &&
      equal(canonical_length_order(a, true), b)
    ) {
      a = b;
      changes.push("C6/F10: exact empty-response header permutation");
    }
  }
  require(equal(left_ids, right_ids), "UUID occurrence/reference mismatch");
  require(equal(
    a,
    b,
  ), `status/ordered headers differ: ${JSON.stringify(a.map((line) => line.toString("latin1")))} != ${JSON.stringify(b.map((line) => line.toString("latin1")))}`);
  require(left.body.equals(
    right.body,
  ), "body bytes differ (including whitespace/order)");
  return changes;
}

export function request_bytes(case_) {
  let body = Buffer.from(case_.body ?? "");
  if ("body_pad_to" in case_) {
    require(case_.body_pad_to >= body.length, "body padding target too small");
    body = Buffer.concat([
      body,
      Buffer.alloc(case_.body_pad_to - body.length, 32),
    ]);
  }
  const path = case_.path + "x".repeat(case_.target_pad ?? 0);
  const headers = [...case_.headers];
  for (let index = 0; index < (case_.extra_headers ?? 0); index++)
    headers.push(`X-Wire-${String(index).padStart(3, "0")}: x`);
  return [path, headers, body];
}

function fresh_directory(path) {
  mkdirSync(dirname(path), { recursive: true });
  mkdirSync(path);
}

function capture(args) {
  const corpus_bytes = readFileSync(CORPUS),
    corpus = JSON.parse(UTF8.decode(corpus_bytes));
  const out = resolve(args.out);
  fresh_directory(out);
  writeFileSync(join(out, "requests.json"), corpus_bytes);
  const manifest = {
    scenario: args.scenario,
    base_url: args["base-url"],
    corpus_sha256: digest(corpus_bytes),
    cases: [],
  };
  for (const case_ of corpus.cases) {
    if (case_.scenario !== args.scenario) continue;
    const dest = join(out, case_.id);
    mkdirSync(dest);
    const [path, headers, body] = request_bytes(case_);
    writeFileSync(join(dest, "request.body"), body);
    json_write(join(dest, "request.json"), {
      method: case_.method,
      path,
      headers,
    });
    const cmd = [
      "-q",
      "--http1.1",
      "--raw",
      "-sS",
      "--path-as-is",
      "--noproxy",
      "*",
      "--max-time",
      "45",
      "-D",
      join(dest, "response.headers"),
      "-o",
      join(dest, "response.body"),
      "-X",
      case_.method,
    ];
    for (const header of headers) cmd.push("-H", header);
    if (body.length || case_.method === "POST")
      cmd.push("--data-binary", "@" + join(dest, "request.body"));
    const result = spawnSync("curl", [...cmd, args["base-url"] + path]);
    writeFileSync(
      join(dest, "curl.stderr"),
      result.stderr ?? Buffer.from(result.error?.message ?? ""),
    );
    const status = result.status ?? 1;
    writeFileSync(join(dest, "curl.exit"), `${status}\n`);
    manifest.cases.push(case_.id);
    console.log(`${case_.id}: curl=${status}`);
    if (result.error) throw result.error;
  }
  json_write(join(out, "capture.json"), manifest);
}

function read_response(directory, case_id) {
  const dest = join(directory, case_id);
  require(readFileSync(join(dest, "curl.exit"), "utf8").trim() ===
    "0", `${case_id}: curl failed; see curl.stderr`);
  return new Response(
    readFileSync(join(dest, "response.headers")),
    readFileSync(join(dest, "response.body")),
  );
}

function read_capture(directory) {
  const manifest = json_file(join(directory, "capture.json"));
  const corpus_bytes = readFileSync(join(directory, "requests.json")),
    corpus = JSON.parse(UTF8.decode(corpus_bytes));
  require(manifest &&
    typeof manifest === "object" &&
    !Array.isArray(manifest), "invalid capture manifest");
  require(["seeded", "unseeded"].includes(
    manifest.scenario,
  ), "invalid capture scenario");
  require(manifest.base_url === BASE_URL, "invalid capture base URL");
  require(manifest.corpus_sha256 ===
    digest(corpus_bytes), "capture corpus digest does not match requests.json");
  require(corpus &&
    Array.isArray(corpus.cases) &&
    corpus.cases.every(
      (case_) => case_ && typeof case_ === "object" && !Array.isArray(case_),
    ), "invalid captured corpus");
  const ids = corpus.cases.map((case_) => case_.id);
  require(ids.every(
    (id) => typeof id === "string" && id.length,
  ), "invalid corpus case ID");
  require(new Set(ids).size === ids.length, "duplicate corpus case ID");
  require(corpus.cases.every((case_) =>
    ["seeded", "unseeded"].includes(case_.scenario),
  ), "invalid corpus scenario");
  const expected = corpus.cases
    .filter((case_) => case_.scenario === manifest.scenario)
    .map((case_) => case_.id);
  require(expected.length &&
    equal(
      manifest.cases,
      expected,
    ), "capture case list must match the complete, ordered scenario corpus");
  return [
    manifest,
    corpus_bytes,
    new Map(corpus.cases.map((case_) => [case_.id, case_])),
  ];
}

function compare(args) {
  const before = resolve(args.before),
    after = resolve(args.after);
  const [a, corpus_a, cases] = read_capture(before),
    [b, corpus_b] = read_capture(after);
  require(equal(
    a,
    b,
  ), "capture scenario, base URL, corpus digest or case list differs");
  require(corpus_a.equals(corpus_b), "captured corpus differs");
  let failures = 0;
  for (const id of a.cases) {
    try {
      const case_ = cases.get(id),
        [path, headers, body] = request_bytes(case_);
      for (const directory of [before, after]) {
        const dest = join(directory, id);
        require(equal(json_file(join(dest, "request.json")), {
          method: case_.method,
          path,
          headers,
        }), "recorded request metadata differs from corpus");
        require(readFileSync(join(dest, "request.body")).equals(
          body,
        ), "recorded request body differs from corpus");
      }
      for (const name of ["request.json", "request.body"])
        require(readFileSync(join(before, id, name)).equals(
          readFileSync(join(after, id, name)),
        ), "request bytes differ");
      const reference = case_.reference,
        before_response = read_response(before, id),
        after_response = read_response(after, id);
      const changes = compare_response(
        case_,
        before_response,
        after_response,
        reference ? read_response(before, reference) : null,
        reference ? read_response(after, reference) : null,
      );
      // The helper also compares baselines; CLI candidates additionally require 413.
      if (case_.differences?.includes("F14"))
        require(args["baseline-repeat"]
          ? before_response.status === after_response.status
          : after_response.status === 413, args["baseline-repeat"]
          ? "F14 baseline repeat requires unchanged status"
          : "F14 candidate requires 413");
      console.log(
        `PASS ${id}${changes.length ? ": " + changes.join("; ") : ""}`,
      );
    } catch (error) {
      failures++;
      console.error(`FAIL ${id}: ${error.message}`);
    }
  }
  require(!failures, `${failures} wire case(s) failed`);
}

async function characterize_raw(args) {
  const out = resolve(args.out);
  fresh_directory(out);
  const cases = {
    "identical-content-length":
      "Content-Length: 0\r\nContent-Length: 0\r\n\r\n",
    "content-length-and-chunked":
      "Content-Length: 0\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n",
  };
  const report = {
    scope:
      "Host-parser characterization only; no universal 400 assertion (F5/F9).",
    cases: {},
  };
  for (const [id, tail] of Object.entries(cases)) {
    const request = Buffer.from(
      "OPTIONS / HTTP/1.1\r\nHost: 127.0.0.1:8787\r\nConnection: close\r\n" +
        tail,
    );
    writeFileSync(join(out, id + ".request"), request);
    const chunks = [];
    let outcome = "closed";
    await new Promise((done) => {
      const connection = createConnection({ host: "127.0.0.1", port: 8787 });
      connection.setTimeout(5000, () => {
        outcome = "TimeoutError: socket timed out";
        connection.destroy();
      });
      connection.on("connect", () => connection.write(request));
      connection.on("data", (chunk) => chunks.push(chunk));
      connection.on("error", (error) => {
        outcome = `${error.name}: ${error.message}`;
      });
      connection.on("close", done);
    });
    const response = Buffer.concat(chunks);
    writeFileSync(join(out, id + ".response"), response);
    report.cases[id] = {
      outcome,
      response_bytes: response.length,
      status_line: response.toString("latin1").split("\r\n", 1)[0],
    };
  }
  json_write(join(out, "characterization.json"), report);
  console.log(JSON.stringify(report, null, 2));
}

async function main() {
  const [command, ...args] = process.argv.slice(2);
  const definitions = {
    capture: ["base-url", "scenario", "out"],
    compare: ["before", "after"],
    "characterize-raw": ["base-url", "out"],
  };
  if (["--help", "-h"].includes(command)) {
    console.log(
      "Capture local HTTP/1 wire contracts; compare bytes with narrow, named exceptions.\nUsage: node wire_contract.mjs <capture|compare|characterize-raw> [options]",
    );
    return;
  }
  require(Object.hasOwn(
    definitions,
    command,
  ), "use capture, compare or characterize-raw");
  const keys = definitions[command];
  const { values } = parseArgs({
    args,
    options: {
      ...Object.fromEntries(keys.map((key) => [key, { type: "string" }])),
      ...(command === "compare"
        ? { "baseline-repeat": { type: "boolean" } }
        : {}),
      help: { type: "boolean", short: "h" },
    },
  });
  if (values.help) {
    console.log(
      `Usage: node wire_contract.mjs ${command} ${keys.map((key) => `--${key} <${key === "scenario" ? "seeded|unseeded" : "value"}>`).join(" ")}${command === "compare" ? " [--baseline-repeat]" : ""}`,
    );
    return;
  }
  for (const key of keys) require(values[key], `missing --${key}`);
  if ("base-url" in values) {
    require(values["base-url"].replace(/\/+$/, "") ===
      BASE_URL, `use the fixed base URL ${BASE_URL}`);
    values["base-url"] = BASE_URL;
  }
  if ("scenario" in values)
    require(["seeded", "unseeded"].includes(
      values.scenario,
    ), "invalid capture scenario");
  await { capture, compare, "characterize-raw": characterize_raw }[command](
    values,
  );
}

if (
  process.argv[1] &&
  existsSync(process.argv[1]) &&
  realpathSync(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  try {
    await main();
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
