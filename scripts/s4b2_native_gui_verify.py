#!/usr/bin/env python3
"""Validate the opt-in S4-B2 native GUI Block smoke evidence."""
import argparse
import json
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("probe", type=Path)
    args = parser.parse_args()
    probe = args.probe.resolve()
    observations = [
        json.loads(line)
        for line in (probe / "native_observations.jsonl").read_text(encoding="utf-8").splitlines()
        if line.strip()
    ]
    actions = (probe / "native_actions.log").read_text(encoding="utf-8").splitlines()
    screens = {path.stem for path in (probe / "screens").glob("*.ppm")}

    fixture = [
        item
        for item in observations
        if any(layer["name"] == "block-fixture" and layer["objects"] == 6 for layer in item["layers"])
    ]
    reopened = [
        item
        for item in observations
        if any(
            layer["name"] == "s4b2-block-export" and layer["objects"] == 24
            for layer in item["layers"]
        )
    ]
    modes = {
        layer["display_mode"]
        for item in fixture
        for layer in item["layers"]
        if layer["name"] == "block-fixture"
    }
    colors = {
        layer["color"]
        for item in fixture
        for layer in item["layers"]
        if layer["name"] == "block-fixture"
    }
    category_color = any(
        layer["color_mode"] == "CategoryColor"
        for item in fixture
        for layer in item["layers"]
        if layer["name"] == "block-fixture"
    )
    selected = any(item.get("selected_block_instances", 0) == 1 for item in fixture)
    healthy = all(
        any(
            item.get("display_error") is None
            and item.get("blocked") is None
            and item.get("has_last_good_frame")
            and item.get("service_error") is None
            for item in phase
        )
        for phase in (fixture, reopened)
    )
    exported = any("ExportLayer" in line and "s4b2-block-export.gbr" in line for line in actions)
    reimported = any("ImportGerbers" in line and "s4b2-block-export.gbr" in line for line in actions)
    required_screens = {
        "block-filled",
        "block-outline",
        "block-zerowidth",
        "block-selected",
        "reopened-export",
    }
    checks = {
        "fixture_1_definition_5_added_instances_plus_original": bool(fixture),
        "filled_outline_zerowidth": {"Filled", "Outline", "ZeroWidth"} <= modes,
        "layer_color_changed": len(colors) >= 2,
        "category_color": category_color,
        "whole_block_selected": selected,
        "exported_gerber": exported,
        "reopened_flattened_export": bool(reopened) and reimported,
        "no_blank_or_display_error": healthy,
        "screenshots": required_screens <= screens,
    }
    report = {
        "schema_version": 2,
        "stage": "S4-B2",
        "status": "PASS" if all(checks.values()) else "FAIL",
        "checks": checks,
        "modes_seen": sorted(modes),
        "layer_colors_seen": sorted(colors),
        "screenshots": sorted(screens),
        "observation_count": len(observations),
        "action_count": len(actions),
        "windows": "deferred / not executed",
    }
    destination = probe / "native-gui-smoke.json"
    destination.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2, sort_keys=True))
    raise SystemExit(report["status"] != "PASS")


if __name__ == "__main__":
    main()
