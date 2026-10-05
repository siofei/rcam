"""Strict PMIX close-frame coverage regressions using synthetic unit data.

These tests call the production portable verifier. Their invented input, UI,
paint and readback records are never native acquisition or acceptance evidence.
"""

import copy
import hashlib
import unittest

import verify_pmix_native
from verify_batch_drag_native import UNIFORM_BYTES


def fixture(frame_count=6):
    """Keep every update, including the actually painted terminal update."""
    assert frame_count >= 2
    version = {
        "document_id": "synthetic-document",
        "document_revision": "1",
        "workspace_revision": "1",
        "generation": 2,
        "rule_revision": 0,
        "geometry_policy_hash": hashlib.sha256(
            b'{"resolution_mm":0.0001}'
        ).hexdigest(),
    }
    frames = []
    ui = []
    paints = []
    for number in range(1, frame_count + 1):
        input_ns = number * 20_000_000
        state = {
            "version": copy.deepcopy(version),
            "scene_serial": 7,
            "selected": 1000,
            "busy": False,
            "display_pending": False,
        }
        view = {"rect": [0.0, 30.0, 800.0, 480.0], "ppp": 2.0}
        frames.append({
            "id": number,
            "phase": 14,
            "pass_index": 0,
            "input_ns": input_ns,
            "observed_ns": input_ns + 1_000_000,
            "completed_ns": input_ns + 2_000_000,
            "input_gpu_complete_ms": 2.0,
            "frame_interval_ms": None if number == 1 else 20.0,
            "painted": True,
            "gpu_completed": True,
            "state": state,
            "view": view,
            "counters": {
                "draw": number,
                "uniform-upload": number * UNIFORM_BYTES,
            },
        })
        ui.append({
            "frame": number,
            "pass_index": 0,
            "t_ns": input_ns + 500_000,
            "input": {"t_ns": input_ns + 100_000},
            "version": copy.deepcopy(version),
            "scene_serial": state["scene_serial"],
            "selected": state["selected"],
            "busy": state["busy"],
            "display_pending": state["display_pending"],
            "canvas": copy.deepcopy(view["rect"]),
            "ppp": view["ppp"],
        })
        paints.append({
            "frame": number,
            "t_ns": input_ns + 1_500_000,
            "viewport_px": [0, 60, 1600, 900],
            "clip_px": [0, 60, 1600, 900],
            "scissor_xyxy": [0, 60, 1600, 960],
        })

    # One actual request object is independently recorded by both producers.
    # No media bytes are needed for this frame/readback accounting unit test.
    request = copy.deepcopy(ui[0])
    request["request_ns"] = ui[0]["t_ns"] + 100_000
    roi_requests = [copy.deepcopy(request)]
    roi_samples = [{"sample": 0, "request": copy.deepcopy(request)}]
    roi_finalization = {
        "schema_version": 1,
        "frames": frame_count,
        "requests": len(roi_requests),
        "samples": len(roi_samples),
        "quiesced": True,
        "pending": False,
        "writer_joined": True,
    }
    close_frame = frames[0]
    report = {
        "observation_version": 3,
        "last_observed_frame_id": frame_count - 1,
        "frames": copy.deepcopy(frames[:-1]),
        "terminal_frame": copy.deepcopy(frames[-1]),
        "counters": copy.deepcopy(frames[-1]["counters"]),
        "exit": {
            "close_requested_frame_id": close_frame["id"],
            "close_requested_ns": close_frame["observed_ns"] + 100_000,
            "exited_ns": frames[-1]["completed_ns"] + 1_000_000,
            "roi_finalization": copy.deepcopy(roi_finalization),
            "full_surface_requests_drained": True,
        },
    }
    return report, ui, paints, roi_requests, roi_samples, roi_finalization


def unpainted_terminal_fixture(frame_count=6):
    """An update with no callback is retained with no invented fence."""
    values = fixture(frame_count)
    report, _, paints, *_ = values
    terminal = report["terminal_frame"]
    terminal["painted"] = False
    for field in ("gpu_completed", "completed_ns", "input_gpu_complete_ms"):
        terminal.pop(field)
    terminal["counters"] = copy.deepcopy(report["frames"][-1]["counters"])
    report["counters"] = copy.deepcopy(terminal["counters"])
    paints.pop()
    return values


class FrameCoverage(unittest.TestCase):
    def bind(self, values):
        return verify_pmix_native.bind_frame_producers(*values)

    def reject(self, mutate, frame_count=6, build_fixture=fixture):
        baseline = build_fixture(frame_count)
        self.bind(baseline)
        changed = copy.deepcopy(baseline)
        mutate(*changed)
        with self.assertRaises(ValueError):
            self.bind(changed)
        # Every isolated negative leaves its original positive intact.
        self.bind(baseline)

    def test_complete_short_and_multiple_close_redraws(self):
        for count in (2, 3, 6):
            with self.subTest(frame_count=count):
                report, ui, paints, requests, samples, finalization = fixture(count)
                self.assertEqual(len(ui), len(report["frames"]) + 1)
                self.assertEqual(report["terminal_frame"]["id"], count)
                self.bind((report, ui, paints, requests, samples, finalization))

    def test_close_request_in_terminal_update_is_also_fully_accounted(self):
        values = fixture(2)
        report = values[0]
        terminal = report["terminal_frame"]
        report["exit"].update(
            close_requested_frame_id=terminal["id"],
            close_requested_ns=terminal["observed_ns"] + 100_000,
        )
        self.bind(values)

    def test_actual_unpainted_terminal_update_is_retained_without_a_fence(self):
        self.bind(unpainted_terminal_fixture())

    def test_unpainted_terminal_cannot_claim_gpu_completion(self):
        for field, value in (("gpu_completed", True), ("completed_ns", 123),
                             ("input_gpu_complete_ms", 2.0)):
            with self.subTest(field=field):
                self.reject(lambda report, *_, field=field, value=value:
                            report["terminal_frame"].update({field: value}),
                            build_fixture=unpainted_terminal_fixture)
        self.reject(lambda report, ui, paints, *_: paints.append({"frame": 6}),
                    build_fixture=unpainted_terminal_fixture)

    def test_old_observation_schema_is_rejected(self):
        self.reject(lambda report, *_: report.update(observation_version=2))

    def test_ordinary_frame_deletion_is_rejected(self):
        for index in (0, 2, -1):
            with self.subTest(index=index):
                self.reject(lambda report, *_, index=index:
                            report["frames"].pop(index))

    def test_ordinary_frame_duplication_and_renumbering_are_rejected(self):
        self.reject(lambda report, *_: report["frames"].insert(
            2, copy.deepcopy(report["frames"][1])))
        self.reject(lambda report, *_: report["frames"][2].update(id=4))
        self.reject(lambda report, *_: report.update(last_observed_frame_id=4))

    def test_deleted_middle_frame_resealed_with_continuous_ids_is_rejected(self):
        def mutate(report, ui, paints, requests, samples, finalization):
            report["frames"].pop(2)
            ui.pop(2)
            paints.pop(2)
            all_frames = report["frames"] + [report["terminal_frame"]]
            for number, (frame, ui_frame, paint) in enumerate(
                    zip(all_frames, ui, paints), 1):
                frame["id"] = ui_frame["frame"] = paint["frame"] = number
                if number > 1:
                    frame["frame_interval_ms"] = (
                        frame["input_ns"] - all_frames[number - 2]["input_ns"]
                    ) / 1_000_000
            report["last_observed_frame_id"] = len(report["frames"])
            finalization["frames"] = len(all_frames)
            report["exit"]["roi_finalization"] = copy.deepcopy(finalization)
            # Actual callback increments still expose the missing draw; no
            # producer counts or thresholds are relaxed for the relabelled IDs.
        self.reject(mutate)

    def test_missing_duplicate_and_renumbered_terminal_are_rejected(self):
        self.reject(lambda report, *_: report.pop("terminal_frame"))
        self.reject(lambda report, *_: report.update(terminal_frame=None))
        self.reject(lambda report, *_: report["frames"].append(
            copy.deepcopy(report["terminal_frame"])))
        self.reject(lambda report, *_: report["terminal_frame"].update(id=5))
        self.reject(lambda report, *_: report["terminal_frame"].update(id=7))

    def test_ui_deletion_duplication_and_renumbering_are_rejected(self):
        for index in (0, 2, -1):
            with self.subTest(index=index):
                self.reject(lambda report, ui, *_, index=index: ui.pop(index))
        self.reject(lambda report, ui, *_: ui.insert(2, copy.deepcopy(ui[1])))
        self.reject(lambda report, ui, *_: ui[-1].update(frame=5))

    def test_terminal_ui_has_no_arbitrary_tail_tolerance(self):
        self.reject(lambda report, ui, *_: ui.append(
            dict(copy.deepcopy(ui[-1]), frame=7)))
        self.reject(lambda report, ui, *_: ui.extend([
            dict(copy.deepcopy(ui[-1]), frame=7),
            dict(copy.deepcopy(ui[-1]), frame=8),
        ]))

    def test_additional_update_pass_and_reused_raw_input_are_rejected(self):
        self.reject(lambda report, ui, *_: ui[-1].update(pass_index=1))
        self.reject(lambda report, ui, *_: ui[-1].update(pass_index=False))
        self.reject(lambda report, *_: report['terminal_frame'].update(pass_index=False))
        self.reject(lambda report, *_: report["terminal_frame"].update(pass_index=1))
        self.reject(lambda report, ui, *_: ui[-1]["input"].update(
            t_ns=ui[-2]["input"]["t_ns"]))

    def test_terminal_scene_version_and_canvas_bindings_are_exact(self):
        for field, value in (("scene_serial", 8), ("selected", 999),
                             ("busy", True), ("display_pending", True),
                             ("canvas", [0.0, 30.0, 801.0, 480.0]),
                             ("ppp", 1.0)):
            with self.subTest(field=field):
                self.reject(lambda report, ui, *_, field=field, value=value:
                            ui[-1].update({field: value}))
        self.reject(lambda report, ui, *_: ui[-1]["version"].update(
            document_revision="2"))

    def test_missing_duplicate_and_changed_terminal_paint_are_rejected(self):
        self.reject(lambda report, ui, paints, *_: paints.pop())
        self.reject(lambda report, ui, paints, *_: paints.append(
            copy.deepcopy(paints[-1])))
        self.reject(lambda report, ui, paints, *_: paints[-1].update(frame=5))

    def test_terminal_and_close_update_phases_are_bound(self):
        self.reject(lambda report, *_: report["terminal_frame"].update(phase=13))
        self.reject(lambda report, *_: report["frames"][0].update(phase=13))

    def test_final_callback_counters_include_the_terminal_frame(self):
        for field in ("draw", "uniform-upload"):
            with self.subTest(field=field):
                self.reject(lambda report, *_, field=field:
                            report["counters"].update({field: 0}))
                self.reject(lambda report, *_, field=field:
                            report["terminal_frame"]["counters"].update({field: 0}))

    def test_terminal_gpu_completion_cannot_be_fabricated(self):
        self.reject(lambda report, *_: report["terminal_frame"].update(
            gpu_completed=False))
        self.reject(lambda report, *_: report["terminal_frame"].pop("gpu_completed"))
        self.reject(lambda report, *_: report["terminal_frame"].update(
            completed_ns=report["terminal_frame"]["observed_ns"] - 1))
        self.reject(lambda report, *_: report["terminal_frame"].update(
            input_gpu_complete_ms=50.0))

    def test_early_exit_and_unbound_close_request_are_rejected(self):
        self.reject(lambda report, *_: report.pop("exit"))
        self.reject(lambda report, *_: report["exit"].update(
            exited_ns=report["terminal_frame"]["completed_ns"] - 1))
        self.reject(lambda report, *_: report["exit"].update(
            close_requested_frame_id=0))
        self.reject(lambda report, *_: report["exit"].update(
            close_requested_frame_id=7))
        self.reject(lambda report, *_: report["exit"].update(
            close_requested_ns=report["frames"][-1]["input_ns"] - 1))
        self.reject(lambda report, *_: report["exit"].update(
            full_surface_requests_drained=False))

    def test_terminal_close_request_cannot_postdate_its_completed_callback(self):
        def mutate(report, *_):
            terminal = report["terminal_frame"]
            report["exit"].update(
                close_requested_frame_id=terminal["id"],
                close_requested_ns=terminal["completed_ns"] + 1,
            )
        self.reject(mutate)

    def test_roi_finalization_is_required_and_bound_to_exit(self):
        self.reject(lambda report, *_: report["exit"].pop("roi_finalization"))
        for field, value in (("schema_version", 2), ("frames", 5),
                             ("requests", 0), ("samples", 0),
                             ("quiesced", False), ("pending", True),
                             ("writer_joined", False)):
            with self.subTest(field=field):
                def mutate(report, ui, paints, requests, samples, finalization):
                    finalization[field] = value
                    report["exit"]["roi_finalization"] = copy.deepcopy(finalization)
                self.reject(mutate)
        self.reject(lambda report, *_: report["exit"]["roi_finalization"].update(
            frames=5))

    def test_roi_request_and_sample_deletion_and_duplication_are_rejected(self):
        self.reject(lambda report, ui, paints, requests, *_: requests.clear())
        self.reject(lambda report, ui, paints, requests, *_: requests.append(
            copy.deepcopy(requests[0])))
        self.reject(lambda report, ui, paints, requests, samples, *_: samples.clear())
        self.reject(lambda report, ui, paints, requests, samples, *_: samples.append(
            copy.deepcopy(samples[0])))

    def test_resealed_duplicate_readbacks_are_still_rejected(self):
        def mutate(report, ui, paints, requests, samples, finalization):
            requests.append(copy.deepcopy(requests[0]))
            samples.append(dict(copy.deepcopy(samples[0]), sample=1))
            finalization.update(requests=2, samples=2)
            report["exit"]["roi_finalization"] = copy.deepcopy(finalization)
        self.reject(mutate)

    def test_roi_request_sample_identity_and_sequence_are_exact(self):
        self.reject(lambda report, ui, paints, requests, samples, *_:
                    samples[0].update(sample=1))
        self.reject(lambda report, ui, paints, requests, samples, *_:
                    samples[0].update(sample=False))
        self.reject(lambda report, ui, paints, requests, samples, *_:
                    samples[0]["request"].update(request_ns=0))
        self.reject(lambda report, ui, paints, requests, samples, *_:
                    requests[0].update(frame=2))

    def test_zero_readbacks_cannot_claim_successful_finalization(self):
        def mutate(report, ui, paints, requests, samples, finalization):
            requests.clear()
            samples.clear()
            finalization.update(requests=0, samples=0)
            report["exit"]["roi_finalization"] = copy.deepcopy(finalization)
        self.reject(mutate)


if __name__ == "__main__":
    unittest.main()
