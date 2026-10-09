#!/usr/bin/env python3
"""Offline, loss-aware source endpoint classification; never a presentation/FPS verdict."""
import argparse
import hashlib
import json
import math
from pathlib import Path


def ns(value):
    if not isinstance(value, str) or not value.isascii() or not value.isdecimal():
        raise ValueError("source time must be an integer decimal string")
    result = int(value)
    if result >= 2**64:
        raise ValueError("source time exceeds u64")
    return result


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


def summarize(rows, supervised_ack=False, actual_boundaries=None):
    headers = [row for row in rows if row.get("kind") == "header"]
    if len(headers) != 1 or headers[0].get("schema_version") != 1:
        raise ValueError("expected exactly one supported header")
    header = headers[0]
    issues = []
    footers = [row for row in rows if row.get("kind") == "footer"]
    footer = footers[0] if len(footers) == 1 else None
    if footer is None or not footer.get("finalized"):
        issues.append("missing_or_failed_finalization")
    if footer is None or not footer.get("observation_complete"):
        issues.append("observation_incomplete")
    if not supervised_ack:
        issues.append("matching_supervisor_flush_ack_not_verified")
    sequenced = [row for row in rows if "record_seq" in row]
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

    planned = windows_report(header["windows"], "predefined_config")
    actual = None
    if actual_boundaries is not None:
        if (header["clock"]["clock_id"] != "mach_absolute_time_ns"
                or actual_boundaries.get("clock_id") != header["clock"]["clock_id"]
                or actual_boundaries.get("run_id") != header["run_id"]
                or actual_boundaries.get("clock_mapping_verified") is not True):
            raise ValueError("external dispatch boundaries require verified same-run source clock mapping")
        actual = windows_report(actual_boundaries["windows"], "external_actual_first_last_dispatch")
    if any(report["issues"] for report in planned + (actual or [])):
        issues.append("window_measurement_incomplete")
    return {"schema_version": 1, "run_id": header["run_id"], "identity": header,
            "observation_status": "INCOMPLETE" if issues else "COMPLETE_WITHIN_DECLARED_SCOPE",
            "performance_status": "OPEN", "issues": sorted(set(issues)), "sequence_gaps": missing,
            "physical_row_order_differs_from_sequence": sequences != sorted(sequences),
            "footer": footer, "planned_windows": planned, "actual_dispatch_windows": actual,
            "updates": [{"start": row, "end": ends.get(update_id), "span": update_spans.get(update_id)}
                        for update_id, row in sorted(starts.items())],
            "input_batches": [row for row in sequenced if row.get("kind") == "input_counts"],
            "versions": [row["metadata"] for row in sequenced if row.get("metadata", {}).get("kind") == "version"],
            "render_sources": [row for row in sequenced if row.get("kind") == "render_source"],
            "callback_enqueued": [row for row in sequenced if row.get("kind") == "callback_enqueued"],
            "external_dispatch_events": actual_boundaries.get("events") if actual_boundaries else None,
            "input_association": "App input batches map to update/pass; exact external event->batch unavailable",
            "render_association": "logical callback encoding only; actual submission/GPU completion/present unavailable"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("trace", type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--supervisor-log", type=Path)
    parser.add_argument("--actual-boundaries", type=Path)
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
    report = summarize(rows, ack, actual)
    if parse_errors:
        report["issues"].append("truncated_or_invalid_source_rows")
        report["observation_status"] = "INCOMPLETE"
    report["parse_errors"] = parse_errors
    report["trace_sha256"] = hashlib.sha256(raw).hexdigest()
    report["supervisor_log_sha256"] = hashlib.sha256(args.supervisor_log.read_bytes()).hexdigest() if args.supervisor_log else None
    report["actual_boundaries_sha256"] = hashlib.sha256(args.actual_boundaries.read_bytes()).hexdigest() if args.actual_boundaries else None
    with args.output.open("x", encoding="utf-8") as output:
        json.dump(report, output, ensure_ascii=False, indent=2)
        output.write("\n")
    return 0 if report["observation_status"] == "COMPLETE_WITHIN_DECLARED_SCOPE" else 2


if __name__ == "__main__":
    raise SystemExit(main())
