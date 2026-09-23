#!/usr/bin/env python3
"""Run S4-B2 final-closeout gates from one clean macOS commit.

The native GUI smoke is recorded separately because it needs a visible user
session. Its executable must match ``binary-sha256.txt`` written here.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]


def output(command):
    return subprocess.run(
        command,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    ).stdout.strip()


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)

    status_before = output(["git", "status", "--porcelain=v1", "--untracked-files=all"])
    (out / "clean-status-before.txt").write_text(status_before + "\n", encoding="utf-8")
    if status_before:
        raise SystemExit("S4-B2 final gates require a clean commit")
    commit = output(["git", "rev-parse", "HEAD"])
    (out / "git-head.txt").write_text(commit + "\n", encoding="utf-8")

    sys.path.insert(0, str(ROOT / "scripts"))
    import source_manifest

    tracked = set(output(["git", "ls-files"]).splitlines())
    payload = [p.relative_to(ROOT).as_posix() for p in source_manifest.source_files()]
    payload.append("MANIFEST.sha256")
    untracked = sorted(set(payload) - tracked)
    if untracked:
        raise SystemExit(f"source payload files are not tracked: {untracked[:5]}")
    tested = "".join(
        f"{sha256(ROOT / name)}  {name}\n" for name in sorted(set(payload))
    )
    (out / "tested-source-hashes.txt").write_text(tested, encoding="utf-8")

    environment = {
        "schema_version": 2,
        "stage": "S4-B2",
        "platform": platform.platform(),
        "machine": platform.machine(),
        "macos": platform.mac_ver()[0],
        "python": platform.python_version(),
        "hardware": output(["/usr/sbin/sysctl", "-n", "machdep.cpu.brand_string"]),
        "rust": output(
            [
                "env",
                "CARGO_HOME=" + str(ROOT / ".tools/cargo"),
                "RUSTUP_HOME=" + str(ROOT / ".tools/rustup"),
                str(ROOT / ".tools/cargo/bin/rustc"),
                "--version",
            ]
        ),
        "windows": "deferred / not executed",
    }
    (out / "environment.json").write_text(
        json.dumps(environment, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )

    env = os.environ.copy()
    env.update(
        PATH=str(ROOT / ".tools/cargo/bin") + os.pathsep + env["PATH"],
        CARGO_HOME=str(ROOT / ".tools/cargo"),
        RUSTUP_HOME=str(ROOT / ".tools/rustup"),
        CARGO_TARGET_DIR=str(ROOT / ".tools/target"),
        PYTHONPYCACHEPREFIX="/tmp/rcam-s4b2-pycache",
        RCAM_S4B2_BLOCK_PERF_OUT=str(out / "s4b2_block_performance.json"),
    )
    commands = [
        ["cargo", "fmt", "--all", "--", "--check"],
        ["cargo", "check", "--workspace", "--all-targets", "--locked"],
        ["cargo", "clippy", "--workspace", "--all-targets", "--locked", "--", "-D", "warnings"],
        ["cargo", "test", "--workspace", "--locked", "--no-fail-fast"],
        ["cargo", "test", "--locked", "-p", "editor-core", "--test", "block_core"],
        ["cargo", "test", "--locked", "-p", "editor-service", "--test", "block_core_workflow"],
        ["cargo", "test", "--locked", "-p", "rcam-project"],
        ["cargo", "tree", "--locked", "-p", "editor-service", "-e", "normal"],
        ["cargo", "tree", "--locked", "-p", "rcam-project", "-e", "normal"],
        ["cargo", "build", "--release", "--locked", "-p", "editor-app"],
        ["python3", "scripts/source_manifest.py", "--check"],
        ["python3", "scripts/test_audit_core10.py"],
        ["python3", "scripts/test_package_source.py"],
        ["python3", "scripts/test_package_release.py"],
        [
            "cargo", "test", "--release", "--locked", "-p", "editor-app", "native_metal",
            "--", "--ignored", "--nocapture", "--test-threads=1",
        ],
        [
            "cargo", "test", "--release", "--locked", "-p", "editor-app",
            "s4b2_block_release_performance", "--", "--ignored", "--nocapture",
            "--test-threads=1",
        ],
        [
            "cargo", "test", "--release", "--locked", "-p", "rcam-project", "--test",
            "performance_workflow", "--", "--nocapture", "--test-threads=1",
        ],
    ]
    results = []
    for number, command in enumerate(commands):
        log = out / f"{number:02d}.log"
        with log.open("w", encoding="utf-8") as stream:
            result = subprocess.run(
                command, cwd=ROOT, env=env, stdout=stream, stderr=subprocess.STDOUT
            )
        results.append({"command": command, "exit_code": result.returncode, "log": log.name})
        log.write_text(
            log.read_text(encoding="utf-8").replace(str(ROOT), "<workspace>"),
            encoding="utf-8",
        )
        print(number, result.returncode, " ".join(command), flush=True)

    binary = ROOT / ".tools/target/release/editor-app"
    if binary.is_file():
        (out / "binary-sha256.txt").write_text(
            f"{sha256(binary)}  {binary.name}\n", encoding="utf-8"
        )
    status_after = output(["git", "status", "--porcelain=v1", "--untracked-files=all"])
    (out / "clean-status-after.txt").write_text(status_after + "\n", encoding="utf-8")
    passed = not status_after and all(item["exit_code"] == 0 for item in results)
    summary = {
        "schema_version": 2,
        "stage": "S4-B2",
        "base_commit": commit,
        "source_manifest_sha256": sha256(ROOT / "MANIFEST.sha256"),
        "clean_before": not bool(status_before),
        "clean_after": not bool(status_after),
        "commands": results,
        "status": "PASS" if passed else "FAIL",
        "windows": "deferred / not executed",
    }
    (out / "gates.json").write_text(
        json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    raise SystemExit(not passed)


if __name__ == "__main__":
    main()
