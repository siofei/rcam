# Passive external RCam microcheck guard

This source-only package contains four scripts. It changes no product source or
manifest. It does not request focus, click, type, change display modes, or inspect
window contents. RCam's own existing focus request must bring its owned window
forward; the external guard only observes that result.

## Files

- `interference.swift`: main-RunLoop raw HID/session counters and last-event ages,
  plus verified owned-process readiness and foreground state. No event tap.
- `guard_policy.py`: pure, injectable input/foreground policy.
- `supervise.py`: locked-input checks, file-tail observation, owned runner control,
  bounded cleanup, and a post-join observation barrier.
- `test_guard.py`: Linux-safe policy, stream, synthetic process and wrapper tests.

Protocol version 2 prevents accidental mixing with the retired activation-capable
observer. Do not use the earlier version.

## Background checks

From this directory, run:

```sh
python3 -B -m unittest -v test_guard
python3 -B -m py_compile guard_policy.py supervise.py test_guard.py
```

These tests use synthetic processes and data. They do not start RCam, collect a
screen capture, or validate macOS Quartz/AppKit behavior.

## macOS placement and configuration

Copy the four scripts into a new, empty evidence directory alongside the existing
locked `background-gates-09` directory. The expected layout is:

```text
exports/
  background-gates-09/
    bin/editor-app-release-internal
    capture-writer-preflight/capture-producer
  fresh-microcheck/
    interference.swift
    guard_policy.py
    supervise.py
    test_guard.py
```

Set `RCAM_GUARD_ROOT` locally to the locked product checkout. No operator path,
credential, or recorded run identifier belongs in source control. For example:

```sh
RCAM_GUARD_ROOT=/absolute/path/to/locked-checkout python3 -B supervise.py
```

The supervisor verifies the fixed product manifest, runner source, app binary,
and capture-producer hashes before launch. It preserves the display policy and
runs one `workflow-reopen` round against the checkout's public synthetic fixture.
There is no automatic retry. Do not relax the hash locks to bypass a mismatch.

Do not include generated control files, receipts, process records, compilation or
observation logs, raw captures, or archives in a public commit.

## Safety boundaries

- Arm from two consecutive valid samples before starting the runner.
- Keep one continuous baseline through launch, readiness, foreground and cleanup.
- Read per-type HID and combined-session counters; use `anyInput` only for age.
- Stop on any observed HID counter/new-event change. Combined-session changes
  also stop the test and remain unattributed. Neither proves human input.
- Use API-call clock brackets and the retained 20 ms event-time tolerance.
- Validate the owned app's PID, parent, kernel start credential and executable
  path. Validate readiness against the bound run ID. No PID-only identity claim.
- Wait passively up to five seconds after owned readiness for RCam's own focus.
  After the first observed owned foreground, losing it stops the test. The guard
  never tries to regain focus.
- Fail closed on malformed, missing, stale or discontinued monitor observations.
- Allow 190 seconds of execution and 40 seconds for cleanup. Only the owned runner
  gets SIGINT; its own cleanup owns its app and producer. There is no group signal
  or unrelated-process termination. A cleanup timeout is reported as incomplete.
- Before stopping the monitor on every joined runner path, require a legal sample
  whose acquisition begins at or after the actual observed runner join time.
  A queued pre-join sample does not qualify; no fresh sample within 250 ms fails.

A supervisor success receipt is an external guard result. Product capture results
and cleanup receipts still need inspection for the intended acceptance criteria.

## Platform verification still required

Linux tests do not establish macOS acceptance. Verify compilation with the target
Swift/SDK, real idle counter/age behavior (including unseen event types), monotonic
clock compatibility, and direct-Popen AppKit identity/foreground refresh before
using this observer for a real own-window capture. Real constructor, SCStream,
first-frame, STOP, valid MOV and joined-cleanup evidence remain separate checks.
