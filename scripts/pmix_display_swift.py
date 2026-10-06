"""PMIX exact-target probe and Quartz-only session mutator; no retries."""

PROBE_SOURCE = r'''
import AppKit
import CoreGraphics
import Darwin
import Foundation

func fail(_ message: String) -> Never {
    FileHandle.standardError.write(Data(("PMIX display: " + message + "\n").utf8))
    exit(2)
}

func identifier(_ text: String, allowZero: Bool) -> UInt32 {
    guard !text.isEmpty, text.utf8.allSatisfy({ $0 >= 48 && $0 <= 57 }),
          let value = UInt32(text), allowZero || value > 0 else {
        fail("invalid unsigned identifier")
    }
    return value
}

var phaseSequence = 0
@MainActor
func phase(_ name: String, error: CGError? = nil, value: UInt32? = nil,
           selectedMode: UInt32? = nil) {
    phaseSequence += 1
    var row: [String: Any] = ["schema_version": 1, "event": "display-phase",
        "producer_pid": getpid(), "uid": getuid(), "thread_main": Thread.isMainThread,
        "clock_domain": "darwin_uptime_raw_ns", "at_ns": DispatchTime.now().uptimeNanoseconds,
        "sequence": phaseSequence, "operation": operation, "display_id": display,
        "phase": name, "cg_error": NSNull(), "value": NSNull(), "selected_mode_id": NSNull()]
    if let error { row["cg_error"] = error.rawValue }
    if let value { row["value"] = value }
    if let selectedMode { row["selected_mode_id"] = selectedMode }
    do {
        var data = try JSONSerialization.data(withJSONObject: row, options: [.sortedKeys])
        data.append(10)
        try FileHandle.standardError.write(contentsOf: data)
    } catch { fail("phase serialization/write failed") }
}

@MainActor
func coreSnapshot(_ prefix: String, selectedMode: UInt32? = nil) -> [String: Any] {
    phase(prefix + "/online-enter", selectedMode: selectedMode)
    let online = CGDisplayIsOnline(display)
    phase(prefix + "/online-return", value: UInt32(online), selectedMode: selectedMode)
    guard online != 0 else { fail("requested display unavailable; no fallback") }
    phase(prefix + "/mode-enter", selectedMode: selectedMode)
    let mode = CGDisplayCopyDisplayMode(display)
    phase(prefix + "/mode-return", value: mode.map { UInt32(bitPattern: $0.ioDisplayModeID) },
          selectedMode: selectedMode)
    guard let mode else { fail("requested display mode unavailable; no fallback") }
    phase(prefix + "/mirror-enter", selectedMode: selectedMode)
    let mirrored = CGDisplayIsInMirrorSet(display)
    phase(prefix + "/mirror-return", value: UInt32(mirrored), selectedMode: selectedMode)
    return ["display_id": display, "mode_id": UInt32(bitPattern: mode.ioDisplayModeID),
            "width": mode.width, "height": mode.height,
            "pixel_width": mode.pixelWidth, "pixel_height": mode.pixelHeight,
            "refresh_hz": mode.refreshRate, "in_mirror_set": mirrored != 0]
}

@MainActor
func emit(_ before: [String: Any], _ after: [String: Any], selectedMode: UInt32? = nil) {
    do {
        let data = try JSONSerialization.data(withJSONObject: ["before": before, "after": after], options: [.sortedKeys])
        print(String(decoding: data, as: UTF8.self))
        phase("receipt-return", selectedMode: selectedMode)
    } catch { fail("snapshot serialization failed") }
}

let arguments = CommandLine.arguments
guard arguments.count == 3 && arguments[1] == "probe" else {
    fail("expected probe and display_id")
}
let operation = arguments[1]
let display = CGDirectDisplayID(identifier(arguments[2], allowZero: false))
phase("runtime-enter")
phase("appkit-enter")
let application = NSApplication.shared
if application.activationPolicy() != .prohibited {
    _ = application.setActivationPolicy(.prohibited)
}
guard application.activationPolicy() == .prohibited else {
    fail("non-activating helper initialization failed")
}
phase("appkit-return", value: UInt32(application.activationPolicy().rawValue))

@MainActor
func snapshot(_ prefix: String) -> [String: Any] {
    var value = coreSnapshot(prefix)
    phase(prefix + "/screen-enter")
    guard let screen = NSScreen.screens.first(where: {
        ($0.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber)?.uint32Value == display
    }) else { fail("requested display has no NSScreen mapping; no fallback") }
    value["backing_scale"] = screen.backingScaleFactor
    phase(prefix + "/screen-return")
    return value
}
let before = snapshot("snapshot-before")
let after = snapshot("snapshot-after")
emit(before, after)
'''

MUTATOR_SOURCE = r'''
import CoreGraphics
import Darwin
import Foundation

var pendingConfiguration: CGDisplayConfigRef? = nil
@MainActor
func fail(_ message: String) -> Never {
    // Phase I/O failure must not strand a still-valid transaction. No recursive logging.
    if let configuration = pendingConfiguration {
        pendingConfiguration = nil
        _ = CGCancelDisplayConfiguration(configuration)
    }
    FileHandle.standardError.write(Data(("PMIX display: " + message + "\n").utf8))
    exit(2)
}

@MainActor
func identifier(_ text: String, allowZero: Bool) -> UInt32 {
    guard !text.isEmpty, text.utf8.allSatisfy({ $0 >= 48 && $0 <= 57 }),
          let value = UInt32(text), allowZero || value > 0 else {
        fail("invalid unsigned identifier")
    }
    return value
}

var phaseSequence = 0
@MainActor
func phase(_ name: String, error: CGError? = nil, value: UInt32? = nil,
           selectedMode: UInt32? = nil) {
    phaseSequence += 1
    var row: [String: Any] = ["schema_version": 1, "event": "display-phase",
        "producer_pid": getpid(), "uid": getuid(), "thread_main": Thread.isMainThread,
        "clock_domain": "darwin_uptime_raw_ns", "at_ns": DispatchTime.now().uptimeNanoseconds,
        "sequence": phaseSequence, "operation": operation, "display_id": display,
        "phase": name, "cg_error": NSNull(), "value": NSNull(), "selected_mode_id": NSNull()]
    if let error { row["cg_error"] = error.rawValue }
    if let value { row["value"] = value }
    if let selectedMode { row["selected_mode_id"] = selectedMode }
    do {
        var data = try JSONSerialization.data(withJSONObject: row, options: [.sortedKeys])
        data.append(10)
        try FileHandle.standardError.write(contentsOf: data)
    } catch { fail("phase serialization/write failed") }
}

@MainActor
func coreSnapshot(_ prefix: String, selectedMode: UInt32? = nil) -> [String: Any] {
    phase(prefix + "/online-enter", selectedMode: selectedMode)
    let online = CGDisplayIsOnline(display)
    phase(prefix + "/online-return", value: UInt32(online), selectedMode: selectedMode)
    guard online != 0 else { fail("requested display unavailable; no fallback") }
    phase(prefix + "/mode-enter", selectedMode: selectedMode)
    let mode = CGDisplayCopyDisplayMode(display)
    phase(prefix + "/mode-return", value: mode.map { UInt32(bitPattern: $0.ioDisplayModeID) },
          selectedMode: selectedMode)
    guard let mode else { fail("requested display mode unavailable; no fallback") }
    phase(prefix + "/mirror-enter", selectedMode: selectedMode)
    let mirrored = CGDisplayIsInMirrorSet(display)
    phase(prefix + "/mirror-return", value: UInt32(mirrored), selectedMode: selectedMode)
    return ["display_id": display, "mode_id": UInt32(bitPattern: mode.ioDisplayModeID),
            "width": mode.width, "height": mode.height,
            "pixel_width": mode.pixelWidth, "pixel_height": mode.pixelHeight,
            "refresh_hz": mode.refreshRate, "in_mirror_set": mirrored != 0]
}

@MainActor
func emit(_ before: [String: Any], _ after: [String: Any], selectedMode: UInt32? = nil) {
    do {
        let data = try JSONSerialization.data(withJSONObject: ["before": before, "after": after], options: [.sortedKeys])
        print(String(decoding: data, as: UTF8.self))
        phase("receipt-return", selectedMode: selectedMode)
    } catch { fail("snapshot serialization failed") }
}

let arguments = CommandLine.arguments
guard arguments.count >= 3 else { fail("expected operation and display_id") }
let operation = arguments[1]
guard (operation == "set60" && arguments.count == 3)
        || (operation == "restore" && arguments.count == 4) else {
    fail("invalid mutation operation or argument count")
}
let display = CGDirectDisplayID(identifier(arguments[2], allowZero: false))
let restoreMode = operation == "restore" ? identifier(arguments[3], allowZero: true) : nil
phase("runtime-enter")
let before = coreSnapshot("snapshot-before")
guard before["in_mirror_set"] as? Bool == false else {
    fail("requested display is mirrored; refusing linked mode changes")
}
phase("modes-enter")
let modes = CGDisplayCopyAllDisplayModes(display,
    [kCGDisplayShowDuplicateLowResolutionModes: true] as CFDictionary) as? [CGDisplayMode]
phase("modes-return", value: modes.map { UInt32($0.count) })
guard let modes else { fail("requested display modes unavailable; no fallback") }
let selected = modes.first(where: { mode in
    if let restoreMode { return UInt32(bitPattern: mode.ioDisplayModeID) == restoreMode }
    return mode.width == before["width"] as? Int && mode.height == before["height"] as? Int
        && mode.pixelWidth == before["pixel_width"] as? Int && mode.pixelHeight == before["pixel_height"] as? Int
        && abs(mode.refreshRate - 60) < 0.01
})
guard let mode = selected else { fail("requested target mode unavailable; no fallback") }
let selectedMode = UInt32(bitPattern: mode.ioDisplayModeID)
phase("mode-selected", value: selectedMode, selectedMode: selectedMode)
var configuration: CGDisplayConfigRef? = nil
phase("begin-enter", selectedMode: selectedMode)
let began = CGBeginDisplayConfiguration(&configuration)
if began == .success { pendingConfiguration = configuration }
phase("begin-return", error: began, selectedMode: selectedMode)
guard began == .success, let configuration else { fail("target session configuration could not begin") }
phase("configure-enter", selectedMode: selectedMode)
let configured = CGConfigureDisplayWithDisplayMode(configuration, display, mode, nil)
phase("configure-return", error: configured, selectedMode: selectedMode)
if configured != .success {
    phase("cancel-enter", selectedMode: selectedMode)
    let cancelled = CGCancelDisplayConfiguration(configuration)
    pendingConfiguration = nil
    phase("cancel-return", error: cancelled, selectedMode: selectedMode)
    fail("target session mode configuration failed; cancel result \(cancelled.rawValue)")
}
// Complete consumes the token on return, including error. Never cancel it twice.
phase("complete-enter", selectedMode: selectedMode)
let completed = CGCompleteDisplayConfiguration(configuration, .forSession)
pendingConfiguration = nil
phase("complete-return", error: completed, selectedMode: selectedMode)
guard completed == .success else {
    fail("target session mode commit failed; restoration remains required")
}
let after = coreSnapshot("snapshot-after", selectedMode: selectedMode)
emit(before, after, selectedMode: selectedMode)
'''
