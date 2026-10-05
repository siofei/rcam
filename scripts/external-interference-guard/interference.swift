// v2 passive RCam observer. No external focus request or GUI operation.
// Python alone judges raw input/foreground observations.
import AppKit
import CoreGraphics
import Darwin
import Foundation

struct Control: Decodable, Equatable {
    let protocolVersion: Int
    let nonce: String
    let commandID: Int
    let runnerPID: Int32
    let appPID: Int32
    let binaryPath: String?
    let native: String?
    let runID: String?
}
struct Credential: Equatable {
    let pid: UInt32
    let parent: UInt32
    let seconds: UInt64
    let micros: UInt64
    var json: [String: Any] {
        ["pid": pid, "parent_pid": parent, "start_seconds": seconds, "start_micros": micros]
    }
}
func monotonicNowNS() -> UInt64 { DispatchTime.now().uptimeNanoseconds }
func credential(_ pid: Int32) -> Credential? {
    var info = proc_bsdinfo()
    let size = Int32(MemoryLayout<proc_bsdinfo>.stride)
    guard proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, &info, size) == size else { return nil }
    return Credential(pid: info.pbi_pid, parent: info.pbi_ppid,
                      seconds: info.pbi_start_tvsec, micros: info.pbi_start_tvusec)
}
func executable(_ pid: Int32) -> String? {
    // SDK PROC_PIDPATHINFO_MAXSIZE is (4 * MAXPATHLEN); its expression macro
    // is unavailable to Swift. Preserve that exact capacity, without a fallback.
    let capacity = 4 * Int(MAXPATHLEN)
    var buffer = [CChar](repeating: 0, count: capacity)
    let result = buffer.withUnsafeMutableBytes {
        proc_pidpath(pid, $0.baseAddress!, UInt32($0.count))
    }
    guard result > 0 else { return nil }
    return buffer.withUnsafeBufferPointer { String(cString: $0.baseAddress!) }
}
func canonical(_ path: String) -> String {
    URL(fileURLWithPath: path).resolvingSymlinksInPath().standardizedFileURL.path
}
func noLater(_ first: Credential, _ second: Credential) -> Bool {
    first.seconds < second.seconds || (first.seconds == second.seconds && first.micros <= second.micros)
}
let inventory: [(String, CGEventType)] = [
    ("mouseMoved", .mouseMoved), ("leftMouseDown", .leftMouseDown),
    ("leftMouseUp", .leftMouseUp), ("rightMouseDown", .rightMouseDown),
    ("rightMouseUp", .rightMouseUp), ("keyDown", .keyDown), ("keyUp", .keyUp),
    ("flagsChanged", .flagsChanged), ("scrollWheel", .scrollWheel),
    ("leftMouseDragged", .leftMouseDragged), ("rightMouseDragged", .rightMouseDragged),
    ("otherMouseDown", .otherMouseDown), ("otherMouseUp", .otherMouseUp),
    ("otherMouseDragged", .otherMouseDragged), ("tabletPointer", .tabletPointer),
    ("tabletProximity", .tabletProximity)
]
func source(_ state: CGEventSourceStateID) -> [String: Any] {
    var rows: [String: Any] = [:]
    for (name, eventType) in inventory {
        let begin = monotonicNowNS()
        let before = CGEventSource.counterForEventType(state, eventType: eventType)
        let ageBegin = monotonicNowNS()
        let age = CGEventSource.secondsSinceLastEventType(state, eventType: eventType)
        let ageEnd = monotonicNowNS()
        let after = CGEventSource.counterForEventType(state, eventType: eventType)
        let end = monotonicNowNS()
        // Quartz may use an infinite/DBL_MAX sentinel for an unseen event type.
        let available = age.isFinite && age >= 0 && age <= Double(end) / 1e9 + 1
        rows[name] = ["begin_ns": begin, "age_begin_ns": ageBegin,
                      "age_end_ns": ageEnd, "end_ns": end,
                      "count_before": before, "count_after": after,
                      "age_seconds": available ? (age as Any) : NSNull()]
    }
    // ~0 is documented only for age, never used as an aggregate counter.
    let begin = monotonicNowNS()
    let ageBegin = monotonicNowNS()
    let age = CGEventSource.secondsSinceLastEventType(state, eventType: CGEventType(rawValue: UInt32.max)!)
    let ageEnd = monotonicNowNS()
    let end = monotonicNowNS()
    let available = age.isFinite && age >= 0 && age <= Double(end) / 1e9 + 1
    rows["anyInput"] = ["begin_ns": begin, "age_begin_ns": ageBegin,
                        "age_end_ns": ageEnd, "end_ns": end,
                        "age_seconds": available ? (age as Any) : NSNull()]
    return rows
}

guard CommandLine.arguments.count == 3 else { exit(64) }
let controlURL = URL(fileURLWithPath: CommandLine.arguments[1])
let nonce = CommandLine.arguments[2]
var current: Control?
var lastCommand = -1
var runnerCredential: Credential?
var ownedCredential: Credential?
var ownedApplication: NSRunningApplication?
var sequence = 0
@MainActor
func emit(_ value: [String: Any]) {
    var envelope = value
    envelope["protocol_version"] = 2
    envelope["nonce"] = nonce
    do {
        var data = try JSONSerialization.data(withJSONObject: envelope, options: [.sortedKeys])
        data.append(0x0A)
        try FileHandle.standardOutput.write(contentsOf: data)
    } catch { exit(74) }
}
@MainActor
func fatal(_ reason: String) -> Never {
    emit(["event": "fatal", "reason": reason, "at_ns": monotonicNowNS(), "thread_main": Thread.isMainThread])
    exit(2)
}
@MainActor
func appMatches(_ app: NSRunningApplication, _ expected: Control) -> Bool {
    guard let path = app.executableURL?.path, let wanted = expected.binaryPath else { return false }
    return app.processIdentifier == expected.appPID && canonical(path) == canonical(wanted)
}
@MainActor
func identityAlive(_ expected: Control) -> Bool {
    guard let original = ownedCredential else { return false }
    guard let live = credential(expected.appPID) else { return false }
    guard live == original, let path = executable(expected.appPID),
          let wanted = expected.binaryPath, canonical(path) == canonical(wanted) else {
        fatal("OWNED_KERNEL_IDENTITY_CHANGED")
    }
    if let fresh = NSRunningApplication(processIdentifier: expected.appPID) {
        guard appMatches(fresh, expected) else { fatal("OWNED_APPKIT_IDENTITY_CHANGED") }
        if fresh.isTerminated { return false }
        if let retained = ownedApplication {
            guard retained.isEqual(fresh) else { fatal("OWNED_APPKIT_OBJECT_CHANGED") }
        } else { ownedApplication = fresh }
    }
    return true
}
@MainActor
func applyControl() {
    let next: Control
    do { next = try JSONDecoder().decode(Control.self, from: Data(contentsOf: controlURL)) }
    catch { fatal("INVALID_OR_MISSING_CONTROL") }
    guard next.protocolVersion == 2, next.nonce == nonce, next.commandID >= lastCommand,
          next.runnerPID >= 0, next.appPID >= 0 else { fatal("CONTROL_PROTOCOL") }
    if next.commandID == lastCommand {
        guard next == current else { fatal("CONTROL_CHANGED_WITHOUT_COMMAND") }
        return
    }
    if let old = current {
        guard next.commandID > old.commandID,
              old.runnerPID == 0 || next.runnerPID == old.runnerPID,
              old.appPID == 0 || (next.appPID == old.appPID && next.binaryPath == old.binaryPath && next.native == old.native && next.runID == old.runID)
        else { fatal("CONTROL_BINDING_CHANGED") }
    }
    if next.runnerPID > 0 && runnerCredential == nil {
        guard let found = credential(next.runnerPID) else { fatal("RUNNER_CREDENTIAL_UNAVAILABLE") }
        runnerCredential = found
        emit(["event": "runner_bound", "credential": found.json, "at_ns": monotonicNowNS()])
    }
    if next.appPID > 0 && ownedCredential == nil {
        guard let runner = runnerCredential, let liveRunner = credential(next.runnerPID),
              runner == liveRunner, let owned = credential(next.appPID),
              owned.parent == UInt32(next.runnerPID), noLater(runner, owned),
              let path = executable(next.appPID), let wanted = next.binaryPath,
              canonical(path) == canonical(wanted), let native = next.native,
              URL(fileURLWithPath: native).lastPathComponent.hasPrefix("rcam-pmix-"),
              URL(fileURLWithPath: canonical(native)).deletingLastPathComponent().path == canonical("/tmp"),
              next.runID != nil
        else { fatal("OWNED_LAUNCH_IDENTITY_UNVERIFIED") }
        ownedCredential = owned
        emit(["event": "owned_bound", "credential": owned.json,
              "executable_path": canonical(path), "at_ns": monotonicNowNS()])
    }
    current = next
    lastCommand = next.commandID

}
struct ReadyIdentity: Decodable {
    let app_pid: Int32
    let run_id: String
}
@MainActor
func readyMarker(_ native: String, _ expected: Control) -> Bool {
    let url = URL(fileURLWithPath: native + "/window-ready.json")
    guard FileManager.default.fileExists(atPath: url.path) else { return false }
    do {
        let marker = try JSONDecoder().decode(ReadyIdentity.self, from: Data(contentsOf: url))
        guard marker.app_pid == expected.appPID && marker.run_id == expected.runID else {
            fatal("OWNED_READY_MARKER_IDENTITY")
        }
        return true
    } catch { fatal("INVALID_OWNED_READY_MARKER") }
}
@MainActor
func sample() {
    autoreleasepool {
        applyControl()
        guard let expected = current else { fatal("NO_CONTROL") }
        let begin = monotonicNowNS()
        if let runner = runnerCredential, let live = credential(expected.runnerPID), runner != live {
            fatal("RUNNER_KERNEL_IDENTITY_CHANGED")
        }
        let sources: [String: Any] = ["hid": source(.hidSystemState), "combined": source(.combinedSessionState)]
        var alive = false, ready = false, frontOwned = false, complete = false
        if expected.appPID > 0 {
            alive = identityAlive(expected)
            if let native = expected.native {
                complete = FileManager.default.fileExists(atPath: native + "/capture-complete.json")
                ready = alive && ownedApplication != nil && readyMarker(native, expected)
            }
            if alive, let app = ownedApplication {
                frontOwned = NSWorkspace.shared.frontmostApplication?.isEqual(app) == true
            }
        }
        let end = monotonicNowNS()
        sequence += 1
        emit(["event": "sample", "seq": sequence, "begin_ns": begin, "end_ns": end,
              "thread_main": Thread.isMainThread, "runner_pid": expected.runnerPID,
              "owned_pid": expected.appPID, "identity_verified": ownedCredential != nil,
              "owned_alive": alive, "owned_ready": ready, "front_owned": frontOwned,
              "capture_complete": complete, "sources": sources])
    }
}
// Timer turns give NSRunningApplication's dynamic properties a main-RunLoop refresh.
emit(["event": "ready", "monitor_pid": getpid(), "thread_main": Thread.isMainThread, "at_ns": monotonicNowNS()])
let timer = Timer(timeInterval: 0.05, repeats: true) { _ in MainActor.assumeIsolated { sample() } }
RunLoop.main.add(timer, forMode: .common)
RunLoop.main.run()
