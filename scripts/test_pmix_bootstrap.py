"""Synthetic constructor-request regressions, never native acceptance evidence.

The real verifier reads source pins and actual request/worker/UI-shaped records.
No private capture, machine detail, run identity or fabricated UI frame0 is used.
"""

import copy
import hashlib
from pathlib import Path
import tempfile
import unittest

import verify_pmix_native


SOURCE_PATHS = (
    "crates/editor-app/src/main.rs",
    "crates/editor-app/src/native_pmix.rs",
    "crates/editor-app/src/native_s5m1.rs",
    "crates/editor-app/src/state.rs",
    "crates/editor-service/src/task.rs",
)
POLICY_HASH = hashlib.sha256(b'{"resolution_mm":0.0001}').hexdigest()


def version(document=None, generation=0):
    return {
        "document_id": document,
        "document_revision": None if document is None else "0",
        "workspace_revision": None if document is None else "0",
        "generation": generation,
        "rule_revision": 0,
        "geometry_policy_hash": "" if document is None else POLICY_HASH,
    }


def ui_state(task_version, serial, objects=0, busy=False):
    empty = task_version["document_id"] is None
    return {
        "document_id": task_version["document_id"],
        "revision": task_version["document_revision"],
        "workspace_revision": task_version["workspace_revision"],
        "version": copy.deepcopy(task_version),
        "undo": None if empty else 0,
        "redo": None if empty else 0,
        "selected": 0,
        "primary": None,
        "scene_serial": serial,
        "scene_objects": objects,
        "dirty": None if empty else False,
        "project_dirty": None if empty else False,
        "busy": busy,
        "display_pending": False,
        "delta": None,
    }


def worker(sequence, action, input_version, result_version, start, finish, serial):
    state = ui_state(result_version, serial)
    state = {key: state[key] for key in (
        "document_id", "revision", "workspace_revision", "version",
        "undo", "redo", "selected", "scene_serial",
    )}
    return {
        "sequence": sequence,
        "action": action,
        "started_ns": start,
        "finished_ns": finish,
        "error": None,
        "blocked": None,
        "receipt": {
            "task_id": sequence,
            "input": copy.deepcopy(input_version),
            "result_version": copy.deepcopy(result_version),
            "state": "completed",
        },
        "state": state,
    }


def fixture(delayed_install=False):
    empty = version()
    initial = version("synthetic-empty-workspace", 1)
    opened = version("synthetic-opened-workspace", 2)
    frames = []
    for number in range(1, 6):
        if delayed_install and number == 1:
            state = ui_state(empty, None, busy=True)
        elif number <= 2:
            state = ui_state(initial, 1)
        else:
            state = ui_state(opened, 2, objects=11)
        frames.append({
            "id": number,
            "input_ns": number * 1_000_000,
            "observed_ns": number * 1_000_000 + 500_000,
            "state": state,
        })
    bootstrap = {
        "sequence": 1,
        "action": "other",
        "frame_id": 0,
        "at_ns": 10_000,
        "input": copy.deepcopy(empty),
        "view_version": copy.deepcopy(empty),
    }
    regular = {
        "sequence": 2,
        "action": "project-open",
        "frame_id": 2,
        "at_ns": 2_100_000,
        "input": copy.deepcopy(initial),
        "view_version": copy.deepcopy(initial),
    }
    bootstrap_finish = 1_700_000 if delayed_install else 30_000
    report = {
        "request": {"mode": "workflow-reopen"},
        "frames": frames[:-1],
        "terminal_frame": frames[-1],
        "requests": [bootstrap, regular],
        "worker": [
            worker(1, "other", empty, initial, 20_000, bootstrap_finish, 1),
            worker(2, "project-open", initial, opened, 2_200_000, 2_800_000, 2),
        ],
    }
    # Ordinary records share their real objects with the complete frame list,
    # exactly as frames + [terminal_frame] does in the production caller.
    return report, frames


def readonly_fixture():
    report, frames = fixture()
    initial = report["worker"][0]["receipt"]["result_version"]
    report["requests"][1]["action"] = "selection-centers"
    report["worker"][1]["action"] = "selection-centers"
    report["worker"][1]["receipt"]["result_version"] = copy.deepcopy(initial)
    report["worker"][1]["state"] = copy.deepcopy(report["worker"][0]["state"])
    for frame in frames[2:]:
        frame["state"] = ui_state(initial, 1)
    return report, frames


class Bootstrap(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="rcam-bootstrap-unit-")
        self.source_manifest = Path(self.temp.name) / "source-manifest.sha256"
        self.source_lines = [
            f"{hashlib.sha256((verify_pmix_native.ROOT / name).read_bytes()).hexdigest()}  {name}"
            for name in SOURCE_PATHS
        ]
        # A legitimate unrelated manifest row is not a required source pin.
        self.source_lines.append("0" * 64 + "  docs/synthetic-unit-note.md")
        self.write_manifest(self.source_lines)

    def tearDown(self):
        self.temp.cleanup()

    def write_manifest(self, lines):
        self.source_manifest.write_text("\n".join(lines) + "\n")

    def bind(self, values):
        report, frames = values
        return verify_pmix_native.bind_worker_requests(
            report, frames, self.source_manifest
        )

    def reject(self, mutate, delayed_install=False):
        baseline = fixture(delayed_install)
        self.bind(baseline)
        changed = copy.deepcopy(baseline)
        mutate(*changed)
        with self.assertRaises(ValueError):
            self.bind(changed)
        self.bind(baseline)

    def reject_manifest(self, mutate):
        baseline = fixture()
        self.bind(baseline)
        changed = list(self.source_lines)
        mutate(changed)
        self.write_manifest(changed)
        with self.assertRaises(ValueError):
            self.bind(baseline)
        self.write_manifest(self.source_lines)
        self.bind(baseline)

    def test_existing_bootstrap_and_ordinary_request_are_retained(self):
        values = fixture()
        before = copy.deepcopy(values)
        result = self.bind(values)
        self.assertEqual(values, before)
        self.assertEqual([frame["id"] for frame in values[1]], [1, 2, 3, 4, 5])
        self.assertEqual(values[0]["requests"][0]["action"], "other")
        self.assertNotIn("events", values[0])
        self.assertEqual(result["observed_action"], "other")
        self.assertEqual(result["first_installed_ui_frame"], 1)

    def test_worker_can_finish_after_first_input_before_later_install(self):
        values = fixture(delayed_install=True)
        report, frames = values
        self.assertGreater(report["worker"][0]["finished_ns"], frames[0]["input_ns"])
        self.assertLess(report["worker"][0]["finished_ns"], frames[1]["observed_ns"])
        result = self.bind(values)
        self.assertEqual(result["first_installed_ui_frame"], 2)

    def test_regular_enqueue_can_precede_same_install_frame_observation(self):
        values = fixture(delayed_install=True)
        report, frames = values
        self.assertLess(report["requests"][1]["at_ns"], frames[1]["observed_ns"])
        self.bind(values)

    def test_missing_and_reordered_bootstrap_are_rejected(self):
        def remove_initial(report, _):
            report["requests"].pop(0)
            report["worker"].pop(0)
        self.reject(remove_initial)
        self.reject(lambda report, _: report["requests"].reverse())
        self.reject(lambda report, _: report["requests"].clear())

    def test_duplicate_bootstrap_and_second_frame0_request_are_rejected(self):
        def duplicate(report, _):
            report["requests"].insert(1, copy.deepcopy(report["requests"][0]))
            report["worker"].insert(1, copy.deepcopy(report["worker"][0]))
        self.reject(duplicate)
        self.reject(lambda report, _: report["requests"][1].update(frame_id=0))

    def test_bootstrap_frame_and_sequence_are_strict_integers(self):
        for value in (False, -1, 1):
            with self.subTest(frame=value):
                self.reject(lambda report, _, value=value:
                            report["requests"][0].update(frame_id=value))
        for field, value in (("sequence", True), ("sequence", 2), ("sequence", 0)):
            with self.subTest(field=field, value=value):
                self.reject(lambda report, _, field=field, value=value:
                            report["requests"][0].update({field: value}))
        self.reject(lambda report, _: report["worker"][0]["receipt"].update(task_id=True))
        self.reject(lambda report, _: report["worker"][0]["receipt"].update(task_id=2))

    def test_action_is_actual_other_and_not_an_invented_typed_action(self):
        self.reject(lambda report, _: report["requests"][0].update(action="new-workspace"))
        self.reject(lambda report, _: report["worker"][0].update(action="new-workspace"))
        def relabel_both(report, _):
            report["requests"][0]["action"] = "new-workspace"
            report["worker"][0]["action"] = "new-workspace"
        self.reject(relabel_both)

    def test_all_six_bootstrap_input_fields_are_empty_gen0(self):
        changes = (("document_id", "synthetic-forged-document"),
                   ("document_revision", "0"), ("workspace_revision", "0"),
                   ("generation", 1), ("generation", False),
                   ("rule_revision", 1), ("geometry_policy_hash", POLICY_HASH))
        for field, value in changes:
            with self.subTest(field=field, value=value):
                def mutate(report, _, field=field, value=value):
                    changed = version()
                    changed[field] = value
                    report["requests"][0]["input"] = copy.deepcopy(changed)
                    report["requests"][0]["view_version"] = copy.deepcopy(changed)
                    report["worker"][0]["receipt"]["input"] = copy.deepcopy(changed)
                self.reject(mutate)
        self.reject(lambda report, _: report["requests"][0]["input"].pop("rule_revision"))

    def test_bootstrap_view_and_worker_input_cannot_diverge(self):
        self.reject(lambda report, _: report["requests"][0]["view_version"].update(generation=1))
        self.reject(lambda report, _: report["worker"][0]["receipt"]["input"].update(generation=1))

    def test_bootstrap_result_is_successful_gen1_default_precision(self):
        for field, value in (("generation", 2), ("rule_revision", 1),
                             ("document_revision", "1"), ("workspace_revision", "1"),
                             ("geometry_policy_hash", "0" * 64)):
            with self.subTest(field=field):
                self.reject(lambda report, _, field=field, value=value:
                            report["worker"][0]["receipt"]["result_version"].update({field: value}))
        self.reject(lambda report, _: report["worker"][0]["receipt"].update(state="failed"))
        self.reject(lambda report, _: report["worker"][0].update(error={"code": "SYNTHETIC_ERROR"}))
        self.reject(lambda report, _: report["worker"][0].update(blocked="synthetic refusal"))

    def test_bootstrap_worker_empty_state_and_scene_serial_are_strict(self):
        for field, value in (("undo", 1), ("redo", 1), ("selected", 1),
                             ("undo", False), ("redo", False), ("selected", False),
                             ("scene_serial", False), ("scene_serial", -1)):
            with self.subTest(field=field, value=value):
                self.reject(lambda report, _, field=field, value=value:
                            report["worker"][0]["state"].update({field: value}))
        self.reject(lambda report, _: report["worker"][0]["state"]["version"].update(generation=2))

    def test_bootstrap_enqueue_and_worker_clocks_are_causal(self):
        self.reject(lambda report, frames: report["requests"][0].update(at_ns=frames[0]["input_ns"]))
        self.reject(lambda report, _: report["requests"][0].update(at_ns=-1))
        self.reject(lambda report, _: report["requests"][0].update(at_ns=False))
        self.reject(lambda report, _: report["worker"][0].update(started_ns=1))
        self.reject(lambda report, _: report["worker"][0].update(finished_ns=1))
        self.reject(lambda report, frames: report["worker"][0].update(finished_ns=frames[0]["observed_ns"] + 1))

    def test_bootstrap_must_have_a_real_empty_installed_ui_state(self):
        for field, value in (("scene_serial", 9), ("scene_serial", True), ("scene_objects", 1),
                             ("scene_objects", False), ("primary", "synthetic-selection"),
                             ("dirty", True), ("project_dirty", True),
                             ("undo", 1), ("redo", 1), ("selected", 1)):
            with self.subTest(field=field):
                self.reject(lambda report, frames, field=field, value=value:
                            frames[0]["state"].update({field: value}))
        def no_installed_initial_version(report, frames):
            for frame in frames[:2]:
                frame["state"] = ui_state(version(), None, busy=True)
        self.reject(no_installed_initial_version)

    def test_frames_before_delayed_install_stay_in_default_gen0_state(self):
        for field, value in (("dirty", True), ("selected", 1),
                             ("scene_objects", 1), ("scene_serial", 1)):
            with self.subTest(field=field):
                self.reject(lambda report, frames, field=field, value=value:
                            frames[0]["state"].update({field: value}), delayed_install=True)
        self.reject(lambda report, frames: frames[0]["state"]["version"].update(generation=1),
                    delayed_install=True)

    def test_missing_wrong_and_duplicate_required_source_pins_are_rejected(self):
        for index in range(len(SOURCE_PATHS)):
            with self.subTest(pin=SOURCE_PATHS[index]):
                self.reject_manifest(lambda lines, index=index: lines.pop(index))
                self.reject_manifest(lambda lines, index=index:
                                     lines.__setitem__(index, "f" * 64 + "  " + SOURCE_PATHS[index]))
                self.reject_manifest(lambda lines, index=index: lines.append(lines[index]))

    def test_malformed_source_manifest_is_rejected(self):
        self.reject_manifest(lambda lines: lines.__setitem__(0, "invalid  " + SOURCE_PATHS[0]))
        self.reject_manifest(lambda lines: lines.append(lines[-1]))
        self.reject_manifest(lambda lines: lines.clear())

    def test_request_worker_coverage_and_terminal_ids_remain_required(self):
        self.reject(lambda report, _: report["worker"].pop())
        self.reject(lambda report, _: report["requests"].pop())
        self.reject(lambda report, _: report["worker"][1]["receipt"].update(task_id=3))
        self.reject(lambda report, _: report["worker"][1]["receipt"].update(state="running"))

    def test_ordinary_requests_keep_exact_ui_version_binding(self):
        self.reject(lambda report, _: report["requests"][1]["input"].update(document_revision="1"))
        self.reject(lambda report, _: report["requests"][1]["view_version"].update(document_revision="1"))
        self.reject(lambda report, _: report["worker"][1]["receipt"]["input"].update(document_revision="1"))
        self.reject(lambda report, _: report["requests"][1].update(action="other"))

    def test_ordinary_requests_keep_exact_ui_interval_binding(self):
        self.reject(lambda report, frames: report["requests"][1].update(at_ns=frames[1]["input_ns"] - 1))
        self.reject(lambda report, frames: report["requests"][1].update(at_ns=frames[2]["input_ns"]))
        self.reject(lambda report, _: report["worker"][1].update(started_ns=2_000_000))
        self.reject(lambda report, _: report["requests"][1].update(frame_id=False))
        self.reject(lambda report, _: report["requests"][1].update(frame_id=999))

    def test_bootstrap_does_not_skip_serial_version_continuity(self):
        def mutate(report, frames):
            alternate = version("synthetic-unrelated-workspace", 1)
            report["requests"][1]["input"] = copy.deepcopy(alternate)
            report["requests"][1]["view_version"] = copy.deepcopy(alternate)
            report["worker"][1]["receipt"]["input"] = copy.deepcopy(alternate)
            frames[1]["state"] = ui_state(alternate, 1)
        self.reject(mutate)

    def test_bootstrap_does_not_skip_worker_result_version_checks(self):
        self.reject(lambda report, _: report["worker"][1]["receipt"]["result_version"].update(generation=3))
        self.reject(lambda report, _: report["worker"][1]["state"]["version"].update(generation=3))
        self.reject(lambda report, _: report["worker"][1]["state"].update(document_id="synthetic-wrong-result"))

    def test_readonly_worker_preserves_its_complete_input_version(self):
        self.bind(readonly_fixture())
        report, frames = fixture()
        report["requests"][1]["action"] = "selection-centers"
        report["worker"][1]["action"] = "selection-centers"
        # Everything else is internally consistent, but this supposed
        # read-only query replaces the document and advances generation.
        with self.assertRaises(ValueError):
            self.bind((report, frames))

    def test_existing_locked_move_failure_is_bounded_and_still_typed(self):
        report, frames = readonly_fixture()
        report["request"]["mode"] = "workflow"
        report["requests"][1]["action"] = "move"
        report["worker"][1]["action"] = "move"
        report["worker"][1]["error"] = {"code": "LAYER_LOCKED"}
        report["worker"][1]["receipt"]["state"] = "failed"
        self.bind((report, frames))
        for change in (lambda value: value["request"].update(mode="workflow-reopen"),
                       lambda value: value["worker"][1]["error"].update(code="SYNTHETIC_ERROR"),
                       lambda value: value["worker"][1]["receipt"].update(state="completed")):
            changed = copy.deepcopy((report, frames))
            change(changed[0])
            with self.assertRaises(ValueError):
                self.bind(changed)

    def test_workers_cannot_outlive_final_ordinary_input_or_overlap(self):
        self.reject(lambda report, _: report["worker"][1].update(
            finished_ns=report["frames"][-1]["input_ns"] + 1))
        self.reject(lambda report, _: report["worker"][1].update(started_ns=25_000))
        self.reject(lambda report, _: report["worker"][1].update(error={"code": "SYNTHETIC_ERROR"}))
        self.reject(lambda report, _: report["worker"][1].update(blocked="synthetic blocked"))

    def test_duplicate_ordinary_sequence_is_rejected(self):
        def mutate(report, _):
            report["requests"].append(copy.deepcopy(report["requests"][1]))
            report["worker"].append(copy.deepcopy(report["worker"][1]))
        self.reject(mutate)


if __name__ == "__main__":
    unittest.main()
