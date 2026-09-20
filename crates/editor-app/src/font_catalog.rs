//! System font metadata only. Selected bytes/face/hash are verified by the service.
use std::path::PathBuf;
#[derive(Clone, Debug)]
pub struct InstalledFont {
    pub family: String,
    pub style: String,
    pub postscript: String,
    pub path: PathBuf,
}
#[cfg(target_os = "macos")]
pub fn installed_fonts() -> Result<Vec<InstalledFont>, String> {
    use objc2_core_foundation::{CFArray, CFRetained, CFString, CFType, CFURL, CFURLPathStyle};
    use objc2_core_text::{
        CTFontCollection, CTFontDescriptor, kCTFontFamilyNameAttribute, kCTFontNameAttribute,
        kCTFontStyleNameAttribute, kCTFontURLAttribute,
    };
    // SAFETY: no options dictionary; immutable OS-owned descriptors are retained
    // by the array, and every attribute is type-checked before reading it.
    let collection = unsafe { CTFontCollection::from_available_fonts(None) };
    let descriptors =
        unsafe { collection.matching_font_descriptors() }.ok_or("系统未返回字体列表")?;
    // SAFETY: CTFontCollectionCopyMatchingFontDescriptors guarantees an array
    // of retained Core Foundation font descriptors, never arbitrary pointers.
    let descriptors = unsafe { CFRetained::cast_unchecked::<CFArray<CFType>>(descriptors) };
    let mut result = Vec::new();
    for (index, value) in descriptors.iter().enumerate() {
        if index >= 10_000 {
            return Err("RESOURCE_LIMIT: system font descriptors (10000)".into());
        }
        let Some(descriptor) = value.downcast_ref::<CTFontDescriptor>() else {
            continue;
        };
        let string = |key: &CFString| -> Option<String> {
            // SAFETY: valid immutable descriptor and a documented Core Text key.
            unsafe { descriptor.attribute(key) }?
                .downcast_ref::<CFString>()
                .map(ToString::to_string)
        };
        // SAFETY: these immutable Core Text attribute keys are provided by macOS.
        let Some((family, style, postscript, path)) = (unsafe {
            let url = descriptor.attribute(kCTFontURLAttribute);
            let path = url
                .as_ref()
                .and_then(|v| v.downcast_ref::<CFURL>())
                .and_then(|url| url.file_system_path(CFURLPathStyle::CFURLPOSIXPathStyle));
            string(kCTFontFamilyNameAttribute)
                .zip(string(kCTFontNameAttribute))
                .zip(path)
                .map(|((family, postscript), path)| {
                    (
                        family,
                        string(kCTFontStyleNameAttribute).unwrap_or_default(),
                        postscript,
                        PathBuf::from(path.to_string()),
                    )
                })
        }) else {
            continue;
        };
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !matches!(extension.as_str(), "ttf" | "otf" | "ttc" | "otc") {
            continue;
        }
        result.push(InstalledFont {
            family,
            style,
            postscript,
            path,
        });
    }
    result.sort_by(|a, b| {
        (&a.family, &a.style, &a.postscript).cmp(&(&b.family, &b.style, &b.postscript))
    });
    result.dedup_by(|a, b| a.path == b.path && a.postscript == b.postscript);
    Ok(result)
}
#[cfg(not(target_os = "macos"))]
pub fn installed_fonts() -> Result<Vec<InstalledFont>, String> {
    Err("Windows 系统字体列表尚未实施 / 验收；可选择字体文件".into())
}
