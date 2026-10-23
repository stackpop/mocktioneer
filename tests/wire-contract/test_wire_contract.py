"""Offline regression tests for the byte-preserving wire comparator."""

import copy
import hashlib
import importlib.util
import inspect
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


HERE = Path(__file__).resolve().parent
UUID_A = "019a8a01000070008000000000000001"
UUID_B = "019a8a02000070009000000000000002"
UUID_C = "019a8a0300007000a000000000000003"
MTKID = "13170e94273c9f84b05e8937f7d4b70b"
CORS_HEADERS = [b"access-control-allow-origin: *",
                b"access-control-allow-methods: GET, POST, OPTIONS",
                b"access-control-allow-headers: *, content-type"]


class WireContractTests(unittest.TestCase):
    def setUp(self):
        path = HERE / "wire_contract.py"
        self.assertTrue(path.exists(), "wire capture/comparator has not been implemented")
        spec = importlib.util.spec_from_file_location("wire_contract", path)
        self.wire = importlib.util.module_from_spec(spec)
        sys.modules[spec.name] = self.wire
        spec.loader.exec_module(self.wire)

    def response(self, body=b"body", status=200, headers=()):
        if not isinstance(body, bytes):
            body = json.dumps(body, separators=(",", ":")).encode()
        reason = {200: "OK", 204: "No Content", 400: "Bad Request", 404: "Not Found", 405: "Method Not Allowed",
                  414: "URI Too Long", 422: "Unprocessable Entity", 503: "Service Unavailable"}[status]
        lines = [f"HTTP/1.1 {status} {reason}".encode(), *headers]
        return self.wire.Response(b"\r\n".join(lines) + b"\r\n\r\n", body)

    def auction(self, uuid=UUID_A, reference=None):
        preview = {"response": {"id": "wire-request", "seatbid": [
            {"bid": [{"id": reference or uuid, "price": 0.35}]}]}}
        adm = "<!-- MOCKTIONEER_METADATA\n" + json.dumps(preview, indent=2) + "\n-->"
        return {"id": "wire-request", "seatbid": [{"bid": [
            {"id": uuid, "impid": "wire-imp", "price": 0.35, "adm": adm}]}]}

    def compare(self, case, before, after):
        return self.wire.compare_response(case, before, after)

    def capture_pair(self, root, scenario="seeded"):
        corpus = {"cases": [
            {"id": "one", "scenario": "seeded", "method": "GET", "path": "/one", "headers": []},
            {"id": "two", "scenario": "seeded", "method": "POST", "path": "/two", "headers": [],
             "body": "{}", "body_pad_to": 4, "target_pad": 2, "extra_headers": 1},
            {"id": "unseeded", "scenario": "unseeded", "method": "GET", "path": "/unseeded", "headers": []},
        ]}
        corpus_bytes = json.dumps(corpus).encode()
        requests = {
            "one": ({"method": "GET", "path": "/one", "headers": []}, b""),
            "two": ({"method": "POST", "path": "/twoxx", "headers": ["X-Wire-000: x"]}, b"{}  "),
            "unseeded": ({"method": "GET", "path": "/unseeded", "headers": []}, b""),
        }
        ids = [case["id"] for case in corpus["cases"] if case["scenario"] == scenario]
        directories = [Path(root) / name for name in ("before", "after")]
        for directory in directories:
            directory.mkdir()
            (directory / "requests.json").write_bytes(corpus_bytes)
            (directory / "capture.json").write_text(json.dumps({
                "scenario": scenario, "base_url": self.wire.BASE_URL,
                "corpus_sha256": hashlib.sha256(corpus_bytes).hexdigest(), "cases": ids,
            }))
            for case_id in ids:
                dest = directory / case_id
                dest.mkdir()
                request, body = requests[case_id]
                (dest / "request.json").write_text(json.dumps(request))
                (dest / "request.body").write_bytes(body)
                (dest / "curl.exit").write_text("0\n")
                (dest / "response.headers").write_bytes(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n")
                (dest / "response.body").write_bytes(b"ok")
        return directories

    def compare_captures(self, directories):
        return subprocess.run([sys.executable, str(HERE / "wire_contract.py"), "compare",
                               "--before", str(directories[0]), "--after", str(directories[1])],
                              capture_output=True, text=True)

    def change_manifest(self, directories, **changes):
        for directory in directories:
            path = directory / "capture.json"
            manifest = json.loads(path.read_text())
            manifest.update(changes)
            path.write_text(json.dumps(manifest))

    def test_cli_compare_accepts_complete_scenarios(self):
        for scenario, count in [("seeded", 2), ("unseeded", 1)]:
            with self.subTest(scenario=scenario), tempfile.TemporaryDirectory() as root:
                result = self.compare_captures(self.capture_pair(root, scenario))
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout.count("PASS "), count)

    def test_cli_compare_rejects_invalid_case_lists(self):
        for ids in [[], ["one"], ["one", "one", "two"], ["two", "one"],
                    ["one", "two", "extra"], ["unseeded"]]:
            with self.subTest(ids=ids), tempfile.TemporaryDirectory() as root:
                directories = self.capture_pair(root)
                self.change_manifest(directories, cases=ids)
                result = self.compare_captures(directories)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("case list", result.stderr)
                self.assertNotIn("PASS ", result.stdout)

    def test_cli_compare_rejects_invalid_manifest_metadata(self):
        for changes, error in [({"corpus_sha256": "0" * 64}, "digest"),
                               ({"scenario": "unknown"}, "scenario"),
                               ({"base_url": "http://127.0.0.1:9999"}, "base URL")]:
            with self.subTest(changes=changes), tempfile.TemporaryDirectory() as root:
                directories = self.capture_pair(root)
                self.change_manifest(directories, **changes)
                result = self.compare_captures(directories)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(error, result.stderr)

    def test_cli_compare_rejects_duplicate_corpus_ids(self):
        with tempfile.TemporaryDirectory() as root:
            directories = self.capture_pair(root)
            for directory in directories:
                path = directory / "requests.json"
                corpus = json.loads(path.read_text())
                corpus["cases"].append(corpus["cases"][0])
                path.write_text(json.dumps(corpus))
            self.change_manifest(directories, corpus_sha256=hashlib.sha256(
                (directories[0] / "requests.json").read_bytes()).hexdigest())
            result = self.compare_captures(directories)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("duplicate", result.stderr)

    def test_cli_compare_rejects_empty_scenario(self):
        with tempfile.TemporaryDirectory() as root:
            directories = self.capture_pair(root)
            for directory in directories:
                (directory / "requests.json").write_text('{"cases": []}')
            self.change_manifest(directories, cases=[], corpus_sha256=hashlib.sha256(
                (directories[0] / "requests.json").read_bytes()).hexdigest())
            result = self.compare_captures(directories)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("case list", result.stderr)

    def test_cli_compare_checks_requests_against_corpus(self):
        for field, value in [("method", "GET"), ("path", "/two"), ("headers", []), ("body", b"{}")]:
            with self.subTest(field=field), tempfile.TemporaryDirectory() as root:
                directories = self.capture_pair(root)
                for directory in directories:
                    if field == "body":
                        (directory / "two/request.body").write_bytes(value)
                    else:
                        path = directory / "two/request.json"
                        request = json.loads(path.read_text())
                        request[field] = value
                        path.write_text(json.dumps(request))
                result = self.compare_captures(directories)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("request", result.stderr)
                self.assertIn("corpus", result.stderr)

    def test_cli_compare_rejects_missing_response_files(self):
        with tempfile.TemporaryDirectory() as root:
            directories = self.capture_pair(root)
            (directories[1] / "two/response.body").unlink()
            result = self.compare_captures(directories)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("FAIL two", result.stderr)

    def test_identical_bytes_pass_and_dates_are_masked(self):
        a = self.response(headers=[b"Date: Tue, 20 Oct 2026 01:00:00 GMT"])
        b = self.response(headers=[b"Date: Tue, 20 Oct 2026 02:00:00 GMT"])
        self.assertEqual(self.compare({}, a, b), [])

    def test_header_values_order_and_duplicate_order_are_protected(self):
        a = self.response(headers=[b"Set-Cookie: a=1", b"Set-Cookie: b=2", b"X-Test: ok"])
        variants = [
            [b"Set-Cookie: a=1", b"Set-Cookie: b=2", b"X-Test: changed"],
            [b"Set-Cookie: b=2", b"Set-Cookie: a=1", b"X-Test: ok"],
            [b"X-Test: ok", b"Set-Cookie: a=1", b"Set-Cookie: b=2"],
        ]
        for headers in variants:
            with self.subTest(headers=headers), self.assertRaises(self.wire.ContractError):
                self.compare({}, a, self.response(headers=headers))

    def test_generated_bid_id_and_escaped_preview_use_one_token(self):
        case = {"mask": "auction"}
        self.assertEqual(self.compare(case, self.response(self.auction()),
                                      self.response(self.auction(UUID_B))), [])

    def test_f10_allows_only_exact_content_length_group_swap(self):
        old = [b"Content-Length: 4", b"Content-Type: text/plain", b"Set-Cookie: a=1", b"Set-Cookie: b=2"]
        new = [b"Set-Cookie: a=1", b"Set-Cookie: b=2", b"Content-Type: text/plain", b"Content-Length: 4"]
        case = {"differences": ["F10"]}
        self.assertEqual(self.compare(case, self.response(headers=old), self.response(headers=new)),
                         ["F10: exact Content-Length group permutation"])
        for changed in [new[::-1], [new[1], new[0], *new[2:]],
                        [b"Set-Cookie: a=changed", *new[1:]], [*new, b"X-Extra: yes"]]:
            with self.subTest(changed=changed), self.assertRaises(self.wire.ContractError):
                self.compare(case, self.response(headers=old), self.response(headers=changed))
        with self.assertRaises(self.wire.ContractError):
            self.compare({}, self.response(headers=old), self.response(headers=new))

    def test_c6_empty_length_insertion_has_one_exact_f10_permutation(self):
        old = [b"Location: https://example.test/", b"Set-Cookie: a=1", b"Set-Cookie: b=2", b"Content-Length: 0"]
        new = [b"Set-Cookie: a=1", b"Set-Cookie: b=2", b"Location: https://example.test/", b"Content-Length: 0"]
        case = {"differences": ["C6", "F10"]}
        self.assertEqual(self.compare(case, self.response(b"", headers=old), self.response(b"", headers=new)),
                         ["C6/F10: exact empty-response header permutation"])
        for changed in [[new[1], new[0], *new[2:]], [*new[:-1], b"Content-Length: 1"],
                        [b"Set-Cookie: changed", *new[1:]]]:
            with self.subTest(changed=changed), self.assertRaises(self.wire.ContractError):
                self.compare(case, self.response(b"", headers=old), self.response(b"", headers=changed))
        with self.assertRaises(self.wire.ContractError):
            self.compare({"differences": ["F10"]}, self.response(b"", headers=old), self.response(b"", headers=new))
        with self.assertRaises(self.wire.ContractError):
            self.compare({"differences": ["F10"]}, self.response(b"", headers=[b"X-Test: same"]),
                         self.response(b"", headers=[b"X-Test: same", b"Content-Length: 0"]))

    def test_body_price_caller_ids_whitespace_and_order_are_protected(self):
        base = self.auction()
        variants = []
        price = copy.deepcopy(base)
        price["seatbid"][0]["bid"][0]["price"] = 0.36
        variants.append(price)
        caller_id = copy.deepcopy(base)
        caller_id["id"] = UUID_B
        variants.append(caller_id)
        variants.extend([json.dumps(base, indent=2).encode(),
                         json.dumps(dict(reversed(list(base.items()))), separators=(",", ":")).encode()])
        for body in variants:
            with self.subTest(body=body), self.assertRaises(self.wire.ContractError):
                self.compare({"mask": "auction"}, self.response(base), self.response(body))

    def test_mtkid_is_never_masked(self):
        a = self.response(headers=[f"Set-Cookie: mtkid={MTKID}".encode()])
        b = self.response(headers=[f"Set-Cookie: mtkid={UUID_B}".encode()])
        with self.assertRaises(self.wire.ContractError):
            self.compare({}, a, b)

    def test_caller_ids_and_signed_fields_inside_adm_are_protected(self):
        base = self.auction()
        base["seatbid"][0]["bid"][0]["adm"] += '\n{"request":{"id":"' + UUID_A + '","signature":"fixed"}}'
        changed = copy.deepcopy(base)
        changed["seatbid"][0]["bid"][0]["adm"] = changed["seatbid"][0]["bid"][0]["adm"].replace('"signature":"fixed"', '"signature":"other"')
        with self.assertRaises(self.wire.ContractError):
            self.compare({"mask": "auction"}, self.response(base), self.response(changed))

    def test_uuid_reference_mismatch_fails(self):
        with self.assertRaises(self.wire.ContractError):
            self.compare({"mask": "auction"}, self.response(self.auction()),
                         self.response(self.auction(UUID_B, UUID_C)))

    def test_uuid_occurrence_count_mismatch_fails(self):
        changed = self.auction(UUID_B)
        changed["seatbid"][0]["bid"][0]["adm"] = "<!-- MOCKTIONEER_METADATA\n{}\n-->"
        with self.assertRaises(self.wire.ContractError):
            self.compare({"mask": "auction"}, self.response(self.auction()), self.response(changed))

    def test_uuid_version_and_variant_are_validated_even_in_self_compare(self):
        for bad in [UUID_A[:12] + "4" + UUID_A[13:], UUID_A[:16] + "0" + UUID_A[17:]]:
            with self.subTest(bad=bad), self.assertRaises(self.wire.ContractError):
                body = self.response(self.auction(bad))
                self.compare({"mask": "auction"}, body, body)

    def test_static_pixel_repeated_reference_equality(self):
        def creative(a, b):
            return (f'<img src="//127.0.0.1:8787/pixel?pid={a}" data-static-pid="{b}">'
                    f'<script>var jsPid = "{UUID_C}";</script>').encode()
        self.assertEqual(self.compare({"mask": "creative"}, self.response(creative(UUID_A, UUID_A)),
                                      self.response(creative(UUID_B, UUID_B))), [])
        with self.assertRaises(self.wire.ContractError):
            self.compare({"mask": "creative"}, self.response(creative(UUID_A, UUID_A)),
                         self.response(creative(UUID_B, UUID_C)))

    def test_aps_generated_ids_are_masked_but_encoded_price_is_protected(self):
        def aps(uuid, price="MC4zNQ=="):
            return {"contextual": {"slots": [{"slotID": "wire-slot", "amzniid": uuid,
                     "crid": uuid + "-mocktioneer", "amznp": price}]}}
        self.assertEqual(self.compare({"mask": "aps"}, self.response(aps(UUID_A)),
                                      self.response(aps(UUID_B))), [])
        with self.assertRaises(self.wire.ContractError):
            self.compare({"mask": "aps"}, self.response(aps(UUID_A)), self.response(aps(UUID_B, "MQ==")))

    def test_allowed_405_adds_only_exact_sorted_allow_header(self):
        body = {"error": {"kind": "method_not_allowed", "status": 405, "message": "same"}}
        case = {"differences": ["F4"], "allow": "GET, OPTIONS"}
        a = self.response(body, 405)
        b = self.response(body, 405, [b"Allow: GET, OPTIONS"])
        self.assertEqual(self.compare(case, a, b), ["F4: Allow"])
        with self.assertRaises(self.wire.ContractError):
            self.compare(case, a, self.response(body, 405, [b"Allow: OPTIONS, GET"]))
        with self.assertRaises(self.wire.ContractError):
            self.compare(case, a, self.response(b"unlisted error", 405, [b"Allow: GET, OPTIONS"]))

    def test_allowed_cap_status_does_not_allow_arbitrary_error_body(self):
        case = {"differences": ["C3"], "transition": "body-cap"}
        before = self.response(self.auction())
        error = {"error": {"kind": "bad_request", "message": "request body too large", "status": 400}}
        self.assertIn("C3: status/body/Content-Length", self.compare(case, before, self.response(error, 400)))
        error["error"]["message"] = "unlisted failure"
        with self.assertRaises(self.wire.ContractError):
            self.compare(case, before, self.response(error, 400))

    def test_c8_adds_exact_cors_before_the_f10_length_permutation(self):
        body = self.wire.error_body(400, "bad_request", "invalid JSON")
        length = b"content-length: " + str(len(body)).encode()
        case = {"id": "auction-malformed", "differences": ["C8", "F10"]}
        date = b"date: Wed, 21 Oct 2026 18:21:58 GMT"
        before = self.response(body, 400, [length, b"content-type: application/json", date])
        after_headers = [CORS_HEADERS[2], b"content-type: application/json",
                         *CORS_HEADERS[:2], length, date]
        after = self.response(body, 400, after_headers)
        self.assertEqual(self.compare(case, before, before), [])
        self.assertEqual(self.compare(case, after, after), [])
        self.assertEqual(self.compare(case, before, after),
                         ["C8: matched-error CORS", "F10: exact Content-Length group permutation"])
        for headers in [after_headers[1:], [*after_headers, CORS_HEADERS[0]],
                        [line.replace(b"GET, POST, OPTIONS", b"GET, POST") for line in after_headers],
                        [after_headers[1], after_headers[0], *after_headers[2:]],
                        [after_headers[0], *after_headers[2:4], after_headers[1], length],
                        [*after_headers, b"x-unrelated: added"]]:
            with self.subTest(headers=headers), self.assertRaises(self.wire.ContractError):
                self.compare(case, before, self.response(body, 400, headers))
        with self.assertRaises(self.wire.ContractError):
            self.compare(case, after, before)

    def test_c8_preserves_config_retry_and_all_other_header_order(self):
        old = self.wire.error_body(503, "config_out_of_date", self.wire.MISSING + " — " + self.wire.PUSH_HINT)
        new = self.wire.error_body(503, "config_out_of_date", self.wire.MISSING + "; " + self.wire.PUSH_HINT)
        case = {"id": "unseeded-auction-missing-config", "differences": ["F2", "C8", "F10"]}
        before = self.response(old, 503, [b"content-length: " + str(len(old)).encode(),
                                         b"content-type: application/json", b"retry-after: 60"])
        headers = [CORS_HEADERS[2], b"content-type: application/json", b"retry-after: 60",
                   *CORS_HEADERS[:2], b"content-length: " + str(len(new)).encode()]
        self.assertIn("C8: matched-error CORS", self.compare(case, before, self.response(new, 503, headers)))
        for changed in [[headers[0], headers[2], headers[1], *headers[3:]],
                        [line for line in headers if not line.startswith(b"retry-after:")],
                        [line.replace(b"retry-after: 60", b"retry-after: 10") for line in headers]]:
            with self.subTest(changed=changed), self.assertRaises(self.wire.ContractError):
                self.compare(case, before, self.response(new, 503, changed))

    def test_c8_is_limited_to_named_matched_error_statuses(self):
        before = self.response(b"same", 400)
        after = self.response(b"same", 400, CORS_HEADERS)
        for case in [{"id": "head-target-9kb", "differences": ["C8"]},
                     {"id": "unknown-path", "differences": ["C8"]},
                     {"id": "method-not-allowed-root", "differences": ["C8"]},
                     {"id": "auction-validation", "differences": ["C8"]},
                     {"id": "auction-malformed", "differences": []}]:
            with self.subTest(case=case), self.assertRaises(self.wire.ContractError):
                self.compare(case, before, after)

    def test_c8_body_cap_transition_preserves_cors_and_the_success_reference(self):
        case = {"id": "auction-over-1mib", "differences": ["C3", "C8", "F10"],
                "transition": "body-cap", "mask": "auction"}
        before_body = self.auction()
        before_length = len(json.dumps(before_body, separators=(",", ":")).encode())
        before = self.response(before_body, 200, [b"content-length: " + str(before_length).encode(),
                                                b"content-type: application/json", *CORS_HEADERS])
        body = self.wire.error_body(400, "bad_request", "request body too large")
        headers = [CORS_HEADERS[2], b"content-type: application/json", *CORS_HEADERS[:2],
                   b"content-length: " + str(len(body)).encode()]
        self.assertEqual(self.compare(case, before, before), [])
        self.assertIn("C3: status/body/Content-Length", self.wire.compare_response(
            case, before, self.response(body, 400, headers), reference_before=before))
        for changed in [headers[1:], [*headers, CORS_HEADERS[1]],
                        [line for line in headers if not line.startswith(b"access-control-")]]:
            with self.subTest(changed=changed), self.assertRaises(self.wire.ContractError):
                self.wire.compare_response(case, before, self.response(body, 400, changed), reference_before=before)
        with self.assertRaises(self.wire.ContractError):
            self.wire.compare_response(case, before, self.response(body, 400, headers),
                                       reference_before=self.response(self.auction(UUID_B), 200))

    def test_historical_body_cap_metadata_retains_its_cors_removal_contract(self):
        case = {"id": "auction-over-1mib", "differences": ["C3", "F10"],
                "transition": "body-cap", "mask": "auction"}
        success = self.auction()
        length = len(json.dumps(success, separators=(",", ":")).encode())
        date = b"date: Wed, 21 Oct 2026 18:21:58 GMT"
        before = self.response(success, 200, [b"content-length: " + str(length).encode(),
                                            b"content-type: application/json", *CORS_HEADERS, date])
        body = self.wire.error_body(400, "bad_request", "request body too large")
        after = self.response(body, 400, [b"content-type: application/json",
                                        b"content-length: 80", date])
        self.assertEqual(self.wire.compare_response(case, before, after, reference_before=before),
                         ["C3: status/body/Content-Length", "F10: exact Content-Length group permutation"])
        current_case = {**case, "differences": ["C3", "C8", "F10"]}
        with self.assertRaises(self.wire.ContractError):
            self.wire.compare_response(current_case, before, after, reference_before=before)

    def test_corpus_is_explicit_and_has_separate_startup_scenarios(self):
        corpus = json.loads((HERE / "requests.json").read_text())
        cases = corpus["cases"]
        self.assertEqual(len({case["id"] for case in cases}), len(cases))
        self.assertEqual({case["scenario"] for case in cases}, {"seeded", "unseeded"})
        self.assertEqual(set(corpus["accepted_differences"]),
                         {f"F{i}" for i in range(1, 14)} | {f"C{i}" for i in range(1, 9)})
        self.assertEqual(len(cases), 87)
        self.assertEqual(sum(case["scenario"] == "seeded" for case in cases), 85)
        self.assertEqual({case["id"] for case in cases if "C8" in case["differences"]}, {
            "image-nonstandard", "creative-nonstandard", "pixel-missing", "aps-validation", "aps-malformed",
            "auction-malformed", "auction-validation", "sync-start-missing", "mediation-malformed",
            "mediation-validation", "auction-over-1mib", "aps-over-1mib", "mediation-over-8mib",
            "unseeded-auction-missing-config", "unseeded-aps-missing-config",
        })
        self.assertTrue(all("method" in case and "path" in case and "headers" in case for case in cases))
        for scenario in ("seeded", "unseeded"):
            self.assertTrue(any(case["scenario"] == scenario for case in cases))

    def test_missing_config_permits_only_exact_message_change(self):
        old = self.wire.error_body(503, "config_out_of_date", self.wire.MISSING + " — " + self.wire.PUSH_HINT)
        new = self.wire.error_body(503, "config_out_of_date", self.wire.MISSING + "; " + self.wire.PUSH_HINT)
        headers = [b"Retry-After: 60"]
        self.assertTrue(self.compare({"differences": ["F2"]}, self.response(old, 503, headers),
                                     self.response(new, 503, headers)))
        with self.assertRaises(self.wire.ContractError):
            self.compare({"differences": ["F2"]}, self.response(old, 503, headers),
                         self.response(new.replace(b"config_out_of_date", b"service_unavailable"), 503, headers))

    def test_no_content_type_success_must_match_successful_reference(self):
        self.assertIn("reference_after", inspect.signature(self.wire.compare_response).parameters,
                      "successful response reference comparison is not implemented")
        case = {"mask": "auction", "transition": "no-content-type", "differences": ["F8"]}
        before = self.response(self.wire.error_body(400, "bad_request", self.wire.STREAM_ERROR), 400)
        reference = self.response(self.auction(UUID_A))
        after = self.response(self.auction(UUID_B))
        self.assertTrue(self.wire.compare_response(case, before, after, reference_after=reference))
        self.assertTrue(self.wire.compare_response(case, before, after,
                                                  reference_before=reference, reference_after=reference))
        changed = self.auction(UUID_B)
        changed["seatbid"][0]["bid"][0]["price"] = 99
        with self.assertRaises(self.wire.ContractError):
            self.wire.compare_response(case, before, self.response(changed), reference_after=reference)

    def test_head_target_permits_only_exact_normalized_error(self):
        self.assertIn("reference_before", inspect.signature(self.wire.compare_response).parameters,
                      "normalized-head transition comparison is not implemented")
        case = {"transition": "head-target", "differences": ["F5", "F9"]}
        before = self.response(b"root")
        after = self.response(self.wire.error_body(414, "uri_too_long", "normalized request target exceeds configured limit"), 414)
        self.assertTrue(self.wire.compare_response(case, before, after, reference_before=before))
        with self.assertRaises(self.wire.ContractError):
            self.wire.compare_response(case, before, self.response(b"wrong", 414), reference_before=before)

    def test_deterministic_generators_preserve_explicit_valid_json(self):
        case = {"path": "/?wire=", "headers": ["Host: 127.0.0.1:8787"],
                "body": '{"id":"fixed"}', "body_pad_to": 1153434, "target_pad": 9216,
                "extra_headers": 101}
        path, headers, body = self.wire.request_bytes(case)
        self.assertEqual(len(body), 1153434)
        self.assertEqual(json.loads(body), {"id": "fixed"})
        self.assertEqual(len(path), len(case["path"]) + 9216)
        self.assertEqual(headers[-1], "X-Wire-100: x")

    def test_baseline_aps_parameter_order_has_a_finite_exact_allowlist(self):
        case = {"id": "aps-validation", "baseline_exception": "BASELINE_APS_PARAMETER_ORDER"}
        messages = [
            'slots: Validation error: length [{"value": Array [], "min": Number(1)}]',
            'slots: Validation error: length [{"min": Number(1), "value": Array []}]',
        ]
        before = self.response(self.wire.error_body(422, "validation", messages[0]), 422)
        after = self.response(self.wire.error_body(422, "validation", messages[1]), 422)
        self.assertIn("BASELINE_APS_PARAMETER_ORDER: error.message", self.compare(case, before, after))
        for altered in [messages[1].replace("Number(1)", "Number(2)"),
                        messages[1].replace("Array []", "Array [1]"),
                        messages[1].replace("slots:", "other:"), messages[1] + " changed"]:
            with self.subTest(altered=altered), self.assertRaises(self.wire.ContractError):
                self.compare(case, before, self.response(self.wire.error_body(422, "validation", altered), 422))
        with self.assertRaises(self.wire.ContractError):
            self.compare(case, before, self.response(self.wire.error_body(422, "bad_request", messages[1]), 422))

    def test_empty_response_exception_does_not_add_forbidden_204_length(self):
        before = self.response(b"", 204)
        after = self.response(b"", 204, [b"Content-Length: 0"])
        with self.assertRaises(self.wire.ContractError):
            self.compare({"differences": ["C6", "F10"]}, before, after)

    def test_boundary_and_fallback_cases_require_expected_status_and_reference(self):
        for transition, status in [("body-boundary", 200), ("fallback", 404), ("fallback", 405)]:
            case = {"transition": transition, "required_status": status}
            reference = self.response(b"fixed response", status)
            self.assertEqual(self.wire.compare_response(case, reference, reference,
                             reference_before=reference, reference_after=reference), [])
            wrong = self.response(b"fixed response", 400)
            with self.subTest(transition=transition, status=status), self.assertRaises(self.wire.ContractError):
                self.wire.compare_response(case, wrong, wrong, reference_before=reference, reference_after=reference)
            changed = self.response(b"mutated response", status)
            with self.assertRaises(self.wire.ContractError):
                self.wire.compare_response(case, reference, changed,
                                           reference_before=reference, reference_after=reference)
            with self.assertRaises(self.wire.ContractError):
                self.wire.compare_response(case, changed, changed,
                                           reference_before=reference, reference_after=reference)

    def test_mediation_parameter_order_has_a_separate_exact_allowlist(self):
        case = {"id": "mediation-validation", "baseline_exception": "BASELINE_MEDIATION_PARAMETER_ORDER"}
        messages = [
            'imp: Validation error: length [{"value": Array [], "min": Number(1)}]',
            'imp: Validation error: length [{"min": Number(1), "value": Array []}]',
        ]
        before = self.response(self.wire.error_body(422, "validation", messages[0]), 422)
        after = self.response(self.wire.error_body(422, "validation", messages[1]), 422)
        self.assertIn("BASELINE_MEDIATION_PARAMETER_ORDER: error.message", self.compare(case, before, after))
        for altered in [messages[1].replace("Number(1)", "Number(2)"),
                        messages[1].replace("Array []", "Array [1]"),
                        messages[1].replace("imp:", "other:"), messages[1] + " changed"]:
            with self.subTest(altered=altered), self.assertRaises(self.wire.ContractError):
                self.compare(case, before, self.response(self.wire.error_body(422, "validation", altered), 422))
        with self.assertRaises(self.wire.ContractError):
            self.compare(case, before, self.response(self.wire.error_body(422, "bad_request", messages[1]), 422))
        with self.assertRaises(self.wire.ContractError):
            self.compare(case, before, self.response(after.body, 400))
        with self.assertRaises(self.wire.ContractError):
            self.compare(case, before, self.response(after.body, 422, [b"X-Mutated: yes"]))

    def test_corpus_has_exact_boundaries_fallbacks_and_mediation_errors(self):
        cases = {case["id"]: case for case in json.loads((HERE / "requests.json").read_text())["cases"]}
        sizes = {"auction-exact-1mib": 1024 ** 2, "aps-exact-1mib": 1024 ** 2,
                 "mediation-exact-8mib": 8 * 1024 ** 2, "mediation-between-1-and-8mib": 2 * 1024 ** 2,
                 "fallback-large-404": 9 * 1024 ** 2, "fallback-large-405": 9 * 1024 ** 2}
        for name, size in sizes.items():
            with self.subTest(name=name):
                self.assertTrue(name in cases, name)
                path, headers, body = self.wire.request_bytes(cases[name])
                self.assertEqual(len(body), size)
                self.assertEqual(json.loads(body), json.loads(cases[name]["body"]))
        self.assertIn("mediation-malformed", cases)
        self.assertIn("mediation-validation", cases)


if __name__ == "__main__":
    unittest.main()
