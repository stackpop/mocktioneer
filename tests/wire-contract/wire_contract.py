#!/usr/bin/env python3
"""Capture local HTTP/1 wire contracts; compare bytes with narrow, named exceptions."""

import argparse
from dataclasses import dataclass
import hashlib
import json
from pathlib import Path
import re
import socket
import subprocess
import sys


CORPUS = Path(__file__).with_name("requests.json")
BASE_URL = "http://127.0.0.1:8787"
UUID7 = re.compile(r"[0-9a-f]{12}7[0-9a-f]{3}[89ab][0-9a-f]{15}\Z")
MISSING = "missing typed app-config blob at key `mocktioneer_config`"
PUSH_HINT = "run `<app-cli> config push` for this deploy"
STREAM_ERROR = "invalid JSON payload: streaming body cannot be materialised as JSON"
CORS_HEADERS = (b"access-control-allow-origin: *",
                b"access-control-allow-methods: GET, POST, OPTIONS",
                b"access-control-allow-headers: *, content-type")
CORS_ERROR_STATUSES = {
    "image-nonstandard": 422, "creative-nonstandard": 404, "pixel-missing": 400,
    "aps-validation": 422, "aps-malformed": 400, "auction-malformed": 400,
    "auction-validation": 422, "sync-start-missing": 400, "mediation-malformed": 400,
    "mediation-validation": 422, "auction-over-1mib": 400, "aps-over-1mib": 400,
    "mediation-over-8mib": 400, "unseeded-auction-missing-config": 503,
    "unseeded-aps-missing-config": 503,
}
BODY_CAP_CASES = {"auction-over-1mib", "aps-over-1mib", "mediation-over-8mib"}


class ContractError(ValueError):
    pass


def require(condition, message):
    if not condition:
        raise ContractError(message)


@dataclass
class Response:
    head: bytes
    body: bytes

    @property
    def lines(self):
        require(self.head.endswith(b"\r\n\r\n"), "missing complete response head")
        lines = self.head[:-4].split(b"\r\n")
        require(lines[0].startswith(b"HTTP/1.1 "), "expected one HTTP/1.1 response")
        require(all(b":" in line for line in lines[1:]), "unexpected interim/malformed head")
        return lines

    @property
    def status(self):
        return int(self.lines[0].split(b" ", 2)[1])


def header_name(line):
    return line.split(b":", 1)[0].lower()


def string_sites(text):
    """Locate JSON string values without serializing or reordering their container."""
    decoder = json.JSONDecoder()
    sites = []

    def skip(pos):
        while pos < len(text) and text[pos] in " \t\r\n":
            pos += 1
        return pos

    def walk(pos, path):
        pos = skip(pos)
        if text[pos] in "{[":
            is_object = text[pos] == "{"
            end_char = "}" if is_object else "]"
            pos = skip(pos + 1)
            index = 0
            while text[pos] != end_char:
                if is_object:
                    key, pos = decoder.raw_decode(text, pos)
                    pos = skip(pos)
                    require(text[pos] == ":", "malformed JSON object")
                    pos += 1
                else:
                    key = index
                pos = skip(walk(pos, path + (key,)))
                index += 1
                if text[pos] == ",":
                    pos = skip(pos + 1)
                else:
                    break
            require(text[pos] == end_char, "malformed JSON container")
            return pos + 1
        value, end = decoder.raw_decode(text, pos)
        if isinstance(value, str):
            sites.append((path, pos, end, value))
        return end

    try:
        decoder.decode(text)
        require(skip(walk(0, ())) == len(text), "trailing JSON data")
    except (json.JSONDecodeError, RecursionError) as error:
        raise ContractError(f"invalid response JSON: {error}") from error
    return sites


def bid_id_path(path):
    return (len(path) == 5 and path[0] == "seatbid" and isinstance(path[1], int)
            and path[2] == "bid" and isinstance(path[3], int) and path[4] == "id")


def raw_string_offset(text, token_start, decoded_offset):
    """Map a decoded string offset through JSON escapes to its original wire offset."""
    pos = token_start + 1
    for _ in range(decoded_offset):
        if text[pos] == "\\":
            width = 6 if text[pos + 1] == "u" else 2
            if width == 6 and 0xD800 <= int(text[pos + 2:pos + 6], 16) <= 0xDBFF:
                if text[pos + 6:pos + 8] == "\\u":
                    width = 12
            pos += width
        else:
            pos += 1
    return pos


def mask_response(case, response):
    lines = response.lines
    lines = [line.split(b":", 1)[0] + b": <DATE>" if header_name(line) == b"date"
             else line for line in lines]
    mode = case.get("mask")
    if not mode or response.status != 200:
        return Response(b"\r\n".join(lines) + b"\r\n\r\n", response.body), []
    text = response.body.decode("utf-8")
    replacements, signature, tokens = [], [], {}

    def add(start, end, value, label, suffix=""):
        uuid = value[:-len(suffix)] if suffix else value
        require(UUID7.fullmatch(uuid), f"invalid UUIDv7 at {label}: {uuid!r}")
        token = tokens.setdefault(uuid, f"{len(tokens) + 1:032x}")
        require(text[start:end] == uuid, f"unexpected UUID encoding at {label}")
        replacements.append((start, end, token))
        signature.append((label, token))

    if mode == "creative":
        patterns = [(r'/pixel\?pid=([^"\s]+)', "pixel-html"),
                    (r'data-static-pid="([^"\s]+)"', "pixel-html-reference"),
                    (r'var jsPid = "([^"\s]+)"', "pixel-js")]
        html_ids = []
        for pattern, label in patterns:
            for match in re.finditer(pattern, text):
                add(*match.span(1), match[1], label)
                if label.startswith("pixel-html"):
                    html_ids.append(match[1])
        require(not html_ids or len(html_ids) == 2 and html_ids[0] == html_ids[1],
                "HTML pixel ID/reference mismatch")
    else:
        sites = string_sites(text)
        primary = {}
        for path, start, end, value in sites:
            suffix = ""
            selected = mode in ("auction", "mediation") and bid_id_path(path)
            if mode == "aps" and len(path) == 4 and path[:2] == ("contextual", "slots"):
                selected = path[-1] in ("amzniid", "crid")
                suffix = "-mocktioneer" if path[-1] == "crid" else ""
            if selected:
                add(start + 1, end - 1 - len(suffix), value, str(path), suffix)
                primary[path] = value
        if mode == "auction":
            marker = "<!-- MOCKTIONEER_METADATA\n"
            for path, start, _, adm in sites:
                if len(path) != 5 or path[-1] != "adm" or not bid_id_path(path[:-1] + ("id",)):
                    continue
                require(adm.startswith(marker), "missing auction metadata")
                try:
                    _, end = json.JSONDecoder().raw_decode(adm, len(marker))
                except RecursionError as error:
                    raise ContractError(f"invalid auction metadata JSON: {error}") from error
                metadata = adm[len(marker):end]
                for nested, begin, finish, value in string_sites(metadata):
                    if nested[:1] != ("response",) or not bid_id_path(nested[1:]):
                        continue
                    require(primary.get(nested[1:]) == value, "preview UUID reference mismatch")
                    begin = raw_string_offset(text, start, len(marker) + begin + 1)
                    finish = begin + len(value)
                    add(begin, finish, value, str(path) + "/" + str(nested))
    for start, end, token in sorted(replacements, reverse=True):
        text = text[:start] + token + text[end:]
    if "uuid_count" in case:
        require(len(signature) == case["uuid_count"], "unexpected generated UUID occurrence count")
    return Response(b"\r\n".join(lines) + b"\r\n\r\n", text.encode("utf-8")), signature


def error_body(status, kind, message):
    return json.dumps({"error": {"kind": kind, "message": message, "status": status}},
                      ensure_ascii=False, separators=(",", ":")).encode()


def remove_field(lines, name, expected=None):
    found = [line for line in lines[1:] if header_name(line) == name]
    if expected is not None:
        require(len(found) == 1 and found[0].split(b":", 1)[1].strip() == expected,
                f"unexpected {name.decode()} header")
    return [line for line in lines if header_name(line) != name]


def normalize_length(lines, original):
    fields = [line for line in lines[1:] if header_name(line) == b"content-length"]
    if fields:
        require(len(fields) == 1 and fields[0].split(b":", 1)[1].strip() == str(len(original)).encode(),
                "Content-Length does not match original body bytes")
    return [line.split(b":", 1)[0] + b": <LENGTH>" if header_name(line) == b"content-length"
            else line for line in lines]


def remove_cors(lines):
    for field in CORS_HEADERS:
        name, value = field.split(b": ", 1)
        if any(header_name(line) == name for line in lines[1:]):
            lines = remove_field(lines, name, value)
    return lines


def require_cors(lines):
    for field in CORS_HEADERS:
        found = [line for line in lines[1:] if header_name(line) == header_name(field)]
        require(found == [field], f"expected one exact {header_name(field).decode()} header")


def canonical_length_order(lines, empty_insertion=False):
    """Reproduce only HeaderMap's CL group swap/remove/reinsert, before host Date."""
    tail = lines[-1:] if header_name(lines[-1]) == b"date" else []
    fields = lines[1:-1] if tail else lines[1:]
    groups = []
    for field in fields:
        if groups and header_name(groups[-1][0]) == header_name(field):
            groups[-1].append(field)
        else:
            groups.append([field])
    names = [header_name(group[0]) for group in groups]
    if len(set(names)) != len(names) or names.count(b"content-length") != 1:
        return None
    index = names.index(b"content-length")
    length = groups[index]
    if len(length) != 1:
        return None
    if empty_insertion:
        if length[0].split(b":", 1)[1].strip() != b"0":
            return None
        groups.pop(index)
        groups.insert(0, length)
        index = 0
    groups[index] = groups[-1]
    groups.pop()
    groups.append(length)
    return [lines[0], *(field for group in groups for field in group), *tail]


def compare_response(case, before, after, reference_before=None, reference_after=None):
    left, left_ids = mask_response(case, before)
    right, right_ids = mask_response(case, after)
    changes = []
    a, b = left.lines, right.lines
    allowed = case.get("differences", [])
    transition = case.get("transition")
    baseline_exception = case.get("baseline_exception")
    if "C8" in allowed:
        case_id = case.get("id")
        expected = CORS_ERROR_STATUSES.get(case_id)
        body_cap = case_id in BODY_CAP_CASES and transition == "body-cap"
        require(expected is not None and (
            before.status == after.status == expected
            or body_cap and before.status == 200 and after.status in (200, expected)
        ), "C8 used outside its named matched-error status")
        cors_names = {header_name(field) for field in CORS_HEADERS}
        before_cors = any(header_name(line) in cors_names for line in a[1:])
        after_cors = any(header_name(line) in cors_names for line in b[1:])
        if before_cors or body_cap:
            require_cors(a)
            require_cors(b)
        elif after_cors:
            require_cors(b)
            # Cors appends these fields before egress performs F10's exact length permutation.
            tail = a[-1:] if header_name(a[-1]) == b"date" else []
            a = [*(a[:-1] if tail else a), *CORS_HEADERS, *tail]
            changes.append("C8: matched-error CORS")
    if baseline_exception in ("BASELINE_APS_PARAMETER_ORDER", "BASELINE_MEDIATION_PARAMETER_ORDER"):
        case_id, field = {"BASELINE_APS_PARAMETER_ORDER": ("aps-validation", "slots"),
                          "BASELINE_MEDIATION_PARAMETER_ORDER": ("mediation-validation", "imp")}[baseline_exception]
        require(case.get("id") == case_id and before.status == after.status == 422,
                "parameter-order exception used outside its single case")
        messages = [
            field + ': Validation error: length [{"value": Array [], "min": Number(1)}]',
            field + ': Validation error: length [{"min": Number(1), "value": Array []}]',
        ]
        bodies = [error_body(422, "validation", message) for message in messages]
        require(before.body in bodies and after.body in bodies, "unlisted baseline validation error")
        if before.body != after.body:
            left = Response(left.head, right.body)
            changes.append(baseline_exception + ": error.message")
    if transition in ("body-boundary", "fallback"):
        require(before.status == after.status == case["required_status"], "unexpected boundary/fallback status")
        require(reference_before is not None and reference_after is not None,
                "boundary/fallback requires both small-body reference captures")
        compare_response({"mask": case.get("mask")}, reference_before, before)
        compare_response({"mask": case.get("mask")}, reference_after, after)
    if reference_before is not None and transition in ("body-cap", "head-target") and before.status != after.status:
        compare_response({"mask": case.get("mask")}, reference_before, before)
    if transition == "no-content-type" and before.status != after.status:
        require("F8" in allowed and before.status == 400 and after.status == 200,
                "unexpected Content-Type status transition")
        require(before.body == error_body(400, "bad_request", STREAM_ERROR),
                "unlisted old Content-Type error")
        require(reference_after is not None, "F8 requires the successful reference capture")
        compare_response({"mask": case.get("mask"), **({"uuid_count": case["uuid_count"]} if "uuid_count" in case else {})},
                         reference_after, after)
        a, b = normalize_length(a, before.body), normalize_length(remove_cors(b), after.body)
        a[0] = b[0]
        left, left_ids = Response(left.head, right.body), right_ids
        changes.append("F8: status/body/Content-Length/CORS (successful reference verified)")
    if transition == "head-target" and before.status != after.status:
        require("F5" in allowed and before.status == 200 and after.status == 414,
                "unexpected normalized-target status transition")
        require(reference_before is not None, "F5 requires the baseline root reference capture")
        require(after.body == error_body(414, "uri_too_long", "normalized request target exceeds configured limit"),
                "unlisted normalized-target error")
        a, b = normalize_length(remove_cors(a), before.body), normalize_length(b, after.body)
        old_types = [line for line in a[1:] if header_name(line) == b"content-type"]
        new_types = [line for line in b[1:] if header_name(line) == b"content-type"]
        if old_types or new_types:
            a = remove_field(a, b"content-type", b"text/html; charset=utf-8")
            b = remove_field(b, b"content-type", b"application/json")
        a[0] = b[0]
        left = Response(left.head, right.body)
        changes.append("F5: status/body/Content-Length/Content-Type/CORS")
    if "F4" in allowed and before.status == after.status == 405:
        old_allow = [line for line in a[1:] if header_name(line) == b"allow"]
        new_allow = [line for line in b[1:] if header_name(line) == b"allow"]
        if old_allow != new_allow:
            require(not old_allow, "F4 only permits adding the missing Allow field")
            b = remove_field(b, b"allow", case["allow"].encode())
            changes.append("F4: Allow")
    if "F2" in allowed and left.body != right.body:
        old = error_body(503, "config_out_of_date", MISSING + " — " + PUSH_HINT)
        new = error_body(503, "config_out_of_date", MISSING + "; " + PUSH_HINT)
        require(before.status == after.status == 503 and left.body == old and right.body == new,
                "unlisted config error change")
        left = Response(left.head, new)
        a, b = normalize_length(a, before.body), normalize_length(b, after.body)
        changes.append("F2: error.message/Content-Length")
    if transition == "body-cap" and before.status != after.status:
        number = "C3" if "C3" in allowed else "F7"
        require(number in allowed and before.status == 200 and after.status == 400,
                "unexpected body-cap status transition")
        require(after.body == error_body(400, "bad_request", "request body too large"),
                "unlisted body-cap error")
        if "C8" not in allowed:
            a = remove_cors(a)
        a, b = normalize_length(a, before.body), normalize_length(b, after.body)
        a[0] = b[0]
        left, left_ids = Response(left.head, right.body), []
        changes.append(number + ": status/body/Content-Length")
    if ("C6" in allowed and not before.body and not after.body
            and before.status == after.status and before.status >= 200 and before.status not in (204, 304)):
        old = [line for line in a[1:] if header_name(line) == b"content-length"]
        new = [line for line in b[1:] if header_name(line) == b"content-length"]
        if not old and new:
            b = remove_field(b, b"content-length", b"0")
            changes.append("C6: Content-Length: 0")
    if a != b and "F10" in allowed:
        if canonical_length_order(a) == b:
            a = b
            changes.append("F10: exact Content-Length group permutation")
        elif ("C6" in allowed and not before.body and not after.body
              and before.status == after.status and before.status >= 200
              and before.status not in (204, 304)
              and canonical_length_order(a, empty_insertion=True) == b):
            a = b
            changes.append("C6/F10: exact empty-response header permutation")
    require(left_ids == right_ids, f"UUID occurrence/reference mismatch: {left_ids!r} != {right_ids!r}")
    require(a == b, f"status/ordered headers differ: {a!r} != {b!r}")
    require(left.body == right.body, "body bytes differ (including whitespace/order)")
    return changes


def request_bytes(case):
    body = case.get("body", "").encode("utf-8")
    if "body_pad_to" in case:
        require(case["body_pad_to"] >= len(body), "body padding target too small")
        body += b" " * (case["body_pad_to"] - len(body))
    path = case["path"] + "x" * case.get("target_pad", 0)
    headers = list(case["headers"])
    headers += [f"X-Wire-{i:03d}: x" for i in range(case.get("extra_headers", 0))]
    return path, headers, body


def local_url(value):
    require(value.rstrip("/") == BASE_URL, "use the fixed base URL " + BASE_URL)
    return BASE_URL


def capture(args):
    corpus = json.loads(CORPUS.read_text())
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=False)
    (out / "requests.json").write_bytes(CORPUS.read_bytes())
    manifest = {"scenario": args.scenario, "base_url": args.base_url,
                "corpus_sha256": hashlib.sha256(CORPUS.read_bytes()).hexdigest(), "cases": []}
    for case in corpus["cases"]:
        if case["scenario"] != args.scenario:
            continue
        dest = out / case["id"]
        dest.mkdir()
        path, headers, body = request_bytes(case)
        (dest / "request.body").write_bytes(body)
        (dest / "request.json").write_text(json.dumps({"method": case["method"], "path": path, "headers": headers}, indent=2) + "\n")
        cmd = ["curl", "-q", "--http1.1", "--raw", "-sS", "--path-as-is", "--noproxy", "*",
               "--max-time", "45", "-D", str(dest / "response.headers"), "-o", str(dest / "response.body"),
               "-X", case["method"]]
        for header in headers:
            cmd.extend(["-H", header])
        if body or case["method"] == "POST":
            cmd.extend(["--data-binary", "@" + str(dest / "request.body")])
        result = subprocess.run(cmd + [args.base_url + path], capture_output=True)
        (dest / "curl.stderr").write_bytes(result.stderr)
        (dest / "curl.exit").write_text(str(result.returncode) + "\n")
        manifest["cases"].append(case["id"])
        print(f"{case['id']}: curl={result.returncode}")
    (out / "capture.json").write_text(json.dumps(manifest, indent=2) + "\n")


def read_response(directory, case_id):
    dest = directory / case_id
    require((dest / "curl.exit").read_text().strip() == "0", f"{case_id}: curl failed; see curl.stderr")
    return Response((dest / "response.headers").read_bytes(), (dest / "response.body").read_bytes())


def read_capture(directory):
    manifest = json.loads((directory / "capture.json").read_text())
    corpus_bytes = (directory / "requests.json").read_bytes()
    corpus = json.loads(corpus_bytes)
    require(isinstance(manifest, dict), "invalid capture manifest")
    require(manifest.get("scenario") in ("seeded", "unseeded"), "invalid capture scenario")
    require(manifest.get("base_url") == BASE_URL, "invalid capture base URL")
    require(manifest.get("corpus_sha256") == hashlib.sha256(corpus_bytes).hexdigest(),
            "capture corpus digest does not match requests.json")
    require(isinstance(corpus, dict) and isinstance(corpus.get("cases"), list)
            and all(isinstance(case, dict) for case in corpus["cases"]), "invalid captured corpus")
    definitions = corpus["cases"]
    ids = [case.get("id") for case in definitions]
    require(all(isinstance(case_id, str) and case_id for case_id in ids), "invalid corpus case ID")
    require(len(ids) == len(set(ids)), "duplicate corpus case ID")
    require(all(case.get("scenario") in ("seeded", "unseeded") for case in definitions),
            "invalid corpus scenario")
    expected = [case["id"] for case in definitions if case["scenario"] == manifest["scenario"]]
    require(expected and manifest.get("cases") == expected,
            "capture case list must match the complete, ordered scenario corpus")
    return manifest, corpus_bytes, dict(zip(ids, definitions))


def compare(args):
    before, after = Path(args.before), Path(args.after)
    a, corpus_a, cases = read_capture(before)
    b, corpus_b, _ = read_capture(after)
    require(a == b, "capture scenario, base URL, corpus digest or case list differs")
    require(corpus_a == corpus_b, "captured corpus differs")
    failures = 0
    for case_id in a["cases"]:
        try:
            case = cases[case_id]
            path, headers, body = request_bytes(case)
            for directory in (before, after):
                dest = directory / case_id
                require(json.loads((dest / "request.json").read_text()) == {
                    "method": case["method"], "path": path, "headers": headers,
                }, "recorded request metadata differs from corpus")
                require((dest / "request.body").read_bytes() == body,
                        "recorded request body differs from corpus")
            for filename in ("request.json", "request.body"):
                require((before / case_id / filename).read_bytes() == (after / case_id / filename).read_bytes(), "request bytes differ")
            reference = case.get("reference")
            changes = compare_response(case, read_response(before, case_id), read_response(after, case_id),
                                       read_response(before, reference) if reference else None,
                                       read_response(after, reference) if reference else None)
            print(f"PASS {case_id}" + (": " + "; ".join(changes) if changes else ""))
        except (ContractError, OSError, ValueError) as error:
            failures += 1
            print(f"FAIL {case_id}: {error}", file=sys.stderr)
    require(not failures, f"{failures} wire case(s) failed")


def characterize_raw(args):
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=False)
    cases = {
        "identical-content-length": b"Content-Length: 0\r\nContent-Length: 0\r\n\r\n",
        "content-length-and-chunked": b"Content-Length: 0\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n",
    }
    report = {"scope": "Host-parser characterization only; no universal 400 assertion (F5/F9).", "cases": {}}
    for case_id, tail in cases.items():
        request = b"OPTIONS / HTTP/1.1\r\nHost: 127.0.0.1:8787\r\nConnection: close\r\n" + tail
        (out / (case_id + ".request")).write_bytes(request)
        response, outcome = bytearray(), "closed"
        try:
            with socket.create_connection(("127.0.0.1", 8787), timeout=5) as connection:
                connection.settimeout(5)
                connection.sendall(request)
                while chunk := connection.recv(65536):
                    response.extend(chunk)
        except OSError as error:
            outcome = type(error).__name__ + ": " + str(error)
        (out / (case_id + ".response")).write_bytes(response)
        report["cases"][case_id] = {"outcome": outcome, "response_bytes": len(response),
                                           "status_line": bytes(response).split(b"\r\n", 1)[0].decode("latin1")}
    (out / "characterization.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    capture_parser = commands.add_parser("capture")
    capture_parser.add_argument("--base-url", required=True, type=local_url)
    capture_parser.add_argument("--scenario", required=True, choices=("seeded", "unseeded"))
    capture_parser.add_argument("--out", required=True)
    compare_parser = commands.add_parser("compare")
    compare_parser.add_argument("--before", required=True)
    compare_parser.add_argument("--after", required=True)
    raw_parser = commands.add_parser("characterize-raw")
    raw_parser.add_argument("--base-url", required=True, type=local_url)
    raw_parser.add_argument("--out", required=True)
    args = parser.parse_args()
    try:
        {"capture": capture, "compare": compare, "characterize-raw": characterize_raw}[args.command](args)
    except (ContractError, OSError, ValueError) as error:
        parser.exit(1, str(error) + "\n")


if __name__ == "__main__":
    main()
