"""Reviewed Mac evidence producer source; no capture on import.

The initialization probe queries no-consent metadata only. Owned-window mode
starts capture; self tests write explicitly synthetic pixels without a GUI.
"""
SOURCE = r'''
import Foundation
import CoreMedia
import CoreVideo
import AVFoundation
import ScreenCaptureKit
import AppKit
import CoreGraphics

func now() -> UInt64 { DispatchTime.now().uptimeNanoseconds }
let outputLock = NSLock()
func emit(_ value: [String:Any]) {
    var data = try! JSONSerialization.data(withJSONObject:value,options:[.sortedKeys])
    data.append(10)
    outputLock.lock();defer {outputLock.unlock()}
    FileHandle.standardOutput.write(data)
}

final class Platform {
    static let lock = NSLock()
    static var rows = [[String:Any]]()
    static var diagnostic: FileHandle?
    static func configure(_ path:URL?) throws {
        guard let path=path else {return}
        guard !FileManager.default.fileExists(atPath:path.path),
              FileManager.default.createFile(atPath:path.path,contents:nil) else {
            throw CaptureError(message:"existing/invalid platform diagnostic")
        }
        diagnostic=try FileHandle(forWritingTo:path)
    }
    static func record(_ stage:String,_ extra:[String:Any]=[:]) {
        var row:[String:Any]=["stage":stage,"producer_pid":ProcessInfo.processInfo.processIdentifier,
                            "at_ns":now(),"thread_main":Thread.isMainThread]
        row.merge(extra) {_,new in new}
        var data=try! JSONSerialization.data(withJSONObject:row,options:[.sortedKeys]);data.append(10)
        lock.lock();defer {lock.unlock()};rows.append(row);diagnostic?.write(data)
    }
    static func snapshot() -> [[String:Any]] {
        lock.lock();defer {lock.unlock()};return rows
    }
    @MainActor static func mainThread(_ stage:String) throws {
        guard Thread.isMainThread else {throw CaptureError(message:"physical main thread required: \(stage)")}
    }
    @MainActor static func bootstrap() throws {
        try mainThread("bootstrap")
        let app=NSApplication.shared
        let before=app.activationPolicy();let changed=app.setActivationPolicy(.prohibited)
        let after=app.activationPolicy();let active=app.isActive;let windows=app.windows.count
        record("bootstrap-attempt",["before_policy_raw":before.rawValue,"after_policy_raw":after.rawValue,
                                  "set_policy_return":changed,"active":active,"own_windows":windows])
        // An unbundled executable already defaults to prohibited. Validate the
        // actual policy instead of treating an idempotent setter false as unsafe.
        guard after == .prohibited,!active,windows==0 else {
            throw CaptureError(message:"non-GUI bootstrap policy: before=\(before.rawValue) after=\(after.rawValue) setter=\(changed) active=\(active) windows=\(windows)")
        }
        record("bootstrap",["activation_policy":"prohibited","active":active,"own_windows":windows])
    }
    @MainActor static func makeFilter(_ window:SCWindow,query:String) throws -> SCContentFilter {
        try mainThread("filter-before")
        guard window.windowID>0,window.isOnScreen,window.windowLayer==0 else {
            throw CaptureError(message:"real eligible on-screen window required")
        }
        record("filter-before",["query_kind":query,"eligible_real_window":true])
        let filter=SCContentFilter(desktopIndependentWindow:window)
        try mainThread("filter-after")
        record("filter-after",["query_kind":query,"constructor":"desktopIndependentWindow"])
        return filter
    }
    @MainActor static func probe(ownWindowOnly:Bool) async {
        let permission=CGPreflightScreenCaptureAccess()
        record("permission-check",["existing_access":permission,"request_called":false])
        var receipt:[String:Any]=["schema_version":2,"event":"initialization-probe","scope":"initialization-only",
           "query_kind":"currentProcess-no-consent","native_proof":false,"existing_screen_access":permission,
           "streams_created":0,"streams_started":0,"writer_created":false,"frames_received":0,
           "own_windows":NSApplication.shared.windows.count,"active":NSApplication.shared.isActive,
           "producer_pid":ProcessInfo.processInfo.processIdentifier,"selection":ownWindowOnly ? "own-window-none" : "real-on-screen"]
        do {
            guard #available(macOS 14.4,*) else {throw ProbeBlocked(reason:"api-unavailable")}
            let content=try await SCShareableContent.currentProcess
            try mainThread("probe-after-query")
            record("metadata-returned",["query_kind":"currentProcess-no-consent","window_count":content.windows.count,
                "nonzero_id_count":content.windows.filter {$0.windowID>0}.count,
                "onscreen_count":content.windows.filter {$0.isOnScreen}.count,
                "layer0_count":content.windows.filter {$0.windowLayer==0}.count,
                "eligible_count":content.windows.filter {$0.windowID>0 && $0.isOnScreen && $0.windowLayer==0}.count])
            guard let window=content.windows.first(where:{$0.windowID>0 && $0.isOnScreen && $0.windowLayer==0 &&
                (!ownWindowOnly || $0.owningApplication?.processID==ProcessInfo.processInfo.processIdentifier)}) else {
                throw ProbeBlocked(reason:"no-eligible-window")
            }
            _=try makeFilter(window,query:"currentProcess-no-consent")
            guard NSApplication.shared.windows.isEmpty,!NSApplication.shared.isActive else {throw CaptureError(message:"probe changed own UI state")}
            receipt["result"]="INITIALIZATION_ONLY_PASS"
        } catch {
            receipt["result"]=error is ProbeBlocked ? "BLOCKED" : "FAIL"
            if let blocked=error as? ProbeBlocked {receipt["blocked_reason"]=blocked.reason}
            receipt["error"]=String(describing:error)
        }
        let stages=snapshot().map {$0["stage"] as? String}
        receipt["filter_constructor"]=stages.contains("filter-after") ? "EXECUTED" :
            (stages.contains("filter-before") ? "ATTEMPTED" : "NOT_EXECUTED")
        receipt["own_windows"]=NSApplication.shared.windows.count
        receipt["active"]=NSApplication.shared.isActive
        record("probe-terminal",["active":NSApplication.shared.isActive,"own_windows":NSApplication.shared.windows.count,
                                "filter_constructor":receipt["filter_constructor"]!,"result":receipt["result"]!])
        receipt["trace"]=snapshot();emit(receipt)
        if receipt["result"] as? String == "BLOCKED" {exit(2)}
        if receipt["result"] as? String == "FAIL" {exit(1)}
    }
}
struct CaptureError: Error { let message: String }
struct ProbeBlocked: Error { let reason: String }

final class Capture: NSObject, SCStreamOutput, SCStreamDelegate, @unchecked Sendable {
    let queue = DispatchQueue(label:"rcam.pmix.capture.samples")
    let writer: AVAssetWriter
    let input: AVAssetWriterInput
    let appPID: Int
    let windowID: UInt32
    let runID: String
    let kind: String
    let width: Int
    let height: Int
    let started = now()
    var stream: SCStream?
    var count = 0
    var first: CMTime?
    var last: CMTime?
    var stopping = false
    var stopped = false
    var error: String?
    var ready: [String:Any]?
    var terminal = "collecting"
    var failureEmitted = false

    init(output: URL, appPID: Int, windowID: UInt32, runID: String,
         kind: String, width: Int, height: Int) throws {
        guard width > 0 && height > 0 && width <= 16384 && height <= 16384,
              !FileManager.default.fileExists(atPath:output.path) else {
            throw CaptureError(message:"invalid dimensions or existing output")
        }
        writer = try AVAssetWriter(outputURL:output,fileType:.mov)
        input = AVAssetWriterInput(mediaType:.video,outputSettings:[
            AVVideoCodecKey:AVVideoCodecType.h264,
            AVVideoWidthKey:width,AVVideoHeightKey:height])
        input.expectsMediaDataInRealTime = true
        guard writer.canAdd(input) else { throw CaptureError(message:"writer cannot add H264 input") }
        writer.add(input)
        self.appPID=appPID;self.windowID=windowID;self.runID=runID
        self.kind=kind;self.width=width;self.height=height
        super.init()
    }
    func identity(_ event: String) -> [String:Any] {
        ["schema_version":2,"event":event,"producer_pid":ProcessInfo.processInfo.processIdentifier,
         "app_pid":appPID,"window_id":windowID,"run_id":runID,"source_kind":kind,
         "clock":"DispatchTime.uptimeNanoseconds","producer_started_ns":started,"at_ns":now(),
         "accepted_samples":count,"width":width,"height":height]
    }
    // Invoked only on the serial sample queue. A late notification is retained
    // in diagnostics; it cannot emit a second contradictory terminal event.
    func fail(_ message: String) {
        if terminal=="completed" || terminal=="failed" {
            Platform.record("late-notification",["terminal":terminal,"error":message]);return
        }
        if error == nil {
            error=message;var row=identity("failure");row["error"]=message
            failureEmitted=true;emit(row);Platform.record("capture-error",["error":message])
        }
    }
    func terminateFailure(_ message:String) {
        queue.sync {
            if terminal=="completed" {Platform.record("late-notification",["terminal":terminal,"error":message]);return}
            if !failureEmitted {fail(message)}
            if writer.status == .writing {writer.cancelWriting()}
            terminal="failed"
        }
    }
    func consume(_ sample: CMSampleBuffer, complete: Bool) {
        guard !stopping && error == nil && complete else { return }
        guard CMSampleBufferIsValid(sample),CMSampleBufferDataIsReady(sample),
              let pixels=CMSampleBufferGetImageBuffer(sample),
              CVPixelBufferGetWidth(pixels)==width,CVPixelBufferGetHeight(pixels)==height else {
            fail("invalid complete screen sample");return
        }
        let pts=CMSampleBufferGetPresentationTimeStamp(sample)
        guard pts.isNumeric,pts.timescale>0,last == nil || CMTimeCompare(pts,last!)>0 else {
            fail("invalid/nonmonotonic sample PTS");return
        }
        if first == nil {
            guard writer.startWriting() else {fail("startWriting: \(String(describing:writer.error))");return}
            writer.startSession(atSourceTime:pts)
        }
        // Backpressure cannot silently discard a complete stream frame.
        guard input.isReadyForMoreMediaData else {fail("writer backpressure on complete frame");return}
        guard input.append(sample),writer.status == .writing else {
            fail("append failed: \(String(describing:writer.error))");return
        }
        first=first ?? pts;last=pts;count+=1
        if ready == nil {
            var row=identity("ready");row["frame_status"]="complete"
            row["sample_pts_value"]=pts.value;row["sample_pts_timescale"]=pts.timescale
            row["writer_status"]="writing";row["sample_append_succeeded"]=true
            ready=row;emit(row)
        }
    }
    func stream(_ stream:SCStream,didOutputSampleBuffer sample:CMSampleBuffer,of type:SCStreamOutputType) {
        guard type == .screen else {return}
        guard let rows=CMSampleBufferGetSampleAttachmentsArray(sample,createIfNecessary:false) as? [[SCStreamFrameInfo:Any]],
              let value=rows.first?[.status] as? Int,
              let status=SCFrameStatus(rawValue:value) else {fail("missing stream frame status");return}
        consume(sample,complete:status == .complete)
    }
    func stream(_ stream:SCStream,didStopWithError error:Error) {
        queue.async {self.fail("stream stopped with error: \(error)")}
    }
    @MainActor func startOwnedWindow() async throws {
        try Platform.mainThread("owned-start")
        guard CGPreflightScreenCaptureAccess() else {throw CaptureError(message:"screen access not already granted; no request")}
        Platform.record("permission-check",["existing_access":true,"request_called":false])
        let content=try await SCShareableContent.excludingDesktopWindows(true,onScreenWindowsOnly:true)
        guard let window=content.windows.first(where:{$0.windowID==windowID}),
              let owner=window.owningApplication,Int(owner.processID)==appPID else {
            throw CaptureError(message:"owned app window not found")
        }
        try Platform.mainThread("owned-after-query")
        let filter=try Platform.makeFilter(window,query:"excludingDesktopWindows-owned")
        let config=SCStreamConfiguration()
        config.width=width;config.height=height
        config.minimumFrameInterval=CMTime(value:1,timescale:60)
        config.queueDepth=3;config.showsCursor=true;config.capturesAudio=false
        if #available(macOS 14.0,*) {config.ignoreShadowsSingleWindow=true}
        if #available(macOS 14.2,*) {config.includeChildWindows=false}
        let capture=SCStream(filter:filter,configuration:config,delegate:self)
        try capture.addStreamOutput(self,type:.screen,sampleHandlerQueue:queue)
        stream=capture
        Platform.record("stream-created",["app_pid":appPID,"window_id":windowID])
        try await capture.startCapture()
        Platform.record("stream-started")
    }
    func stopAndFinish() async throws -> [String:Any] {
        let stopAt=now()
        if let stream=stream {try await stream.stopCapture()}
        Platform.record("stream-stop-completed")
        // This drains callbacks already enqueued, not every possible future
        // delegate notification. All state and the terminal decision stay here.
        let failure=queue.sync {() -> String? in
            stopping=true;stopped=true;terminal="finishing"
            guard error == nil,count>0,ready != nil,writer.status == .writing else {
                return error ?? "stop without accepted first frame"
            }
            input.markAsFinished();return nil
        }
        if let failure=failure {throw CaptureError(message:failure)}
        await withCheckedContinuation { (continuation:CheckedContinuation<Void,Never>) in
            queue.async {self.writer.finishWriting {continuation.resume()}}
        }
        return try queue.sync {
            guard error == nil,writer.status == .completed,writer.error == nil else {
                throw CaptureError(message:error ?? "finishWriting failed: \(String(describing:writer.error))")
            }
            var row=identity("finished")
            row["stop_requested_ns"]=stopAt;row["stream_stopped"]=true
            row["sample_queue_drained"]=true;row["input_marked_finished"]=true
            row["writer_status"]="completed";row["finish_writing_callback_received"]=true
            row["first_pts_value"]=first!.value;row["first_pts_timescale"]=first!.timescale
            row["last_pts_value"]=last!.value;row["last_pts_timescale"]=last!.timescale
            row["output_bytes"]=(try FileManager.default.attributesOfItem(atPath:writer.outputURL.path)[.size] as! NSNumber).uint64Value
            terminal="completed";Platform.record("writer-terminal",["terminal":terminal])
            return row
        }
    }

}

func syntheticSample(index:Int,width:Int,height:Int) throws -> CMSampleBuffer {
    var pixels:CVPixelBuffer?
    guard CVPixelBufferCreate(kCFAllocatorDefault,width,height,kCVPixelFormatType_32BGRA,
        [kCVPixelBufferIOSurfacePropertiesKey:[:]] as CFDictionary,&pixels)==kCVReturnSuccess,
        let pixels=pixels else {throw CaptureError(message:"synthetic pixel allocation")}
    CVPixelBufferLockBaseAddress(pixels,[])
    memset(CVPixelBufferGetBaseAddress(pixels),Int32(index+20),CVPixelBufferGetDataSize(pixels))
    CVPixelBufferUnlockBaseAddress(pixels,[])
    var format:CMVideoFormatDescription?
    guard CMVideoFormatDescriptionCreateForImageBuffer(allocator:kCFAllocatorDefault,imageBuffer:pixels,formatDescriptionOut:&format)==noErr,
        let format=format else {throw CaptureError(message:"synthetic format")}
    var timing=CMSampleTimingInfo(duration:CMTime(value:1,timescale:60),presentationTimeStamp:CMTime(value:Int64(index+1),timescale:60),decodeTimeStamp:.invalid)
    var sample:CMSampleBuffer?
    guard CMSampleBufferCreateReadyWithImageBuffer(allocator:kCFAllocatorDefault,imageBuffer:pixels,formatDescription:format,sampleTiming:&timing,sampleBufferOut:&sample)==noErr,
        let sample=sample else {throw CaptureError(message:"synthetic sample")}
    return sample
}

@main struct Main {
    @MainActor static func main() async {
        var activeCapture:Capture?
        do {
            let args=CommandLine.arguments
            guard args.count>=2 else {throw CaptureError(message:"capture arguments")}
            if args[1]=="--owned-window",args.count==9 {
                try Platform.configure(URL(fileURLWithPath:args[4]).deletingLastPathComponent().appendingPathComponent("capture-platform.jsonl"))
            }
            try Platform.bootstrap()
            if args[1]=="--probe-initialization" || args[1]=="--probe-no-window" {
                guard args.count==2 else {throw CaptureError(message:"probe arguments")}
                await Platform.probe(ownWindowOnly:args[1]=="--probe-no-window");return
            }
            if args[1]=="--self-test" {
                guard args.count==4 else {throw CaptureError(message:"self-test arguments")}
                let mode=args[2];let capture=try Capture(output:URL(fileURLWithPath:args[3]),appPID:0,windowID:0,
                    runID:"synthetic-writer-self-test",kind:"synthetic-self-test",width:64,height:48)
                activeCapture=capture
                if mode != "no-frames" && mode != "stop-before-ready" {
                    let n=mode=="short" ? 1 : 8
                    for index in 0..<n {
                        let sample=try syntheticSample(index:index,width:64,height:48)
                        capture.queue.sync {capture.consume(sample,complete:true)}
                    }
                }
                if mode=="fail-after-ready" {capture.queue.sync {capture.fail("injected source failure")}}
                if mode=="invalid-pts" {
                    let sample=try syntheticSample(index:0,width:64,height:48)
                    capture.queue.sync {capture.consume(sample,complete:true)}
                }
                emit(try await capture.stopAndFinish());return
            }
            guard args[1]=="--owned-window",args.count==9,
                  let pid=Int(args[2]),let window=UInt32(args[3]),
                  let width=Int(args[7]),let height=Int(args[8]),pid>0,window>0 else {
                throw CaptureError(message:"owned-window arguments")
            }
            let capture=try Capture(output:URL(fileURLWithPath:args[4]),appPID:pid,windowID:window,
                runID:args[5],kind:"owned-window-sck",width:width,height:height)
            activeCapture=capture
            guard args[6]=="H264-MOV" else {throw CaptureError(message:"unreviewed codec/container")}
            try await capture.startOwnedWindow()
            let command=await withCheckedContinuation {(continuation:CheckedContinuation<String?,Never>) in
                DispatchQueue.global().async {continuation.resume(returning:readLine())}
            }
            guard command=="STOP" else {throw CaptureError(message:"capture control EOF/invalid stop")}
            emit(try await capture.stopAndFinish())
        } catch {
            if let capture=activeCapture {capture.terminateFailure(String(describing:error))}
            else {emit(["schema_version":2,"event":"failure","error":String(describing:error)])}
            exit(2)
        }
    }
}
'''
