"""PMIX-only explicit target display helper; never resolve the current main screen."""

SOURCE = r'''
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

let arguments = CommandLine.arguments
guard arguments.count >= 3 else { fail("expected operation and display_id") }
let operation = arguments[1]
guard ((operation == "probe" || operation == "set60") && arguments.count == 3)
        || (operation == "restore" && arguments.count == 4) else {
    fail("invalid operation or argument count")
}
let display = CGDirectDisplayID(identifier(arguments[2], allowZero: false))
let restoreMode = operation == "restore" ? identifier(arguments[3], allowZero: true) : nil
let application = NSApplication.shared
guard application.setActivationPolicy(.prohibited) else {
    fail("non-activating helper initialization failed")
}

func snapshot() -> [String: Any] {
    guard CGDisplayIsOnline(display) != 0,
          let mode = CGDisplayCopyDisplayMode(display) else {
        fail("requested display unavailable; no fallback")
    }
    guard let screen = NSScreen.screens.first(where: {
        ($0.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber)?.uint32Value == display
    }) else { fail("requested display has no NSScreen mapping; no fallback") }
    return ["display_id": display, "mode_id": UInt32(bitPattern: mode.ioDisplayModeID),
            "width": mode.width, "height": mode.height,
            "pixel_width": mode.pixelWidth, "pixel_height": mode.pixelHeight,
            "refresh_hz": mode.refreshRate, "backing_scale": screen.backingScaleFactor,
            "in_mirror_set": CGDisplayIsInMirrorSet(display) != 0]
}

let before = snapshot()
if operation != "probe" {
    guard CGDisplayIsInMirrorSet(display) == 0 else {
        fail("requested display is mirrored; refusing linked mode changes")
    }
    guard CGDisplayIsOnline(display) != 0,
          let current = CGDisplayCopyDisplayMode(display),
          let modes = CGDisplayCopyAllDisplayModes(display,
            [kCGDisplayShowDuplicateLowResolutionModes: true] as CFDictionary) as? [CGDisplayMode] else {
        fail("requested display modes unavailable; no fallback")
    }
    let selected = modes.first(where: { mode in
        if let restoreMode { return UInt32(bitPattern: mode.ioDisplayModeID) == restoreMode }
        return mode.width == current.width && mode.height == current.height
            && mode.pixelWidth == current.pixelWidth && mode.pixelHeight == current.pixelHeight
            && abs(mode.refreshRate - 60) < 0.01
    })
    guard let mode = selected else { fail("requested target mode unavailable; no fallback") }
    var configuration: CGDisplayConfigRef? = nil
    guard CGBeginDisplayConfiguration(&configuration) == .success,
          let configuration else { fail("target session configuration could not begin") }
    guard CGConfigureDisplayWithDisplayMode(configuration, display, mode, nil) == .success else {
        let cancelled = CGCancelDisplayConfiguration(configuration)
        fail("target session mode configuration failed; cancel result \(cancelled.rawValue)")
    }
    // Complete consumes the token on return, including error. Never cancel it twice.
    guard CGCompleteDisplayConfiguration(configuration, .forSession) == .success else {
        fail("target session mode commit failed; restoration remains required")
    }
}
let result = ["before": before, "after": snapshot()]
do {
    let data = try JSONSerialization.data(withJSONObject: result, options: [.sortedKeys])
    print(String(decoding: data, as: UTF8.self))
} catch { fail("snapshot serialization failed") }
'''
