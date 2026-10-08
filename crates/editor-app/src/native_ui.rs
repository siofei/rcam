//! Opt-in bounded release UI investigation. Read-only, public fixtures only.
//! Captures actual postpaint surface regions; does not claim OS scanout/video.
use crate::EditorApp;
use eframe::egui::{self, Rect};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{BufWriter, Write},
    path::PathBuf,
    sync::{Mutex, OnceLock, mpsc},
    thread::JoinHandle,
    time::{Duration, Instant},
};
const MAX_FRAMES: u64 = 40_000;
const MAX_SAMPLES: u64 = 1_200;
struct Tag {
    requested: Value,
}
enum WriteJob {
    Append(&'static str, Value),
    Sample(Vec<(String, Vec<u8>)>, Value),
}
fn writer(dir: PathBuf) -> (mpsc::SyncSender<WriteJob>, JoinHandle<()>) {
    // A full bounded queue waits; evidence is never dropped. Errors propagate
    // through send/join and prevent a successful owned application run.
    let (send, receive) = mpsc::sync_channel(16);
    let handle = std::thread::spawn(move || {
        let mut logs = BTreeMap::<&'static str, BufWriter<std::fs::File>>::new();
        for job in receive {
            let (name, value) = match job {
                WriteJob::Append(name, value) => (name, value),
                WriteJob::Sample(files, value) => {
                    for (name, bytes) in files {
                        std::fs::write(dir.join(name), bytes).unwrap();
                    }
                    ("samples.jsonl", value)
                }
            };
            let file = logs.entry(name).or_insert_with(|| {
                BufWriter::new(
                    std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(dir.join(name))
                        .unwrap(),
                )
            });
            serde_json::to_writer(&mut *file, &value).unwrap();
            file.write_all(b"\n").unwrap();
        }
        for file in logs.values_mut() {
            file.flush().unwrap();
        }
    });
    (send, handle)
}
struct Capture {
    dir: PathBuf,
    pmix: bool,
    quiesced: bool,
    requests: u64,
    origin: Instant,
    frame: u64,
    samples: u64,
    last: Instant,
    pending: bool,
    menus: BTreeMap<String, Value>,
    callbacks: Vec<Value>,
    input: Value,
    last_ui: Value,
    profile_at: Instant,
    profile_spans: Vec<Value>,
    writer: Option<mpsc::SyncSender<WriteJob>>,
    writer_handle: Option<JoinHandle<()>>,
}
static CAPTURE: OnceLock<Mutex<Option<Capture>>> = OnceLock::new();
fn capture() -> &'static Mutex<Option<Capture>> {
    CAPTURE.get_or_init(||{
 let value=(||{let dir=std::fs::canonicalize(std::env::var_os("RCAM_UI_ROI_DIR")?).ok()?;let root=std::fs::canonicalize(std::env::var_os("RCAM_UI_ROI_ROOT")?).ok()?;if dir.parent()!=Some(root.as_path())||!dir.file_name()?.to_str()?.starts_with("rcam-i2-c-ui-"){return None;}
 assert!(!cfg!(debug_assertions));let binary=std::fs::read(std::env::current_exe().ok()?).ok()?;
 std::fs::write(dir.join("identity.json"),serde_json::to_vec_pretty(&json!({"schema_version":2,"commit":option_env!("RCAM_BUILD_COMMIT"),"build_source":option_env!("RCAM_BUILD_SOURCE"),"binary_sha256":editor_core::hash::sha256_hex(&binary),"source_manifest_sha256":editor_core::hash::sha256_hex(include_bytes!("../../../MANIFEST.sha256")),"profile":"release","pid":std::process::id(),"scope":"instrumented synthetic native Metal UI actual surface readback; not physical input/OS compositor/scanout","user_flicker_report":"OPEN","sample_interval_ms":100,"max_frames":MAX_FRAMES,"max_samples":MAX_SAMPLES})).unwrap()).unwrap();
 let (writer,writer_handle)=writer(dir.clone());
 Some(Capture{dir,pmix:crate::native_pmix::directory().is_some(),quiesced:false,requests:0,origin:Instant::now(),frame:0,samples:0,last:Instant::now(),pending:false,menus:BTreeMap::new(),callbacks:vec![],input:Value::Null,last_ui:Value::Null,profile_at:Instant::now(),profile_spans:vec![],writer:Some(writer),writer_handle:Some(writer_handle)})})();Mutex::new(value)
})
}
pub fn profile(stage: &str) {
    if let Some(c) = capture().lock().unwrap().as_mut() {
        let now = Instant::now();
        c.profile_spans.push(
            json!({"stage":stage,"duration_ns":now.duration_since(c.profile_at).as_nanos() as u64}),
        );
        c.profile_at = now;
    }
}

fn rect(r: Rect) -> [f32; 4] {
    [r.min.x, r.min.y, r.max.x, r.max.y]
}
fn append(c: &Capture, name: &'static str, value: Value) {
    let _span = crate::native_pmix::spans::enter(crate::native_pmix::spans::Stage::RoiWriterSend);
    c.writer
        .as_ref()
        .unwrap()
        .send(WriteJob::Append(name, value))
        .unwrap();
}
pub fn quiesce() {
    if let Some(c) = capture().lock().unwrap().as_mut() {
        c.quiesced = true;
    }
}
pub fn readbacks_drained() -> bool {
    capture()
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|c| c.quiesced && !c.pending)
}
pub fn finish() -> Option<Value> {
    let capture = CAPTURE.get()?;
    let owned = capture.lock().unwrap().take();
    if let Some(mut c) = owned {
        drop(c.writer.take());
        c.writer_handle.take().unwrap().join().unwrap();
        if c.pmix {
            // Written only for PMIX, after every queued byte has been flushed
            // and the writer joined. Other accepted ROI formats stay unchanged.
            let receipt = json!({"schema_version":1,"frames":c.frame,"requests":c.requests,"samples":c.samples,"quiesced":c.quiesced,"pending":c.pending,"writer_joined":true});
            std::fs::write(
                c.dir.join("capture-finalization.json"),
                serde_json::to_vec_pretty(&receipt).unwrap(),
            )
            .unwrap();
            return Some(receipt);
        }
    }
    None
}
pub fn menu(label: &str, response: &egui::Response) {
    if let Some(c) = capture().lock().unwrap().as_mut() {
        c.menus.insert(label.into(),json!({"rect":rect(response.rect),"hovered":response.hovered(),"enabled":response.enabled(),"layer":format!("{:?}",response.layer_id)}));
    }
}
pub fn callback(info: &egui::PaintCallbackInfo) {
    let _span = crate::native_pmix::spans::enter(crate::native_pmix::spans::Stage::RoiCallback);
    if let Some(c) = capture().lock().unwrap().as_mut() {
        let v = info.viewport_in_pixels();
        let p = info.clip_rect_in_pixels();
        let a = [
            v.left_px.max(p.left_px).max(0),
            v.top_px.max(p.top_px).max(0),
            (v.left_px + v.width_px).min(p.left_px + p.width_px).max(0),
            (v.top_px + v.height_px).min(p.top_px + p.height_px).max(0),
        ];
        let value = json!({"frame":c.frame,"t_ns":c.origin.elapsed().as_nanos() as u64,"viewport_px":[v.left_px,v.top_px,v.width_px,v.height_px],"clip_px":[p.left_px,p.top_px,p.width_px,p.height_px],"scissor_xyxy":a});
        append(c, "paint.jsonl", value.clone());
        c.callbacks.push(value);
        if c.callbacks.len() > 4 {
            c.callbacks.remove(0);
        }
    }
}
pub fn raw_input(raw: &egui::RawInput) {
    let _span = crate::native_pmix::spans::enter(crate::native_pmix::spans::Stage::RoiRawInput);
    let mut guard = capture().lock().unwrap();
    let Some(c) = guard.as_mut() else {
        return;
    };
    c.input = json!({"t_ns":c.origin.elapsed().as_nanos() as u64,"focused":raw.focused,"events":raw.events.iter().filter(|e|!matches!(e,egui::Event::Screenshot{..})).map(|e|format!("{e:?}")).collect::<Vec<_>>()});
    c.profile_at = Instant::now();
    c.profile_spans.clear();
    for event in &raw.events {
        if let egui::Event::Screenshot {
            image, user_data, ..
        } = event
            && let Some(tag) = user_data
                .data
                .as_ref()
                .and_then(|d| d.downcast_ref::<Tag>())
        {
            let t = c.origin.elapsed().as_nanos() as u64;
            let n = c.samples;
            c.samples += 1;
            c.pending = false;
            let mut crops = vec![];
            let mut files = vec![];
            for (name, metadata) in tag.requested["menus"].as_object().unwrap() {
                let bounds = metadata["rect"].as_array().unwrap();
                let ppp = tag.requested["ppp"].as_f64().unwrap();
                let mut px = [0usize; 4];
                for i in 0..4 {
                    px[i] = (bounds[i].as_f64().unwrap() * ppp).round().max(0.) as usize;
                }
                px[2] = px[2].min(image.size[0]);
                px[3] = px[3].min(image.size[1]);
                if px[2] <= px[0] || px[3] <= px[1] {
                    continue;
                }
                let mut bytes =
                    format!("P6\n{} {}\n255\n", px[2] - px[0], px[3] - px[1]).into_bytes();
                for y in px[1]..px[3] {
                    for x in px[0]..px[2] {
                        let p = image.pixels[y * image.size[0] + x];
                        bytes.extend([p.r(), p.g(), p.b()]);
                    }
                }
                let file = format!("roi-{n:04}-{name}.ppm");
                crops.push(json!({"name":name,"path":file,"sha256":editor_core::hash::sha256_hex(&bytes),"rect_px":px,"metadata":metadata}));
                files.push((file, bytes));
            }
            let _span =
                crate::native_pmix::spans::enter(crate::native_pmix::spans::Stage::RoiWriterSend);
            c.writer.as_ref().unwrap().send(WriteJob::Sample(files,
                json!({"sample":n,"callback_ns":t,"callback_after_ui_frame":c.frame,"callback_last_ui":c.last_ui,"surface_size_px":image.size,"request":tag.requested,"crops":crops,"latest_paints":c.callbacks}),
            )).unwrap();
        }
    }
    c.profile_spans.push(
        json!({"stage":"roi-readback","duration_ns":c.profile_at.elapsed().as_nanos() as u64}),
    );
    c.profile_at = Instant::now();
}
pub fn frame(app: &EditorApp, ctx: &egui::Context) {
    let _span = crate::native_pmix::spans::enter(crate::native_pmix::spans::Stage::RoiFrame);
    let mut guard = capture().lock().unwrap();
    let Some(c) = guard.as_mut() else {
        return;
    };
    c.frame += 1;
    if c.frame > MAX_FRAMES {
        return;
    }
    let fields =
        crate::status_bar::fields(&app.view, app.display_unit, app.precision().resolution_mm);
    let version = editor_service::task::TaskVersion::capture(
        app.view.info.as_ref(),
        app.view.task_generation,
        app.view.rule_revision,
    );
    let snapshot = json!({"profile_spans":c.profile_spans,"frame":c.frame,"pass_index":ctx.current_pass_index(),"t_ns":c.origin.elapsed().as_nanos() as u64,"canvas":rect(app.canvas_rect),"ppp":ctx.pixels_per_point(),"menus":c.menus,"popup":egui::Popup::is_any_open(ctx),"pointer":ctx.input(|i|i.pointer.hover_pos().map(|p|[p.x,p.y])),"focused":ctx.input(|i|i.focused),"interaction":app.prefs.interaction,"status":{"selection":fields.selection,"area":fields.area,"perimeter":fields.perimeter,"state":fields.state},"version":version,"selection_epoch":app.view.selection_epoch,"selected":app.view.selected.ordered.len(),"info":app.view.info,"scene_serial":app.view.scene.as_ref().map(|s|s.serial),"busy":app.busy,"display_pending":app.display_pending,"drag":app.drag.as_ref().map(crate::drag::Gesture::evidence_state),"grip":app.grip.as_ref().map(|g|json!({"id":g.id,"moved":g.moved,"target":g.target})),"input":c.input});
    c.last_ui = snapshot.clone();
    append(c, "frames.jsonl", snapshot.clone());
    if c.samples < MAX_SAMPLES
        && !c.quiesced
        && !c.pending
        && c.last.elapsed() >= Duration::from_millis(100)
        && !c.menus.is_empty()
    {
        let requested_at = Instant::now();
        c.last = requested_at;
        c.pending = true;
        let mut requested = snapshot;
        requested["request_ns"] = json!(requested_at.duration_since(c.origin).as_nanos() as u64);
        c.requests += 1;
        if c.pmix {
            append(c, "requests.jsonl", requested.clone());
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(
            Tag { requested },
        )));
    }
}

pub fn geometry_request(sequence: u64, context: &str) {
    if let Some(c) = capture().lock().unwrap().as_ref() {
        append(
            c,
            "geometry-requests.jsonl",
            json!({"frame":c.frame+1,"t_ns":c.origin.elapsed().as_nanos() as u64,"sequence":sequence,"context":context}),
        );
    }
}

#[cfg(test)]
mod writer_tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn directory() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "rcam-pmix-roi-writer-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        std::fs::create_dir(&path).unwrap();
        path
    }
    #[test]
    fn queued_frames_and_readbacks_are_flushed_before_join_returns() {
        let dir = directory();
        let (send, handle) = writer(dir.clone());
        for frame in 1..=20 {
            send.send(WriteJob::Append("frames.jsonl", json!({"frame":frame})))
                .unwrap();
        }
        send.send(WriteJob::Sample(
            vec![("unit-roi.ppm".into(), b"synthetic-unit-pixels".to_vec())],
            json!({"sample":0}),
        ))
        .unwrap();
        drop(send);
        handle.join().unwrap();
        let frames = std::fs::read_to_string(dir.join("frames.jsonl")).unwrap();
        let observed: Vec<Value> = frames
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(observed.len(), 20);
        assert_eq!(observed.first().unwrap()["frame"], 1);
        assert_eq!(observed.last().unwrap()["frame"], 20);
        assert_eq!(
            std::fs::read(dir.join("unit-roi.ppm")).unwrap(),
            b"synthetic-unit-pixels".to_vec()
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("samples.jsonl")).unwrap(),
            "{\"sample\":0}\n"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn writer_io_failure_cannot_produce_a_successful_join() {
        let dir = directory();
        std::fs::remove_dir(&dir).unwrap();
        let (send, handle) = writer(dir);
        send.send(WriteJob::Append("frames.jsonl", json!({"frame":1})))
            .unwrap();
        drop(send);
        assert!(handle.join().is_err());
    }
}
