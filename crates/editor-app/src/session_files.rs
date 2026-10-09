//! Worker-only cross-session file reservations; the service still performs I/O.
use std::{
    fs, io,
    path::{Path, PathBuf},
};

fn canonical(path: &Path) -> io::Result<PathBuf> {
    match fs::canonicalize(path) {
        Ok(path) => Ok(path),
        Err(cause) if cause.kind() == io::ErrorKind::NotFound => {
            let name = path
                .file_name()
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing file name"))?;
            let parent = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            Ok(fs::canonicalize(parent)?.join(name))
        }
        Err(cause) => Err(cause),
    }
}

#[cfg(unix)]
fn identity(path: &Path) -> io::Result<Option<(u64, u128)>> {
    use std::os::unix::fs::MetadataExt;
    match fs::metadata(path) {
        Ok(meta) => Ok(Some((meta.dev(), u128::from(meta.ino())))),
        Err(cause) if cause.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(cause) => Err(cause),
    }
}

#[cfg(windows)]
fn identity(path: &Path) -> io::Result<Option<(u64, u128)>> {
    use std::os::windows::io::AsRawHandle;
    #[repr(C)]
    #[derive(Default)]
    struct FileInformation {
        volume: u64,
        file_id: [u8; 16],
    }
    const _: () = assert!(std::mem::size_of::<FileInformation>() == 24);
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetFileInformationByHandleEx(
            handle: *mut std::ffi::c_void,
            class: i32,
            information: *mut std::ffi::c_void,
            size: u32,
        ) -> i32;
    }
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(cause) if cause.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(cause) => return Err(cause),
    };
    let mut info = FileInformation::default();
    // FILE_ID_INFO / FileIdInfo=0x12, per the Windows SDK:
    // https://learn.microsoft.com/windows/win32/api/winbase/ns-winbase-file_id_info
    // https://learn.microsoft.com/windows/win32/api/winbase/nf-winbase-getfileinformationbyhandleex
    // SAFETY: File owns a live handle. The exclusive repr(C) buffer has the
    // documented 24-byte layout. Failure rejects the operation, without a
    // truncated 64-bit fallback for filesystems with 128-bit identifiers.
    if unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            0x12,
            std::ptr::from_mut(&mut info).cast(),
            24,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(Some((info.volume, u128::from_le_bytes(info.file_id))))
}

pub(crate) fn check_reserved(
    path: &Path,
    reserved: &[PathBuf],
) -> Result<(), editor_service::ServiceError> {
    let check = || -> io::Result<bool> {
        if reserved.is_empty() {
            return Ok(false);
        }
        let target = canonical(path)?;
        let target_id = identity(path)?;
        for other in reserved {
            if canonical(other)? == target {
                return Ok(true);
            }
            if target_id.is_some() && identity(other)? == target_id {
                return Ok(true);
            }
        }
        Ok(false)
    };
    match check() {
        Ok(false) => Ok(()),
        Ok(true) => Err(editor_service::ServiceError {
            code: "CONFLICT".into(),
            message: "目标文件已由另一工程页签持有，请切换到该页签或选择其他路径".into(),
            details: serde_json::json!({}),
        }),
        Err(_) => Err(editor_service::ServiceError {
            code: "IO_ERROR".into(),
            message: "无法核对其他页签的文件身份，未执行打开或保存".into(),
            details: serde_json::json!({}),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn live_output_reservation_rechecks_paths_links_and_external_rebinding() {
        let root = std::env::temp_dir().join(format!("rcam-tab-alias-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let live = root.join("live.rcam");
        let other = root.join("other.rcam");
        fs::write(&live, b"live source").unwrap();
        fs::write(&other, b"other source").unwrap();
        let reserved = vec![live.clone()];
        assert!(check_reserved(&other, &reserved).is_ok());
        assert_eq!(
            check_reserved(&live, &reserved).unwrap_err().code,
            "CONFLICT"
        );
        let hard = root.join("hard.rcam");
        fs::hard_link(&live, &hard).unwrap();
        assert_eq!(
            check_reserved(&hard, &reserved).unwrap_err().code,
            "CONFLICT"
        );
        #[cfg(unix)]
        {
            let soft = root.join("soft.rcam");
            std::os::unix::fs::symlink(&live, &soft).unwrap();
            assert_eq!(
                check_reserved(&soft, &reserved).unwrap_err().code,
                "CONFLICT"
            );
        }
        fs::remove_file(&other).unwrap();
        fs::hard_link(&live, &other).unwrap();
        assert_eq!(
            check_reserved(&other, &reserved).unwrap_err().code,
            "CONFLICT"
        );
        assert_eq!(fs::read(&live).unwrap(), b"live source");
        fs::remove_dir_all(&root).unwrap();
    }
}
