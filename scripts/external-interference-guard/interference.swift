// v2 passive RCam observer. No external focus request or GUI operation.
// Python alone judges raw input/foreground observations.
import AppKit
import CoreGraphics
import Darwin
import Foundation
import os

struct Control: Decodable, Equatable, Sendable {
    let protocolVersion: Int
    let nonce: String
    let commandID: Int
    let runnerPID: Int32
    let appPID: Int32
    let binaryPath: String?
    let native: String?
    let runID: String?
}
struct Credential: Equatable, Sendable {
    let pid: UInt32
    let parent: UInt32
    let seconds: UInt64
    let micros: UInt64
}
struct ReadyIdentity: Decodable, Sendable {
    let app_pid: Int32
    let run_id: String
}


// Diagnostic-only values cross the FIFO. Original JSON/raw IO stays on MainActor.
private struct FieldBuild: Sendable, Encodable {
    let source: String, event_type: String
    let build_enter_ns: UInt64, build_return_ns: UInt64
}
private struct EmitFact: Sendable, Encodable {
    let raw_record_ordinal: Int, event: String
    let sample_seq: Int?
    var encode_enter_ns: UInt64?, encode_return_ns: UInt64?
    var write_enter_ns: UInt64?, write_return_ns: UInt64?
    var expected_bytes: Int?, offset_before: UInt64?, offset_after: UInt64?
    var state: String = "NOT_STARTED"
    var error: String?
}
private struct CycleFact: Sendable, Encodable {
    let cycle_id: Int, sample_seq: Int, timer_enter_ns: UInt64
    var control_enter_ns: UInt64?, control_return_ns: UInt64?
    var sample_begin_ns: UInt64?, sample_end_ns: UInt64?
    var field_build: [FieldBuild] = []
    var timer_exit_ns: UInt64?
    let previous_emit: EmitFact?
}
private struct TraceHeader: Sendable, Encodable {
    let source_sha256: String, executable_sha256: String
    let queue_limit = 256, memory_limit_bytes = 1048576
    let sidecar_limit_bytes = 16777216, line_limit_bytes = 8192
    let raw_limit_bytes = 67108864
}
private enum TracePayload: Sendable, Encodable {
    case header(TraceHeader), cycle_enter(CycleFact), cycle(CycleFact)
    case emit_start(EmitFact), emit_progress(EmitFact), emit_return(EmitFact), terminal(CycleFact?)
    var retainedFieldBytes: Int {
        switch self {
        case .cycle_enter(let value), .cycle(let value):
            return value.field_build.capacity * MemoryLayout<FieldBuild>.stride
        case .terminal(let value):
            return (value?.field_build.capacity ?? 0) * MemoryLayout<FieldBuild>.stride
        default: return 0
        }
    }
}
private struct TraceRow: Sendable, Encodable {
    let schema_version = 1
    let clock_domain = "darwin_uptime_raw_ns"
    let nonce: String, role: String
    let pid: Int32, sequence: Int
    let payload: TracePayload
}
// At most 256 typed records. Actual queue/field Array capacity is charged
// against 768KiB, leaving 256KiB for the current cycle, encoder and <=8KiB line.
// Try-lock submission never waits for encoder/filesystem work.
private protocol DiagnosticSink: Sendable {
    func submit(_ value: TracePayload) -> Bool
    func flush()
}
private struct TraceSeal: Sendable, Encodable {
    struct Counts: Sendable, Encodable {
        let records_before_seal: Int, bytes_before_seal: Int
    }
    let schema_version = 1
    let clock_domain = "darwin_uptime_raw_ns"
    let nonce: String, role: String
    let pid: Int32, sequence: Int
    let seal: Counts
}
@available(macOS 13.0, *)
private final class DiagnosticFIFO: DiagnosticSink {
    private struct State: Sendable {
        var queue: [TracePayload] = []
        var failed = false, closing = false
        var retainedFieldBytes = 0
    }
    private let state = OSAllocatedUnfairLock(initialState: State())
    private let wake = DispatchSemaphore(value: 0), finished = DispatchSemaphore(value: 0)
    private let fd: Int32, nonce: String, role: String
    init(path: String, nonce: String) {
        func environment(_ name: String, limit: Int) -> String? {
            guard let value = getenv(name), strnlen(value, limit + 1) <= limit else { return nil }
            return String(cString: value)
        }
        guard UUID(uuidString: nonce)?.uuidString.lowercased() == nonce else { exit(64) }
        guard let role = environment("RCAM_DIAG_ROLE", limit: 10), ["continuous", "owned"].contains(role),
              let source = environment("RCAM_DIAG_SOURCE_SHA", limit: 64),
              let executable = environment("RCAM_DIAG_EXECUTABLE_SHA", limit: 64),
              [source, executable].allSatisfy({ $0.count == 64 && $0.allSatisfy({ "0123456789abcdef".contains($0) }) })
        else { exit(64) }
        self.role = role; self.nonce = nonce
        fd = open(path, O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW, S_IRUSR | S_IWUSR)
        guard fd >= 0 else { exit(74) }
        state.withLock { $0.queue.reserveCapacity(256) }
        guard submit(.header(TraceHeader(source_sha256: source, executable_sha256: executable))) else { exit(74) }
        Thread { [self] in work() }.start()
    }
    func submit(_ value: TracePayload) -> Bool {
        let accepted = state.withLockIfAvailable { state in
            guard !state.failed, !state.closing, state.queue.count < 256,
                  state.queue.capacity * MemoryLayout<TracePayload>.stride +
                    state.retainedFieldBytes + value.retainedFieldBytes <= 786432 else { return false }
            state.queue.append(value)
            state.retainedFieldBytes += value.retainedFieldBytes
            if case .terminal = value { state.closing = true }
            return true
        } ?? false
        if accepted { wake.signal() }
        return accepted
    }
    func flush() { _ = finished.wait(timeout: .now() + .milliseconds(200)) }
    private func work() {
        var bytes = 0, count = 0
        defer { close(fd); finished.signal() }
        func output(_ data: Data) throws {
            guard data.count <= 8192, bytes + data.count <= 16777216 else {
                throw NSError(domain: "diagnostic capacity", code: 74)
            }
            try data.withUnsafeBytes { raw in
                var offset = 0
                while offset < raw.count {
                    let n = Darwin.write(fd, raw.baseAddress!.advanced(by: offset), raw.count - offset)
                    if n < 0 && errno == EINTR { continue }
                    guard n > 0 else { throw NSError(domain: "diagnostic write", code: Int(errno)) }
                    offset += n
                }
            }
            bytes += data.count
        }
        do {
            while true {
                wake.wait()
                let value = state.withLock { state in
                    let value = state.queue.removeFirst()
                    state.retainedFieldBytes -= value.retainedFieldBytes
                    return value
                }
                count += 1
                var data = try JSONEncoder().encode(TraceRow(nonce: nonce, role: role, pid: getpid(), sequence: count, payload: value))
                data.append(10)
                try output(data)
                if case .terminal = value {
                    // Seal is written only after every submitted row reached write return.
                    let seal = TraceSeal(nonce: nonce, role: role, pid: getpid(), sequence: count + 1,
                        seal: .init(records_before_seal: count, bytes_before_seal: bytes))
                    var last = try JSONEncoder().encode(seal)
                    last.append(10); try output(last); return
                }
            }
        } catch {
            state.withLock { $0.failed = true }
            // Only this observer is owned. No stderr hook or main-thread IO.
            _ = kill(getpid(), SIGTERM)
        }
    }
}

@MainActor
private final class InterferenceObserver {
    private let controlURL: URL
    private let nonce: String
    private var current: Control?
    private var lastCommand = -1
    private var runnerCredential: Credential?
    private var ownedCredential: Credential?
    private var ownedApplication: NSRunningApplication?
    private var sequence = 0
    private let diagnostic: (any DiagnosticSink)?
    private var cycle: CycleFact?
    private var previousEmit: EmitFact?
    private var rawOrdinal = 0
    private var rawBytes: UInt64 = 0
    private var termination: DispatchSourceSignal?
    private var samplingTimer: Timer?
    private func trace(_ value: TracePayload) {
        if let diagnostic, !diagnostic.submit(value) { exit(74) }
    }

    init(controlURL: URL, nonce: String, sidecar: String?) {
        self.controlURL = controlURL
        self.nonce = nonce
        if let sidecar {
            if #available(macOS 13.0, *) { diagnostic = DiagnosticFIFO(path: sidecar, nonce: nonce) }
            else { exit(64) }
        } else { diagnostic = nil }
    }

    private func credentialJSON(_ value: Credential) -> [String: Any] {
        ["pid": value.pid, "parent_pid": value.parent,
         "start_seconds": value.seconds, "start_micros": value.micros]
    }
    // Dispatch uptime is Mach absolute time in ns (awake time since boot),
    // equivalent to Darwin CLOCK_UPTIME_RAW. No process-local epoch or offset.
    private func monotonicNowNS() -> UInt64 { DispatchTime.now().uptimeNanoseconds }
    private func credential(_ pid: Int32) -> Credential? {
        var info = proc_bsdinfo()
        let size = Int32(MemoryLayout<proc_bsdinfo>.stride)
        guard proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, &info, size) == size else { return nil }
        return Credential(pid: info.pbi_pid, parent: info.pbi_ppid,
                          seconds: info.pbi_start_tvsec, micros: info.pbi_start_tvusec)
    }
    private func executable(_ pid: Int32) -> String? {
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
    private func canonical(_ path: String) -> String {
        URL(fileURLWithPath: path).resolvingSymlinksInPath().standardizedFileURL.path
    }
    private func noLater(_ first: Credential, _ second: Credential) -> Bool {
        first.seconds < second.seconds || (first.seconds == second.seconds && first.micros <= second.micros)
    }
    private let inventory: [(String, CGEventType)] = [
        ("mouseMoved", .mouseMoved), ("leftMouseDown", .leftMouseDown),
        ("leftMouseUp", .leftMouseUp), ("rightMouseDown", .rightMouseDown),
        ("rightMouseUp", .rightMouseUp), ("keyDown", .keyDown), ("keyUp", .keyUp),
        ("flagsChanged", .flagsChanged), ("scrollWheel", .scrollWheel),
        ("leftMouseDragged", .leftMouseDragged), ("rightMouseDragged", .rightMouseDragged),
        ("otherMouseDown", .otherMouseDown), ("otherMouseUp", .otherMouseUp),
        ("otherMouseDragged", .otherMouseDragged), ("tabletPointer", .tabletPointer),
        ("tabletProximity", .tabletProximity)
    ]
    private func source(_ state: CGEventSourceStateID) -> [String: Any] {
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
            let buildEnter = diagnostic == nil ? nil : monotonicNowNS()
            rows[name] = ["begin_ns": begin, "age_begin_ns": ageBegin,
                          "age_end_ns": ageEnd, "end_ns": end,
                          "count_before": before, "count_after": after,
                          "age_seconds": available ? (age as Any) : NSNull()]
            if let buildEnter {
                let buildReturn = monotonicNowNS()
                cycle?.field_build.append(FieldBuild(source: state == .hidSystemState ? "hid" : "combined",
                    event_type: name, build_enter_ns: buildEnter, build_return_ns: buildReturn))
            }
        }
        // ~0 is documented only for age, never used as an aggregate counter.
        let begin = monotonicNowNS()
        let ageBegin = monotonicNowNS()
        let age = CGEventSource.secondsSinceLastEventType(state, eventType: CGEventType(rawValue: UInt32.max)!)
        let ageEnd = monotonicNowNS()
        let end = monotonicNowNS()
        let available = age.isFinite && age >= 0 && age <= Double(end) / 1e9 + 1
        let buildEnter = diagnostic == nil ? nil : monotonicNowNS()
        rows["anyInput"] = ["begin_ns": begin, "age_begin_ns": ageBegin,
                            "age_end_ns": ageEnd, "end_ns": end,
                            "age_seconds": available ? (age as Any) : NSNull()]
        if let buildEnter {
            let buildReturn = monotonicNowNS()
            cycle?.field_build.append(FieldBuild(source: state == .hidSystemState ? "hid" : "combined",
                event_type: "anyInput", build_enter_ns: buildEnter, build_return_ns: buildReturn))
        }
        return rows
    }

    private func emit(_ value: [String: Any]) {
        var envelope = value
        envelope["protocol_version"] = 3
        envelope["clock_domain"] = "darwin_uptime_raw_ns"
        envelope["nonce"] = nonce
        var fact: EmitFact?
        if diagnostic != nil {
            rawOrdinal += 1
            fact = EmitFact(raw_record_ordinal: rawOrdinal, event: value["event"] as? String ?? "unknown",
                            sample_seq: value["seq"] as? Int)
            fact?.state = "IN_PROGRESS"
            trace(.emit_start(fact!))
            fact?.encode_enter_ns = monotonicNowNS()
        }
        do {
            var data = try JSONSerialization.data(withJSONObject: envelope, options: [.sortedKeys])
            data.append(0x0A)
            if diagnostic != nil {
                fact?.encode_return_ns = monotonicNowNS()
                fact?.expected_bytes = data.count
                guard rawBytes + UInt64(data.count) <= 67108864 else { exit(74) }
                fact?.offset_before = rawBytes
                fact?.write_enter_ns = monotonicNowNS()
                trace(.emit_progress(fact!))
            }
            try FileHandle.standardOutput.write(contentsOf: data)
            if diagnostic != nil {
                fact?.write_return_ns = monotonicNowNS()
                rawBytes += UInt64(data.count)
                fact?.offset_after = rawBytes
                fact?.state = "RETURNED"
                previousEmit = fact
                trace(.emit_return(fact!))
            }
        } catch {
            if fact != nil {
                fact?.state = "THREW"; fact?.error = "raw encode/write failed"
                trace(.emit_return(fact!))
            }
            exit(74)
        }
    }
    private func fatal(_ reason: String) -> Never {
        emit(["event": "fatal", "reason": reason, "at_ns": monotonicNowNS(), "thread_main": Thread.isMainThread])
        exit(2)
    }
    private func appMatches(_ app: NSRunningApplication, _ expected: Control) -> Bool {
        guard let path = app.executableURL?.path, let wanted = expected.binaryPath else { return false }
        return app.processIdentifier == expected.appPID && canonical(path) == canonical(wanted)
    }
    private func identityAlive(_ expected: Control) -> Bool {
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
    private func applyControl() {
        let next: Control
        do { next = try JSONDecoder().decode(Control.self, from: Data(contentsOf: controlURL)) }
        catch { fatal("INVALID_OR_MISSING_CONTROL") }
        guard next.protocolVersion == 3, next.nonce == nonce, next.commandID >= lastCommand,
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
            emit(["event": "runner_bound", "credential": credentialJSON(found), "at_ns": monotonicNowNS()])
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
            emit(["event": "owned_bound", "credential": credentialJSON(owned),
                  "executable_path": canonical(path), "at_ns": monotonicNowNS()])
        }
        current = next
        lastCommand = next.commandID

    }
    private func readyMarker(_ native: String, _ expected: Control) -> Bool {
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
    private func sample() {
        if diagnostic != nil {
            cycle = CycleFact(cycle_id: sequence + 1, sample_seq: sequence + 1,
                              timer_enter_ns: monotonicNowNS(), previous_emit: previousEmit)
            trace(.cycle_enter(cycle!))
            cycle?.control_enter_ns = monotonicNowNS()
        }
        autoreleasepool {
            applyControl()
            if diagnostic != nil { cycle?.control_return_ns = monotonicNowNS() }
            guard let expected = current else { fatal("NO_CONTROL") }
            let begin = monotonicNowNS()
            if diagnostic != nil { cycle?.sample_begin_ns = begin }
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
            if diagnostic != nil { cycle?.sample_end_ns = end }
            sequence += 1
            emit(["event": "sample", "seq": sequence, "begin_ns": begin, "end_ns": end,
                  "thread_main": Thread.isMainThread, "runner_pid": expected.runnerPID,
                  "owned_pid": expected.appPID, "identity_verified": ownedCredential != nil,
                  "owned_alive": alive, "owned_ready": ready, "front_owned": frontOwned,
                  "capture_complete": complete, "sources": sources])
        }
        if diagnostic != nil {
            cycle?.timer_exit_ns = monotonicNowNS()
            trace(.cycle(cycle!))
        }
    }
    // Timer callbacks are explicitly re-entered through MainActor. All mutable
    // state, AppKit references and non-Sendable JSON values stay inside this actor.
    func run() -> Never {
        guard Thread.isMainThread else { fatal("MONITOR_NOT_MAIN_THREAD") }
        emit(["event": "ready", "monitor_pid": getpid(), "thread_main": Thread.isMainThread,
              "at_ns": monotonicNowNS()])
        let timer = Timer(timeInterval: 0.05, repeats: true) { [self] _ in
            MainActor.assumeIsolated {
                guard Thread.isMainThread else { self.fatal("MONITOR_NOT_MAIN_THREAD") }
                self.sample()
            }
        }
        samplingTimer = timer
        if let diagnostic {
            signal(SIGTERM, SIG_IGN)
            let source = DispatchSource.makeSignalSource(signal: SIGTERM, queue: .main)
            source.setEventHandler { [self] in
                MainActor.assumeIsolated {
                    self.samplingTimer?.invalidate()
                    _ = diagnostic.submit(.terminal(self.cycle))
                    diagnostic.flush()
                    signal(SIGTERM, SIG_DFL)
                    _ = kill(getpid(), SIGTERM)
                }
            }
            termination = source
            source.resume()
        }
        RunLoop.main.add(timer, forMode: .common)
        RunLoop.main.run()
        fatal("MONITOR_MAIN_RUNLOOP_RETURNED")
    }
}

@main
private struct InterferenceObserverMain {
    @MainActor
    static func main() {
        guard [3, 4].contains(CommandLine.arguments.count) else { exit(64) }
        let observer = InterferenceObserver(
            controlURL: URL(fileURLWithPath: CommandLine.arguments[1]),
            nonce: CommandLine.arguments[2],
            sidecar: CommandLine.arguments.count == 4 ? CommandLine.arguments[3] : nil)
        observer.run()
    }
}
