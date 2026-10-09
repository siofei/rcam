import copy
import unittest
from frame_trace_report import boundary, ns, summarize


def fixture():
    binding = {"update_id": 1, "version_id": 1, "input_batch_id": 1, "rendered_source_update_id": 0}
    return [
        {"kind": "header", "schema_version": 1, "run_id": "run", "clock": {"clock_id": "mach_absolute_time_ns"},
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
        report = summarize(rows, True)
        self.assertEqual(report["observation_status"], "COMPLETE_WITHIN_DECLARED_SCOPE")
        self.assertEqual(report["performance_status"], "OPEN")
        interval = report["planned_windows"][0]["intervals"][0]
        self.assertIsNone(interval["wall_duration_ns"])
        self.assertEqual(interval["classification"], "no_previous")

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
        actual = {"run_id": "run", "clock_id": "mach_absolute_time_ns", "clock_mapping_verified": True,
                  "windows": [{"id": "active", "start_ns": "120", "end_ns": "180"}]}
        report = summarize(fixture(), True, actual)
        self.assertEqual(report["planned_windows"][0]["window"]["start_ns"], "100")
        self.assertEqual(report["actual_dispatch_windows"][0]["window"]["start_ns"], "120")
        wrong = copy.deepcopy(actual)
        wrong["clock_mapping_verified"] = False
        with self.assertRaises(ValueError):
            summarize(fixture(), True, wrong)

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
