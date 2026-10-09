#!/usr/bin/env python3
"""Offline, loss-aware source endpoint classification; never a presentation/FPS verdict."""
import argparse
import hashlib
import json
import math
from pathlib import Path

MOVE_EXIT_REASONS = frozenset({
    "focus_lost", "pointer_gone", "window_focus_lost", "escape", "secondary_click",
    "point_pick_cancelled", "modal_cancelled", "modal_replaced", "tool_changed",
    "modal_task_completed", "context_changed", "transition", "initial_admission_rejected",
    "task_identity_mismatch", "apply_terminal", "task_cancelled", "task_error",
    "placement_missing", "invalid_base", "preview_phase_mismatch", "preview_missing",
    "worker_disconnected", "request_invalid", "admission_rejected",
})
MOVE_PHASES = frozenset({"preparing", "following", "frozen", "final_preview", "ready", "applying"})
EXIT_BINDING_INTEGERS = ("viewport_id", "update_id", "egui_frame_nr", "egui_pass_nr", "pass_index",
                         "input_batch_id", "version_id", "render_callback_id", "rendered_source_version_id",
                         "rendered_source_update_id", "rendered_scene_serial")


def ns(value):
    if not isinstance(value, str) or not value.isascii() or not value.isdecimal():
        raise ValueError("source time must be an integer decimal string")
    result = int(value)
    if result >= 2**64:
        raise ValueError("source time exceeds u64")
    return result

def valid_move_exit(row):
    binding = row.get("binding")
    if (type(row.get("record_seq")) is not int or not 0 < row["record_seq"] < 2**64
            or type(row.get("phase")) is not str or row["phase"] not in MOVE_PHASES
            or type(binding) is not dict or type(binding.get("egui_identity_known")) is not bool
            or any(type(binding.get(key)) is not int or not 0 <= binding[key] < 2**64
                   for key in EXIT_BINDING_INTEGERS)
            or binding["version_id"] == 0
            or (binding["egui_identity_known"] and binding["update_id"] == 0)
            or (not binding["egui_identity_known"] and any(binding[key] != 0 for key in
                ("viewport_id", "update_id", "egui_frame_nr", "egui_pass_nr", "pass_index", "input_batch_id")))):
        return False
    try:
        ns(row.get("source_ns"))
    except (TypeError, ValueError):
        return False
    return True


def boundary(start, end, first, last):
    """Canonical endpoint rule: [first,last); every original span is retained."""
    if end < start or first >= last:
        raise ValueError("invalid endpoints")
    if first <= start and end < last:
        return "inside"
    if end < first:
        return "before"
    if start >= last:
        return "after"
    if start < first and end >= last:
        return "cross_both"
    if start < first:
        return "cross_start"
    return "cross_end"


def percentiles(values):
    values = sorted(values)
    if not values:
        return {"count": 0, "p95_ns": None, "p99_ns": None, "max_ns": None}
    return {"count": len(values), "p95_ns": str(values[math.ceil(.95 * len(values)) - 1]),
            "p99_ns": str(values[math.ceil(.99 * len(values)) - 1]), "max_ns": str(values[-1]),
            "quantile": "nearest rank; wall/source intervals, no presentation or FPS claim"}


def exact_keys(value, keys):
    if type(value) is not dict or set(value) != set(keys):
        raise ValueError("unexpected/missing structured evidence fields")


def safe_id(value, limit=64):
    return isinstance(value, str) and 0 < len(value) <= limit and all(
        c.isascii() and (c.isalnum() or c in "-_") for c in value)


def safe_external_windows(actual):
    """Never echo an external payload, even when evidence is missing or invalid."""
    windows = actual.get("windows") if isinstance(actual, dict) else None
    if not isinstance(windows, list):
        return None
    safe = []
    for window in windows[:16]:
        if not isinstance(window, dict):
            continue
        item = {"id": window["id"]} if safe_id(window.get("id")) else {}
        for key in ("start_ns", "end_ns", "first_dispatch_raw_ticks", "last_dispatch_raw_ticks"):
            try:
                ns(window[key])
                item[key] = window[key]
            except (KeyError, ValueError):
                pass
        safe.append(item)
    return safe


def calibration_check(header, actual, evidence):
    """Validate one supported, bounded evidence format, not an external clock assertion."""
    if actual is None or evidence is None:
        return None, ["external_activity_clock_calibration_missing"]
    try:
        exact_keys(actual, ("run_id", "clock_id", "windows"))
        exact_keys(evidence, ("schema_version", "method", "run_id", "trace_identity", "source_clock",
                              "external_clock", "provenance", "sync_samples", "max_error_ns"))
        if type(evidence["schema_version"]) is not int or evidence["schema_version"] != 1 or evidence["method"] != "shared_mach_absolute_timebase":
            raise ValueError("unsupported calibration method")
        if evidence["run_id"] != header["run_id"] or actual["run_id"] != header["run_id"]:
            raise ValueError("run mismatch")
        identity_keys = ("commit", "source_manifest_sha256", "binary_sha256", "config_sha256")
        exact_keys(evidence["trace_identity"], identity_keys)
        for key in identity_keys:
            if evidence["trace_identity"][key] != header[key]:
                raise ValueError("trace identity mismatch")
        source, external = evidence["source_clock"], evidence["external_clock"]
        for clock in (source, external):
            exact_keys(clock, ("clock_id", "origin_raw_ticks", "timebase"))
            exact_keys(clock["timebase"], ("numer", "denom"))
        if (source["clock_id"] != "mach_absolute_time_ns" or external["clock_id"] != source["clock_id"]
                or header["clock"]["clock_id"] != source["clock_id"] or actual["clock_id"] != source["clock_id"]
                or source["origin_raw_ticks"] != header["clock"]["origin_raw_ticks"]
                or source["timebase"] != header["clock"]["conversion"]
                or external["timebase"] != source["timebase"]):
            raise ValueError("clock domain/timebase mismatch")
        numer, denom = source["timebase"]["numer"], source["timebase"]["denom"]
        if any(type(v) is not int or not 0 < v < 2**32 for v in (numer, denom)):
            raise ValueError("invalid timebase")
        def converted(raw):
            result = ns(raw) * numer // denom
            if result >= 2**64:
                raise ValueError("converted time exceeds u64")
            return result
        if converted(source["origin_raw_ticks"]) != ns(header["clock"]["origin_source_ns"]):
            raise ValueError("source origin mismatch")
        converted(external["origin_raw_ticks"])
        provenance = evidence["provenance"]
        exact_keys(provenance, ("source", "producer_pid", "producer_sha256", "evidence_sha256"))
        if (not safe_id(provenance["source"], 128)
                or type(provenance["producer_pid"]) is not int or provenance["producer_pid"] <= 0):
            raise ValueError("missing producer identity")
        for key in ("producer_sha256", "evidence_sha256"):
            value = provenance[key]
            if not isinstance(value, str) or len(value) != 64 or any(c not in "0123456789abcdef" for c in value):
                raise ValueError("invalid provenance hash")
        samples = evidence["sync_samples"]
        if not isinstance(samples, list) or not 2 <= len(samples) <= 32:
            raise ValueError("bounded synchronization samples required")
        external_times, external_ticks, widths = [], [], []
        previous_after = None
        for sample in samples:
            exact_keys(sample, ("before_raw_ticks", "external_raw_ticks", "after_raw_ticks", "external_ns"))
            before, point, after = (ns(sample[key]) for key in
                                    ("before_raw_ticks", "external_raw_ticks", "after_raw_ticks"))
            if not before <= point <= after or (previous_after is not None and before < previous_after):
                raise ValueError("invalid synchronization bracket")
            if converted(sample["external_raw_ticks"]) != ns(sample["external_ns"]):
                raise ValueError("external conversion mismatch")
            external_times.append(ns(sample["external_ns"]))
            external_ticks.append(point)
            widths.append(converted(sample["after_raw_ticks"]) - converted(sample["before_raw_ticks"]))
            previous_after = after
        error = ns(evidence["max_error_ns"])
        if error < max(widths):
            raise ValueError("understated synchronization error bound")
        windows = actual["windows"]
        if not isinstance(windows, list) or not 1 <= len(windows) <= 16:
            raise ValueError("actual dispatch windows required")
        identifiers = [window["id"] for window in windows]
        if (any(not safe_id(value) for value in identifiers) or len(set(identifiers)) != len(identifiers)
                or not header["windows"] or set(identifiers) != {window["id"] for window in header["windows"]}):
            raise ValueError("window identity mismatch")
        previous_end = None
        for window in windows:
            exact_keys(window, ("id", "start_ns", "end_ns", "first_dispatch_raw_ticks", "last_dispatch_raw_ticks"))
            first_ticks, last_ticks = ns(window["first_dispatch_raw_ticks"]), ns(window["last_dispatch_raw_ticks"])
            first, last = converted(window["first_dispatch_raw_ticks"]), converted(window["last_dispatch_raw_ticks"])
            if (first != ns(window["start_ns"]) or last != ns(window["end_ns"])
                    or not external_times[0] <= first < last <= external_times[-1]
                    or not external_ticks[0] <= first_ticks < last_ticks <= external_ticks[-1]
                    or (previous_end is not None and first < previous_end)):
                raise ValueError("dispatch bounds conversion/coverage mismatch")
            previous_end = last
        return {"status": "SUPPORTED_EVIDENCE_CONSISTENT", "method": evidence["method"],
                "max_error_ns": str(error), "evidence": evidence,
                "limit": "executor provenance and brackets, not per-CGEvent receipt or universal clock equivalence"}, []
    except (KeyError, TypeError, ValueError, OverflowError):
        return None, ["external_activity_clock_calibration_invalid_or_unsupported"]


def summarize(rows, supervised_ack=False, actual_boundaries=None, clock_calibration=None):
    headers = [row for row in rows if row.get("kind") == "header"]
    if len(headers) != 1 or headers[0].get("schema_version") != 2 or headers[0].get("writer_join_required") is not True:
        raise ValueError("expected exactly one supported header")
    header = headers[0]
    issues = []
    move_exits = []
    invalid_exits = set()
    for row in rows:
        if row.get("kind") != "move_exit":
            continue
        reason = row.get("reason")
        known = type(reason) is str and reason in MOVE_EXIT_REASONS
        valid = valid_move_exit(row)
        if not known:
            issues.append("move_exit_unknown_reason")
        if not valid:
            issues.append("move_exit_invalid_record")
            invalid_exits.add(id(row))
        move_exits.append({"raw": row, "reason_status":
                           "INVALID_RECORD" if not valid else "KNOWN_SOURCE_BRANCH" if known else "UNKNOWN_REASON"})
    footers = [row for row in rows if row.get("kind") == "footer"]
    footer = footers[0] if len(footers) == 1 else None
    if footer is None or not footer.get("finalized"):
        issues.append("missing_or_failed_finalization")
    if footer is None or not footer.get("observation_complete"):
        issues.append("observation_incomplete")
    if not supervised_ack:
        issues.append("matching_supervisor_flush_ack_not_verified")
    # Untrusted exit structure remains in raw projection, never in lineage/sequence qualification.
    # Other record types retain the existing strict parsing behavior.
    sequenced = [row for row in rows if "record_seq" in row and id(row) not in invalid_exits]
    sequences = [row["record_seq"] for row in sequenced]
    if any(type(value) is not int or value <= 0 for value in sequences):
        raise ValueError("invalid record sequence")
    if len(set(sequences)) != len(sequences):
        issues.append("duplicate_sequence")
    sequenced.sort(key=lambda row: row["record_seq"])
    missing = []
    previous_sequence = 0
    for row in sequenced:
        seq = row["record_seq"]
        if seq > previous_sequence + 1:
            missing.append([previous_sequence + 1, seq - 1])
        previous_sequence = max(previous_sequence, seq)
    attempted = footer.get("counters", {}).get("attempted", 0) if footer else 0
    if attempted > previous_sequence:
        missing.append([previous_sequence + 1, attempted])
    if missing:
        issues.append("record_sequence_gaps")
    counters = footer.get("counters", {}) if footer else {}
    if (any(counters.get("drop_by_stage", [])) or counters.get("dropped_metadata", 0)
            or counters.get("dropped_other", 0) or counters.get("failed")
            or counters.get("clock_failed") or counters.get("supervisor_timeout")):
        issues.append("drops_or_measurement_failure")
    versions = {row["metadata"]["version_id"] for row in sequenced
                if row.get("metadata", {}).get("kind") == "version"}
    sources = {row["source"]["update_id"]: row["source"] for row in sequenced
               if row.get("kind") == "render_source"}
    starts = {row["binding"]["update_id"]: row for row in sequenced if row.get("kind") == "update_start"}
    ends = {row["binding"]["update_id"]: row for row in sequenced if row.get("kind") == "update_end"}
    update_spans = {row["binding"]["update_id"]: row for row in sequenced
                    if row.get("kind") == "span" and row.get("stage") == "update"}
    for exit_record in move_exits:
        row = exit_record["raw"]
        if id(row) not in invalid_exits and row["binding"]["egui_identity_known"]:
            if row["binding"]["update_id"] not in starts:
                issues.append("move_exit_unknown_update")
    for row in sequenced:
        if row.get("kind") in ("span", "request_attempt"):
            start, end = ns(row["start_ns"]), ns(row["end_ns"])
            if end < start or ns(row["wall_duration_ns"]) != end - start:
                issues.append("inconsistent_wall_duration")
            if row["source_ns"] != row["end_ns"]:
                issues.append("noncanonical_span_source_end")
        if row.get("kind") == "render_source" and row["source"]["version_id"] not in versions:
            issues.append("dangling_render_source_version")
        binding = row.get("binding")
        if binding and binding["version_id"] and binding["version_id"] not in versions:
            issues.append("dangling_version_metadata")
        if row.get("kind") == "update_end" and row["end_version_id"] not in versions:
            issues.append("dangling_end_version")
        if binding and binding.get("rendered_source_update_id"):
            source = sources.get(binding["rendered_source_update_id"])
            if source is None:
                issues.append("dangling_render_source")
            elif (binding.get("rendered_source_version_id") != source["version_id"]
                  or binding.get("rendered_scene_serial") != source["scene_serial"]):
                issues.append("render_source_binding_mismatch")
            if binding.get("rendered_source_version_id") not in versions:
                issues.append("dangling_rendered_binding_version")
    for update_id in starts:
        if update_id not in ends or update_id not in update_spans:
            issues.append("unfinished_update")
            continue
        span = update_spans[update_id]
        if (span["outcome"] != "normal_end" or span["end_ns"] != ends[update_id]["source_ns"]
                or span["start_ns"] != starts[update_id]["source_ns"]):
            issues.append("noncanonical_update_end")
    ready = [ns(row["source_ns"]) for row in rows if row.get("kind") == "identity_ready"]
    if len(ready) != 1:
        issues.append("identity_ready_missing_or_duplicate")

    def windows_report(windows, origin):
        reports = []
        for window in windows:
            first, last = ns(window["start_ns"]), ns(window["end_ns"])
            intervals, spans, values = [], [], []
            window_issues = []
            if not ready or ready[0] > first:
                window_issues.append("initialization_overlaps_window")
            for update_id, row in sorted(starts.items()):
                previous = row["previous"]
                if previous is None:
                    intervals.append({"update_id": update_id, "previous": None, "end_ns": row["source_ns"],
                                      "wall_duration_ns": None, "classification": "no_previous"})
                    continue
                start, end = ns(previous["source_ns"]), ns(row["source_ns"])
                classification = boundary(start, end, first, last)
                prev_row = starts.get(previous["update_id"])
                gap = prev_row is None or previous["update_id"] + 1 != update_id
                if prev_row is not None:
                    gap |= prev_row["source_ns"] != previous["source_ns"]
                    lo, hi = sorted((prev_row["record_seq"], row["record_seq"]))
                    gap |= any(lo < stop and begin < hi for begin, stop in missing)
                intervals.append({"update_id": update_id, "current_binding": row["binding"], "previous": previous,
                                  "end_ns": row["source_ns"], "wall_duration_ns": str(end - start),
                                  "classification": classification, "observation_gap": gap})
                if classification == "inside" and not gap:
                    values.append(end - start)
                if gap and classification not in ("before", "after"):
                    window_issues.append("interval_observation_gap")
            for row in sequenced:
                if row.get("kind") not in ("span", "request_attempt"):
                    continue
                start, end = ns(row["start_ns"]), ns(row["end_ns"])
                if end < start:
                    spans.append({"record_seq": row["record_seq"], "raw": row,
                                  "classification": "invalid_endpoints"})
                    continue
                spans.append({"record_seq": row["record_seq"], "stage": row.get("stage", "request_attempt"),
                              "start_ns": row["start_ns"], "end_ns": row["end_ns"],
                              "wall_duration_ns": row["wall_duration_ns"], "classification": boundary(start, end, first, last),
                              "binding": row["binding"], "outcome": row["outcome"]})
            reports.append({"window": window, "boundary_origin": origin, "issues": sorted(set(window_issues)),
                            "intervals": intervals, "spans": spans, "inside_interval_diagnostics": percentiles(values)})
        return reports

    planned = windows_report(header["windows"], "internal_predefined_source_clock_only")
    actual = None
    calibration, calibration_issues = calibration_check(header, actual_boundaries, clock_calibration)
    if header["windows"] or actual_boundaries is not None:
        issues.extend(calibration_issues)
    if calibration is not None:
        actual = windows_report(actual_boundaries["windows"], "external_actual_first_last_dispatch")
    if any(report["issues"] for report in planned + (actual or [])):
        issues.append("window_measurement_incomplete")
    return {"schema_version": 2, "run_id": header["run_id"], "identity": header,
            "observation_status": "INCOMPLETE" if issues else "COMPLETE_WITHIN_DECLARED_SCOPE",
            "performance_status": "OPEN", "issues": sorted(set(issues)), "sequence_gaps": missing,
            "physical_row_order_differs_from_sequence": sequences != sorted(sequences),
            "footer": footer, "planned_windows": planned, "actual_dispatch_windows": actual,
            "activity_window_association_status": "CALIBRATED_WITH_DECLARED_ERROR_BOUND" if calibration else "INCOMPLETE",
            "clock_calibration": calibration,
            "unassociated_external_windows": safe_external_windows(actual_boundaries) if actual_boundaries and calibration is None else None,
            "internal_update_wall_diagnostics": percentiles([ns(row["wall_duration_ns"]) for row in update_spans.values()
                                                           if row["outcome"] == "normal_end"]),
            "updates": [{"start": row, "end": ends.get(update_id), "span": update_spans.get(update_id)}
                        for update_id, row in sorted(starts.items())],
            "input_batches": [row for row in sequenced if row.get("kind") == "input_counts"],
            "move_exits": move_exits,
            "move_exit_classification_status": ("NO_EXIT_RECORDS" if not move_exits else
                                                "INCOMPLETE" if issues else "KNOWN_SOURCE_BRANCHES_WITHIN_COMPLETE_TRACE"),
            "native_exit_attribution_status": "OPEN",
            "versions": [row["metadata"] for row in sequenced if row.get("metadata", {}).get("kind") == "version"],
            "render_sources": [row for row in sequenced if row.get("kind") == "render_source"],
            "callback_enqueued": [row for row in sequenced if row.get("kind") == "callback_enqueued"],
            "input_association": "App input batches map to update/pass; exact external event->batch unavailable",
            "render_association": "logical callback encoding only; actual submission/GPU completion/present unavailable"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("trace", type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--supervisor-log", type=Path)
    parser.add_argument("--actual-boundaries", type=Path)
    parser.add_argument("--clock-calibration", type=Path)
    args = parser.parse_args()
    raw = args.trace.read_bytes()
    rows, parse_errors = [], []
    for index, line in enumerate(raw.splitlines(), 1):
        if not line.strip():
            continue
        try:
            rows.append(json.loads(line))
        except (ValueError, UnicodeError):
            parse_errors.append({"line": index, "bytes": len(line)})
    header = next(row for row in rows if row.get("kind") == "header")
    ack = False
    if args.supervisor_log:
        log = args.supervisor_log.read_bytes()
        expected = f"RCAM_FRAME_TRACE finalization_ack=ok run_id={header['run_id']}"
        lines = log.decode("utf-8", errors="replace").splitlines()
        ack = lines.count(expected) == 1 and not any(
            f"finalization_ack=unconfirmed run_id={header['run_id']}" in line for line in lines)
    actual = json.loads(args.actual_boundaries.read_bytes()) if args.actual_boundaries else None
    calibration = json.loads(args.clock_calibration.read_bytes()) if args.clock_calibration else None
    report = summarize(rows, ack, actual, calibration)
    if parse_errors:
        report["issues"].append("truncated_or_invalid_source_rows")
        report["observation_status"] = "INCOMPLETE"
    report["parse_errors"] = parse_errors
    report["trace_sha256"] = hashlib.sha256(raw).hexdigest()
    report["supervisor_log_sha256"] = hashlib.sha256(args.supervisor_log.read_bytes()).hexdigest() if args.supervisor_log else None
    report["actual_boundaries_sha256"] = hashlib.sha256(args.actual_boundaries.read_bytes()).hexdigest() if args.actual_boundaries else None
    report["clock_calibration_sha256"] = hashlib.sha256(args.clock_calibration.read_bytes()).hexdigest() if args.clock_calibration else None
    with args.output.open("x", encoding="utf-8") as output:
        json.dump(report, output, ensure_ascii=False, indent=2)
        output.write("\n")
    return 0 if report["observation_status"] == "COMPLETE_WITHIN_DECLARED_SCOPE" else 2


if __name__ == "__main__":
    raise SystemExit(main())
