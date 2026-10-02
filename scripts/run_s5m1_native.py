"""S5-M1 synthetic native runner; preserves every run and raw process sample.
No product API/CLI is introduced. Requires a release internal-evidence binary.
"""
import argparse, hashlib, json, os, pathlib, shutil, subprocess, tempfile, time

DISPLAY_SWIFT = r'''
import CoreGraphics
import AppKit
import Foundation
let d = CGMainDisplayID()
func snapshot() -> [String:Any] {
 let m = CGDisplayCopyDisplayMode(d)!
 return ["display_id":d,"mode_id":m.ioDisplayModeID,"width":m.width,"height":m.height,"pixel_width":m.pixelWidth,"pixel_height":m.pixelHeight,"refresh_hz":m.refreshRate,"backing_scale":NSScreen.main?.backingScaleFactor ?? 0]
}
let before = snapshot()
let op = CommandLine.arguments[1]
if op != "probe" {
 let modes = CGDisplayCopyAllDisplayModes(d,[kCGDisplayShowDuplicateLowResolutionModes:true] as CFDictionary) as! [CGDisplayMode]
 let current = CGDisplayCopyDisplayMode(d)!
 let selected:CGDisplayMode?
 if op == "set60" {
  selected = modes.first { $0.width == current.width && $0.height == current.height && $0.pixelWidth == current.pixelWidth && $0.pixelHeight == current.pixelHeight && abs($0.refreshRate-60)<0.01 }
 } else {
  let id = UInt32(CommandLine.arguments[2])!
  selected = modes.first {$0.ioDisplayModeID == id}
 }
 guard let mode = selected else {fatalError("matching native mode missing")}
 let result = CGDisplaySetDisplayMode(d,mode,nil)
 guard result == .success else {fatalError("display mode error \(result.rawValue)")}
}
let result:[String:Any] = ["before":before,"after":snapshot()]
print(String(data:try! JSONSerialization.data(withJSONObject:result,options:[.sortedKeys]),encoding:.utf8)!)
'''
WINDOW_SWIFT = r'''
import CoreGraphics
import Foundation
let pid = Int(CommandLine.arguments[1])!
let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly,.excludeDesktopElements], kCGNullWindowID) as! [[String:Any]]
let matching = windows.filter { ($0[kCGWindowOwnerPID as String] as? Int)==pid && ($0[kCGWindowLayer as String] as? Int)==0 }
print(String(data:try! JSONSerialization.data(withJSONObject:matching.map { ["window_id":$0[kCGWindowNumber as String]!,"bounds":$0[kCGWindowBounds as String]!] },options:[]),encoding:.utf8)!)
'''
def digest(path): return hashlib.sha256(path.read_bytes()).hexdigest()
def write(path,data): path.write_text(json.dumps(data,ensure_ascii=False,indent=2)+'\n')
def run():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=pathlib.Path,required=True)
    parser.add_argument('--output',type=pathlib.Path,required=True)
    parser.add_argument('--mode',choices=['nav','load','select','idle','lifecycle'],required=True)
    parser.add_argument('--fixture',choices=['P10K','P100K'],required=True)
    parser.add_argument('--video',action='store_true')
    args=parser.parse_args()
    if args.mode=='select' and args.fixture!='P100K': parser.error('fixed 200 points/full marquee require P100K')
    args.output.mkdir(parents=True,exist_ok=False)
    native=pathlib.Path(tempfile.mkdtemp(prefix='rcam-s5m1-native-',dir='/tmp')).resolve()
    write(native/'request.json',{'schema_version':2,'mode':args.mode,'fixture':args.fixture})
    swift=native/'display.swift';swift.write_text(DISPLAY_SWIFT)
    window=native/'window.swift';window.write_text(WINDOW_SWIFT)
    display=json.loads(subprocess.check_output(['/usr/bin/swift',str(swift),'probe'],text=True))
    write(native/'display-before.json',display)
    original=display['after']['mode_id']
    process=None;video=None;raw=[];video_result=None;restore=None;peak_rss=None
    try:
        display=json.loads(subprocess.check_output(['/usr/bin/swift',str(swift),'set60'],text=True))
        write(native/'display-60hz.json',display)
        if abs(display['after']['refresh_hz']-60)>.01: raise RuntimeError('actual display is not 60 Hz')
        env=dict(os.environ,RCAM_S5M1_NATIVE_DIR=str(native))
        for key in ['RCAM_NATIVE_BENCH','RCAM_NATIVE_PROBE_DIR','RCAM_NATIVE_PROBE_AUTOLOAD_BLOCK_FIXTURE','RCAM_S4D1_NATIVE_DIR','RCAM_S4D2_NATIVE_DIR']:
            env.pop(key,None)
        with (native/'app.stdout').open('wb') as out,(native/'app.stderr').open('wb') as err:
            binary=args.binary.resolve(strict=True)
            write(native/'binary-before.json',{'sha256':digest(binary),'bytes':binary.stat().st_size})
            process=subprocess.Popen([str(binary)],stdout=out,stderr=err,env=env)
            start=time.monotonic();last=0.;captured=False
            while True:
                waited,status,usage=os.wait4(process.pid,os.WNOHANG)
                if waited:
                    process.returncode=os.waitstatus_to_exitcode(status)
                    peak_rss=int(usage.ru_maxrss)  # Darwin reports bytes, actual app child only.
                    break
                elapsed=time.monotonic()-start
                if elapsed>930: raise RuntimeError('native process timeout; preserve failed run')
                if elapsed-last>=.5:
                    text=subprocess.run(['/bin/ps','-o','rss=,%cpu=,etime=','-p',str(process.pid)],capture_output=True,text=True,check=False)
                    fields=text.stdout.strip().split()
                    if len(fields)>=3:
                        raw.append({'monotonic_seconds':elapsed,'wall_time_ns':time.time_ns(),'rss_bytes':int(fields[0])*1024,'cpu_percent':float(fields[1]),'elapsed':fields[2]})
                        write(native/'process-samples.json',raw)
                    last=elapsed
                if not captured and elapsed>5:
                    windows=json.loads(subprocess.check_output(['/usr/bin/swift',str(window),str(process.pid)],text=True))
                    write(native/'window.json',windows)
                    if windows:
                        window_id=windows[0]['window_id']
                        subprocess.run(['/usr/sbin/screencapture','-x','-o','-l',str(window_id),str(native/'native-window.png')],capture_output=True,check=True)
                        if args.video:
                            # Capture the app's own native window for the whole
                            # warm-up/navigation interval, not the private desktop.
                            video=subprocess.Popen(['/usr/sbin/screencapture','-x','-v','-V80','-l',str(window_id),str(native/'native-navigation.mov')],stdout=subprocess.PIPE,stderr=subprocess.PIPE)
                        captured=True
                time.sleep(.1)
            exit_code=process.returncode
            if video is not None:
                stdout,stderr=video.communicate(timeout=20)
                video_result={'exit_code':video.returncode,'stdout':stdout.decode(errors='replace'),'stderr':stderr.decode(errors='replace')}
                write(native/'video-command.json',video_result)
            report=native/'native-observations.json'
            if exit_code!=0 or not report.is_file(): raise RuntimeError(f'app exit {exit_code}; observations exist={report.is_file()}')
            observations=json.loads(report.read_text())
            write(native/'runner-result.json',{'status':'OBSERVED','exit_code':exit_code,'mode':args.mode,'fixture':args.fixture,'process_peak_rss_bytes':peak_rss,'sampled_peak_rss_bytes':max(s['rss_bytes'] for s in raw),'peak_rss_method':'macOS wait4 actual app child ru_maxrss bytes','runner_sha256':digest(pathlib.Path(__file__)),'native_failures':observations['failures'],'video':video_result})
    except BaseException as error:
        write(native/'runner-error.json',{'error':str(error),'type':type(error).__name__})
        raise
    finally:
        if process is not None and process.poll() is None:
            process.terminate()
            try:process.wait(timeout=5)
            except subprocess.TimeoutExpired:process.kill();process.wait()
        if video is not None and video.poll() is None:video.terminate();video.wait(timeout=5)
        try:
            restore=json.loads(subprocess.check_output(['/usr/bin/swift',str(swift),'restore',str(original)],text=True))
            write(native/'display-restored.json',restore)
        finally:
            for item in native.iterdir():
                if item.is_file(): shutil.copy2(item,args.output/item.name)
            write(args.output/'file-hashes.json',{p.name:digest(p) for p in sorted(args.output.iterdir()) if p.is_file()})
            print(json.dumps({'output':str(args.output.resolve()),'native_directory':str(native),'display_restored':restore},ensure_ascii=False))
if __name__=='__main__':run()
