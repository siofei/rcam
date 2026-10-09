import copy
import unittest
from frame_trace_report import boundary, ns, summarize


def fixture():
    binding = {"update_id": 1, "version_id": 1, "input_batch_id": 1, "rendered_source_update_id": 0}
    return [
        {"kind": "header", "schema_version": 2, "writer_join_required": True, "run_id": "run",
         "commit": "c" * 40, "source_manifest_sha256": "a" * 64, "binary_sha256": "b" * 64, "config_sha256": "d" * 64,
         "clock": {"clock_id": "mach_absolute_time_ns", "origin_raw_ticks": "100", "origin_source_ns": "100",
                   "conversion": {"numer": 1, "denom": 1}},
         "windows": [{"id": "active", "start_ns": "100", "end_ns": "200"}]},
        {"kind": "identity_ready", "source_ns": "50"},
        {"record_seq": 1, "kind": "metadata", "metadata": {"kind": "version", "version_id": 1}},
        {"record_seq": 2, "kind": "update_start", "source_ns": "140", "binding": binding,
         "previous": {"update_id": 0, "source_ns": "0", "version_id": 1, "input_batch_id": 0}},
        {"record_seq": 3, "kind": "update_end", "source_ns": "150", "binding": binding, "end_version_id": 1},
        {"record_seq": 4, "kind": "span", "stage": "update", "source_ns": "150", "binding": binding,
         "start_ns": "140", "end_ns": "150", "wall_duration_ns": "10", "outcome": "normal_end"},
        {"kind": "footer", "finalized": True, "observation_complete": True,
         "counters": {"attempted": 4, "drop_by_stage": [0] * 8}},
    ]


def calibration_fixture():
    header = fixture()[0]
    actual = {"run_id": "run", "clock_id": "mach_absolute_time_ns", "windows": [
        {"id": "active", "start_ns": "120", "end_ns": "180",
         "first_dispatch_raw_ticks": "120", "last_dispatch_raw_ticks": "180"}]}
    calibration = {"schema_version": 1, "method": "shared_mach_absolute_timebase", "run_id": "run",
                   "trace_identity": {key: header[key] for key in
                                      ("commit", "source_manifest_sha256", "binary_sha256", "config_sha256")},
                   "source_clock": {"clock_id": "mach_absolute_time_ns", "origin_raw_ticks": "100",
                                    "timebase": {"numer": 1, "denom": 1}},
                   "external_clock": {"clock_id": "mach_absolute_time_ns", "origin_raw_ticks": "70",
                                      "timebase": {"numer": 1, "denom": 1}},
                   "provenance": {"source": "synthetic-parser-test-only", "producer_pid": 123,
                                  "producer_sha256": "e" * 64, "evidence_sha256": "f" * 64},
                   "sync_samples": [{"before_raw_ticks": str(t - 1), "external_raw_ticks": str(t),
                                     "after_raw_ticks": str(t + 1), "external_ns": str(t)} for t in (80, 220)],
                   "max_error_ns": "2"}
    return actual, calibration


class TraceReportTests(unittest.TestCase):
    def test_exact_endpoint_categories_retain_large_boundary_interval(self):
        self.assertEqual(boundary(0, 140, 100, 200), "cross_start")
        self.assertEqual(boundary(100, 199, 100, 200), "inside")
        self.assertEqual(boundary(100, 200, 100, 200), "cross_end")
        self.assertEqual(boundary(0, 200, 100, 200), "cross_both")
        self.assertEqual(boundary(200, 201, 100, 200), "after")
        self.assertEqual(boundary(0, 100, 100, 200), "cross_start")
        report = summarize(fixture(), True)
        interval = report["planned_windows"][0]["intervals"][0]
        self.assertEqual(interval["wall_duration_ns"], "140")
        self.assertEqual(interval["classification"], "cross_start")
        self.assertEqual(report["planned_windows"][0]["inside_interval_diagnostics"]["count"], 0)

    def test_missing_metadata_and_sequence_never_make_a_pass(self):
        rows = fixture()
        del rows[2]
        report = summarize(rows, True)
        self.assertIn("dangling_version_metadata", report["issues"])
        self.assertIn("record_sequence_gaps", report["issues"])
        self.assertEqual(report["observation_status"], "INCOMPLETE")
        self.assertEqual(report["performance_status"], "OPEN")

    def test_complete_within_scope_is_not_performance_pass_and_no_first_interval_invented(self):
        rows = fixture()
        rows[3]["previous"] = None
        rows[0]["windows"] = []
        report = summarize(rows, True)
        self.assertEqual(report["observation_status"], "COMPLETE_WITHIN_DECLARED_SCOPE")
        self.assertEqual(report["performance_status"], "OPEN")
        self.assertEqual(report["planned_windows"], [])
        self.assertIsNone(report["updates"][0]["start"]["previous"])

    def test_footer_without_supervisor_ack_or_timeout_is_incomplete(self):
        report = summarize(fixture())
        self.assertIn("matching_supervisor_flush_ack_not_verified", report["issues"])
        rows = fixture()
        rows[-1]["counters"]["supervisor_timeout"] = True
        self.assertEqual(summarize(rows, True)["observation_status"], "INCOMPLETE")

    def test_late_footer_does_not_hide_unfinished_or_disagreeing_end(self):
        rows = fixture()
        rows[4]["source_ns"] = "149"
        self.assertIn("noncanonical_update_end", summarize(rows, True)["issues"])
        del rows[4]
        self.assertIn("unfinished_update", summarize(rows, True)["issues"])

    def test_clock_and_actual_dispatch_are_distinct_from_planned_window(self):
        actual, calibration = calibration_fixture()
        report = summarize(fixture(), True, actual, calibration)
        self.assertEqual(report["planned_windows"][0]["window"]["start_ns"], "100")
        self.assertEqual(report["actual_dispatch_windows"][0]["window"]["start_ns"], "120")
        self.assertEqual(report["clock_calibration"]["max_error_ns"], "2")
        self.assertEqual(report["activity_window_association_status"], "CALIBRATED_WITH_DECLARED_ERROR_BOUND")

    def test_boolean_only_or_missing_calibration_never_associates_activity(self):
        actual, _ = calibration_fixture()
        actual["clock_mapping_verified"] = True
        report = summarize(fixture(), True, actual)
        self.assertEqual(report["observation_status"], "INCOMPLETE")
        self.assertEqual(report["activity_window_association_status"], "INCOMPLETE")
        self.assertIsNone(report["actual_dispatch_windows"])
        self.assertEqual(report["internal_update_wall_diagnostics"]["count"], 1)
        self.assertEqual(report["unassociated_external_windows"], actual["windows"])
        self.assertIn("external_activity_clock_calibration_missing", report["issues"])

    def test_invalid_calibration_identity_brackets_timebase_and_error_are_rejected(self):
        actual, calibration = calibration_fixture()
        mutations = [lambda c: c["trace_identity"].update(binary_sha256="wrong"),
                     lambda c: c["source_clock"]["timebase"].update(denom=0),
                     lambda c: c["external_clock"].update(clock_id="cg_event_timestamp_ns"),
                     lambda c: c.update(max_error_ns="1"),
                     lambda c: c["sync_samples"][0].update(external_ns="999"),
                     lambda c: c["sync_samples"][0].update(before_raw_ticks="90"),
                     lambda c: c["provenance"].update(producer_sha256="not-a-hash")]
        for mutate in mutations:
            with self.subTest(mutation=mutate):
                broken = copy.deepcopy(calibration)
                mutate(broken)
                report = summarize(fixture(), True, actual, broken)
                self.assertEqual(report["activity_window_association_status"], "INCOMPLETE")
                self.assertIsNone(report["actual_dispatch_windows"])
        broken_actual = copy.deepcopy(actual)
        broken_actual["windows"][0]["last_dispatch_raw_ticks"] = "181"
        self.assertIsNone(summarize(fixture(), True, broken_actual, calibration)["actual_dispatch_windows"])

    def test_external_payload_is_never_copied_on_valid_or_invalid_evidence(self):
        actual, calibration = calibration_fixture()
        actual["windows"][0]["payload"] = "secret-keyboard-geometry"
        calibration["provenance"]["camera"] = "secret-keyboard-geometry"
        for evidence in [None, calibration]:
            report = summarize(fixture(), True, actual, evidence)
            self.assertIsNone(report["actual_dispatch_windows"])
            self.assertNotIn("secret-keyboard-geometry", repr(report))
            self.assertEqual(set(report["unassociated_external_windows"][0]),
                             {"id", "start_ns", "end_ns", "first_dispatch_raw_ticks", "last_dispatch_raw_ticks"})
        actual, calibration = calibration_fixture()
        calibration["provenance"]["source"] = "/private/secret-keyboard-geometry"
        self.assertNotIn("secret-keyboard-geometry", repr(summarize(fixture(), True, actual, calibration)))

    def test_raw_tick_coverage_cannot_be_faked_by_nanosecond_rounding_or_empty_plan(self):
        rows = fixture()
        actual, calibration = calibration_fixture()
        rows[0]["clock"]["conversion"]["denom"] = 3
        rows[0]["clock"]["origin_source_ns"] = "33"
        calibration["source_clock"]["timebase"]["denom"] = 3
        calibration["external_clock"]["timebase"]["denom"] = 3
        for sample in calibration["sync_samples"]:
            sample["external_ns"] = str(int(sample["external_raw_ticks"]) // 3)
        actual["windows"][0].update(first_dispatch_raw_ticks="79", start_ns="26", end_ns="60")
        self.assertIsNone(summarize(rows, True, actual, calibration)["actual_dispatch_windows"])
        rows = fixture()
        rows[0]["windows"] = []
        actual, calibration = calibration_fixture()
        self.assertIsNone(summarize(rows, True, actual, calibration)["actual_dispatch_windows"])

    def test_source_ns_exactness_and_gap_are_not_writer_arrival_time(self):
        self.assertEqual(ns("9007199254740993"), 9007199254740993)
        for value in [9007199254740993, "1.5", "-1", "18446744073709551616"]:
            with self.assertRaises(ValueError):
                ns(value)
        rows = fixture()
        rows[2], rows[3] = rows[3], rows[2]
        report = summarize(rows, True)
        self.assertTrue(report["physical_row_order_differs_from_sequence"])
        self.assertEqual(report["planned_windows"][0]["intervals"][0]["end_ns"], "140")

    def test_fallback_source_and_binding_versions_must_resolve_and_match(self):
        rows = fixture()
        rows[3]["previous"] = None
        source = {"update_id": 1, "version_id": 99, "scene_serial": 42}
        binding = dict(rows[3]["binding"], rendered_source_update_id=1,
                       rendered_source_version_id=99, rendered_scene_serial=42)
        rows[-1]["counters"]["attempted"] = 6
        rows[-1:-1] = [{"record_seq": 5, "kind": "render_source", "source": source},
                      {"record_seq": 6, "kind": "callback_enqueued", "binding": binding}]
        report = summarize(rows, True)
        self.assertEqual(report["observation_status"], "INCOMPLETE")
        self.assertIn("dangling_render_source_version", report["issues"])
        self.assertIn("dangling_rendered_binding_version", report["issues"])
        source["version_id"] = 1
        binding["rendered_source_version_id"] = 1
        binding["rendered_scene_serial"] = 43
        self.assertIn("render_source_binding_mismatch", summarize(rows, True)["issues"])

    def test_endpoint_validation_is_independent_of_configured_windows(self):
        rows = fixture()
        rows[0]["windows"] = []
        rows[3]["previous"] = None
        rows[4]["source_ns"] = "139"
        rows[5].update(source_ns="139", end_ns="139", wall_duration_ns="0")
        report = summarize(rows, True)
        self.assertEqual(report["observation_status"], "INCOMPLETE")
        self.assertIn("inconsistent_wall_duration", report["issues"])


if __name__ == "__main__":
    unittest.main()
