//! Native modal path selection only; parsing and export stay on the service worker.
use std::path::PathBuf;
/// Native `.rcam` project picker; the service validates the selected path.
pub fn choose_project(save: bool) -> Result<Option<PathBuf>, String> {
    #[cfg(target_os = "macos")]
    {
        use objc2::MainThreadMarker;
        use objc2_app_kit::{NSModalResponseCancel, NSModalResponseOK, NSOpenPanel, NSSavePanel};
        use objc2_foundation::{NSArray, NSString};
        let mtm = MainThreadMarker::new().ok_or("文件选择器必须在 UI 线程运行")?;
        let panel: objc2::rc::Retained<NSSavePanel> = if save {
            let panel = NSSavePanel::savePanel(mtm);
            panel.setTitle(Some(&NSString::from_str("保存 RCam 工程（.rcam）")));
            panel
        } else {
            let panel = NSOpenPanel::openPanel(mtm);
            panel.setTitle(Some(&NSString::from_str("打开 RCam 工程（.rcam）")));
            panel.setCanChooseDirectories(false);
            panel.setCanChooseFiles(true);
            panel.setAllowsMultipleSelection(false);
            panel.into_super()
        };
        let types = NSArray::from_retained_slice(&[NSString::from_str("rcam")]);
        #[allow(deprecated)]
        panel.setAllowedFileTypes(Some(&types));
        if save {
            // NSSavePanel appends its allowed extension. Supplying it here as
            // well can produce "Untitled.rcam.rcam" when the panel is reused.
            panel.setNameFieldStringValue(&NSString::from_str("Untitled"));
        }
        match panel.runModal() {
            response if response == NSModalResponseOK => panel
                .URL()
                .and_then(|url| url.path())
                .map(|p| Some(PathBuf::from(p.to_string())))
                .ok_or("文件选择器没有返回路径".into()),
            response if response == NSModalResponseCancel => Ok(None),
            _ => Err("原生文件选择器未能完成".into()),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = save;
        Err("本轮原生工程选择器仅在 macOS 实施；Windows 待验收".into())
    }
}
/// Multi-select Gerber picker for batch import. `Ok(None)` means cancelled.
pub fn choose_gerbers() -> Result<Option<Vec<PathBuf>>, String> {
    #[cfg(target_os = "macos")]
    {
        use objc2::MainThreadMarker;
        use objc2_app_kit::{NSModalResponseCancel, NSModalResponseOK, NSOpenPanel};
        use objc2_foundation::NSString;
        let mtm = MainThreadMarker::new().ok_or("文件选择器必须在 UI 线程运行")?;
        let panel = NSOpenPanel::openPanel(mtm);
        panel.setTitle(Some(&NSString::from_str(
            "导入 Gerber（可多选，每个文件成为一个图层）",
        )));
        panel.setCanChooseDirectories(false);
        panel.setCanChooseFiles(true);
        panel.setAllowsMultipleSelection(true);
        match panel.runModal() {
            response if response == NSModalResponseOK => {
                let urls = panel.URLs();
                let paths: Vec<PathBuf> = (0..urls.count())
                    .filter_map(|index| urls.objectAtIndex(index).path())
                    .map(|p| PathBuf::from(p.to_string()))
                    .collect();
                if paths.is_empty() {
                    Err("文件选择器没有返回路径".into())
                } else {
                    Ok(Some(paths))
                }
            }
            response if response == NSModalResponseCancel => Ok(None),
            _ => Err("原生文件选择器未能完成".into()),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("本轮原生文件选择器只在 macOS 实施；Windows 待验收".into())
    }
}

pub fn choose_path(save: bool, name: &str) -> Result<Option<PathBuf>, String> {
    #[cfg(target_os = "macos")]
    {
        use objc2::MainThreadMarker;
        use objc2_app_kit::{NSModalResponseCancel, NSModalResponseOK, NSOpenPanel, NSSavePanel};
        use objc2_foundation::NSString;
        let mtm = MainThreadMarker::new().ok_or("文件选择器必须在 UI 线程运行")?;
        let panel: objc2::rc::Retained<NSSavePanel> = if save {
            let panel = NSSavePanel::savePanel(mtm);
            panel.setTitle(Some(&NSString::from_str(
                "导出 Gerber（请选择新文件名；导出不会保存工作区）",
            )));
            panel.setNameFieldStringValue(&NSString::from_str(name));
            panel
        } else {
            let panel = NSOpenPanel::openPanel(mtm);
            panel.setTitle(Some(&NSString::from_str(if name == "font" {
                "选择本地字体 TTF / OTF / TTC"
            } else {
                "打开 Gerber"
            })));
            panel.setCanChooseDirectories(false);
            panel.setCanChooseFiles(true);
            panel.setAllowsMultipleSelection(false);
            panel.into_super()
        };
        match panel.runModal() {
            response if response == NSModalResponseOK => panel
                .URL()
                .and_then(|url| url.path())
                .map(|p| Some(PathBuf::from(p.to_string())))
                .ok_or("文件选择器没有返回路径".into()),
            response if response == NSModalResponseCancel => Ok(None),
            _ => Err("原生文件选择器未能完成".into()),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (save, name);
        Err("本轮原生文件选择器只在 macOS 实施；Windows 待验收".into())
    }
}

/// Explicit diagnostic export; no overwrite is performed by the background exporter.
pub fn choose_diagnostics() -> Result<Option<PathBuf>, String> {
    #[cfg(target_os = "macos")]
    {
        use objc2::MainThreadMarker;
        use objc2_app_kit::{NSModalResponseCancel, NSModalResponseOK, NSSavePanel};
        use objc2_foundation::{NSArray, NSString};
        let mtm = MainThreadMarker::new().ok_or("文件选择器必须在 UI 线程运行")?;
        let panel = NSSavePanel::savePanel(mtm);
        panel.setTitle(Some(&NSString::from_str("导出诊断包（请选择新文件名）")));
        let types = NSArray::from_retained_slice(&[NSString::from_str("zip")]);
        #[allow(deprecated)]
        panel.setAllowedFileTypes(Some(&types));
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        panel.setNameFieldStringValue(&NSString::from_str(&format!("RCam-diagnostics-{stamp}")));
        match panel.runModal() {
            response if response == NSModalResponseOK => panel
                .URL()
                .and_then(|url| url.path())
                .map(|p| Some(PathBuf::from(p.to_string())))
                .ok_or("文件选择器没有返回路径".into()),
            response if response == NSModalResponseCancel => Ok(None),
            _ => Err("原生文件选择器未能完成".into()),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("诊断导出选择器本轮仅验收 macOS".into())
    }
}
