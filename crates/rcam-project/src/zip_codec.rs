//! Deterministic ZIP with Store (legacy) and raw Deflate entries.
//! Compression uses pinned miniz_oxide level 6; schema and manifest hashes
//! remain over the original JSON. Inflation has a fixed, pre-budgeted buffer.

use crate::error::ProjectError;

const LOCAL_HEADER_SIG: u32 = 0x0403_4b50;
const CENTRAL_HEADER_SIG: u32 = 0x0201_4b50;
const EOCD_SIG: u32 = 0x0605_4b50;
/// 1980-01-01 00:00:00, the ZIP epoch: every entry gets this exact
/// timestamp so two encodes of the same logical content are byte-identical.
const DOS_DATE: u16 = 0x0021;
const DOS_TIME: u16 = 0x0000;
/// EFS (bit 11): filenames/comments are UTF-8, not the legacy codepage.
const GENERAL_PURPOSE_FLAG: u16 = 0x0800;
const VERSION: u16 = 20;

pub struct ZipEntry<'a> {
    pub path: &'a str,
    pub data: &'a [u8],
}

/// Bounded reader policy (§8/§49): every limit is checked before the
/// corresponding allocation, never after.
#[derive(Debug, Clone, Copy)]
pub struct ReadPolicy {
    pub max_entries: usize,
    pub max_uncompressed_bytes: usize,
    pub max_entry_bytes: usize,
    pub max_path_len: usize,
}

pub struct ReadEntry {
    pub path: String,
    pub data: Vec<u8>,
}

pub fn write_zip(entries: &[ZipEntry]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for entry in entries {
        let offset = out.len() as u32;
        let name = entry.path.as_bytes();
        let crc = crc32(entry.data);
        let size = entry.data.len() as u32;
        let compressed = miniz_oxide::deflate::compress_to_vec(entry.data, 6);
        let (method, payload): (u16, &[u8]) = if compressed.len() < entry.data.len() {
            (8, &compressed)
        } else {
            (0, entry.data)
        };
        let compressed_size = payload.len() as u32;
        out.extend_from_slice(&LOCAL_HEADER_SIG.to_le_bytes());
        out.extend_from_slice(&VERSION.to_le_bytes());
        out.extend_from_slice(&GENERAL_PURPOSE_FLAG.to_le_bytes());
        out.extend_from_slice(&method.to_le_bytes()); // Store or Deflate
        out.extend_from_slice(&DOS_TIME.to_le_bytes());
        out.extend_from_slice(&DOS_DATE.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&compressed_size.to_le_bytes()); // compressed size
        out.extend_from_slice(&size.to_le_bytes()); // uncompressed size
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // extra field length
        out.extend_from_slice(name);
        out.extend_from_slice(payload);

        central.extend_from_slice(&CENTRAL_HEADER_SIG.to_le_bytes());
        central.extend_from_slice(&VERSION.to_le_bytes()); // version made by
        central.extend_from_slice(&VERSION.to_le_bytes()); // version needed
        central.extend_from_slice(&GENERAL_PURPOSE_FLAG.to_le_bytes());
        central.extend_from_slice(&method.to_le_bytes());
        central.extend_from_slice(&DOS_TIME.to_le_bytes());
        central.extend_from_slice(&DOS_DATE.to_le_bytes());
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&compressed_size.to_le_bytes());
        central.extend_from_slice(&size.to_le_bytes());
        central.extend_from_slice(&(name.len() as u16).to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes()); // extra field length
        central.extend_from_slice(&0u16.to_le_bytes()); // comment length
        central.extend_from_slice(&0u16.to_le_bytes()); // disk number start
        central.extend_from_slice(&0u16.to_le_bytes()); // internal attributes
        central.extend_from_slice(&0u32.to_le_bytes()); // external attributes
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name);
    }
    let cd_offset = out.len() as u32;
    let cd_size = central.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&EOCD_SIG.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // disk number
    out.extend_from_slice(&0u16.to_le_bytes()); // disk with CD start
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&cd_size.to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // comment length
    out
}

fn limit(resource: &'static str, limit: usize, actual: usize) -> ProjectError {
    ProjectError::ResourceLimit {
        resource,
        limit,
        actual,
    }
}

fn read_u16(bytes: &[u8], at: usize) -> Result<u16, ProjectError> {
    bytes
        .get(at..at + 2)
        .map(|slice| u16::from_le_bytes([slice[0], slice[1]]))
        .ok_or_else(|| ProjectError::MalformedArchive("truncated field".into()))
}

fn read_u32(bytes: &[u8], at: usize) -> Result<u32, ProjectError> {
    bytes
        .get(at..at + 4)
        .map(|slice| u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
        .ok_or_else(|| ProjectError::MalformedArchive("truncated field".into()))
}

/// Reject `../`, an absolute path, a backslash (Windows separator confusion),
/// an empty path, or a NUL byte; return the normalized (already-clean) path.
fn safe_path(raw: &str, max_len: usize) -> Result<String, ProjectError> {
    if raw.is_empty() || raw.len() > max_len {
        return Err(limit("path_len", max_len, raw.len()));
    }
    if raw.starts_with('/')
        || raw.contains('\\')
        || raw.contains('\0')
        || raw
            .split('/')
            .any(|segment| segment == "." || segment == "..")
    {
        return Err(ProjectError::PathTraversal(raw.into()));
    }
    Ok(raw.into())
}

pub fn read_zip(bytes: &[u8], policy: &ReadPolicy) -> Result<Vec<ReadEntry>, ProjectError> {
    if bytes.len() < 22 {
        return Err(ProjectError::MalformedArchive(
            "too small to be a zip".into(),
        ));
    }
    let eocd_at = bytes.len() - 22;
    if read_u32(bytes, eocd_at)? != EOCD_SIG {
        return Err(ProjectError::MalformedArchive(
            "end-of-central-directory record not found (trailing data or not a zip)".into(),
        ));
    }
    let disk = read_u16(bytes, eocd_at + 4)?;
    let cd_disk = read_u16(bytes, eocd_at + 6)?;
    if disk != 0 || cd_disk != 0 {
        return Err(ProjectError::UnsupportedFeature(
            "multi-disk archive".into(),
        ));
    }
    let total_entries = read_u16(bytes, eocd_at + 10)? as usize;
    let cd_size = read_u32(bytes, eocd_at + 12)? as usize;
    let cd_offset = read_u32(bytes, eocd_at + 16)? as usize;
    let comment_len = read_u16(bytes, eocd_at + 20)?;
    if comment_len != 0 || eocd_at + 22 != bytes.len() {
        return Err(ProjectError::MalformedArchive(
            "unexpected trailing data after the central directory".into(),
        ));
    }
    if total_entries > policy.max_entries {
        return Err(limit("entries", policy.max_entries, total_entries));
    }
    if cd_offset > eocd_at || cd_offset + cd_size != eocd_at {
        return Err(ProjectError::MalformedArchive(
            "central directory offset/size is inconsistent".into(),
        ));
    }

    let mut entries = Vec::with_capacity(total_entries);
    let mut seen_paths = std::collections::HashSet::with_capacity(total_entries);
    let mut total_uncompressed: usize = 0;
    let mut cursor = cd_offset;
    for _ in 0..total_entries {
        if read_u32(bytes, cursor)? != CENTRAL_HEADER_SIG {
            return Err(ProjectError::MalformedArchive(
                "central directory record signature mismatch".into(),
            ));
        }
        let method = read_u16(bytes, cursor + 10)?;
        let crc = read_u32(bytes, cursor + 16)?;
        let compressed_size = read_u32(bytes, cursor + 20)? as usize;
        let uncompressed_size = read_u32(bytes, cursor + 24)? as usize;
        let name_len = read_u16(bytes, cursor + 28)? as usize;
        let extra_len = read_u16(bytes, cursor + 30)? as usize;
        let comment_len = read_u16(bytes, cursor + 32)? as usize;
        let local_offset = read_u32(bytes, cursor + 42)? as usize;
        let name_at = cursor + 46;
        let name_bytes = bytes.get(name_at..name_at + name_len).ok_or_else(|| {
            ProjectError::MalformedArchive("truncated central directory name".into())
        })?;
        let name = std::str::from_utf8(name_bytes)
            .map_err(|_| ProjectError::MalformedArchive("entry path is not valid UTF-8".into()))?;
        let path = safe_path(name, policy.max_path_len)?;
        if !seen_paths.insert(path.clone()) {
            return Err(ProjectError::DuplicatePath(path));
        }
        if method != 0 && method != 8 {
            return Err(ProjectError::UnsupportedFeature(format!(
                "compression method {method} (only Store and Deflate are supported)"
            )));
        }
        if method == 0 && compressed_size != uncompressed_size {
            return Err(ProjectError::MalformedArchive(
                "store entry must have equal compressed/uncompressed size".into(),
            ));
        }
        if uncompressed_size > policy.max_entry_bytes {
            return Err(limit(
                "entry_bytes",
                policy.max_entry_bytes,
                uncompressed_size,
            ));
        }
        total_uncompressed = total_uncompressed
            .checked_add(uncompressed_size)
            .ok_or_else(|| {
                limit(
                    "uncompressed_bytes",
                    policy.max_uncompressed_bytes,
                    usize::MAX,
                )
            })?;
        if total_uncompressed > policy.max_uncompressed_bytes {
            return Err(limit(
                "uncompressed_bytes",
                policy.max_uncompressed_bytes,
                total_uncompressed,
            ));
        }

        // Cross-check the local header before trusting the central directory.
        if read_u32(bytes, local_offset)? != LOCAL_HEADER_SIG {
            return Err(ProjectError::MalformedArchive(
                "local file header signature mismatch".into(),
            ));
        }
        let local_method = read_u16(bytes, local_offset + 8)?;
        let local_crc = read_u32(bytes, local_offset + 14)?;
        let local_compressed = read_u32(bytes, local_offset + 18)? as usize;
        let local_uncompressed = read_u32(bytes, local_offset + 22)? as usize;
        let local_name_len = read_u16(bytes, local_offset + 26)? as usize;
        let local_extra_len = read_u16(bytes, local_offset + 28)? as usize;
        if local_method != method
            || local_crc != crc
            || local_compressed != compressed_size
            || local_uncompressed != uncompressed_size
            || local_name_len != name_len
        {
            return Err(ProjectError::MalformedArchive(
                "local file header disagrees with the central directory".into(),
            ));
        }
        let local_name_at = local_offset + 30;
        let local_name = bytes
            .get(local_name_at..local_name_at + local_name_len)
            .ok_or_else(|| ProjectError::MalformedArchive("truncated local file name".into()))?;
        if local_name != name_bytes {
            return Err(ProjectError::MalformedArchive(
                "local file header name disagrees with the central directory".into(),
            ));
        }
        let data_at = local_name_at + local_name_len + local_extra_len;
        if data_at + compressed_size > cd_offset {
            return Err(ProjectError::MalformedArchive(
                "entry overlaps central directory".into(),
            ));
        }
        let payload = bytes
            .get(data_at..data_at + compressed_size)
            .ok_or_else(|| ProjectError::MalformedArchive("truncated entry data".into()))?;
        let data = if method == 0 {
            payload.to_vec()
        } else {
            use miniz_oxide::inflate::{
                TINFLStatus,
                core::{DecompressorOxide, decompress, inflate_flags},
            };
            // Never let an untrusted stream grow the output beyond the declared,
            // already budget-checked size. Require full stream and exact length.
            let mut decoded = vec![0; uncompressed_size];
            let (status, consumed, written) = decompress(
                &mut DecompressorOxide::new(),
                payload,
                &mut decoded,
                0,
                inflate_flags::TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF,
            );
            if status != TINFLStatus::Done
                || consumed != payload.len()
                || written != uncompressed_size
            {
                return Err(ProjectError::MalformedArchive(
                    "invalid Deflate stream or size mismatch".into(),
                ));
            }
            decoded
        };
        if crc32(&data) != crc {
            return Err(ProjectError::HashMismatch { path: path.clone() });
        }
        entries.push(ReadEntry { path, data });
        cursor = name_at + name_len + extra_len + comment_len;
    }
    if cursor != cd_offset + cd_size {
        return Err(ProjectError::MalformedArchive(
            "central directory size does not match its entries".into(),
        ));
    }
    Ok(entries)
}

/// Standard reflected CRC-32 (polynomial 0xEDB88320), computed directly
/// (no lookup table) since entries are small and this runs once per encode.
fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> ReadPolicy {
        ReadPolicy {
            max_entries: 100,
            max_uncompressed_bytes: 1_000_000,
            max_entry_bytes: 1_000_000,
            max_path_len: 512,
        }
    }

    #[test]
    fn compressed_and_legacy_entries_round_trip_deterministically() {
        let large = vec![b'a'; 10000];
        let entries = [
            ZipEntry {
                path: "large",
                data: &large,
            },
            ZipEntry {
                path: "small",
                data: b"x",
            },
        ];
        let bytes = write_zip(&entries);
        assert_eq!(bytes, write_zip(&entries));
        assert!(bytes.len() < 500);
        assert_eq!(read_u16(&bytes, 8).unwrap(), 8);
        let decoded = read_zip(&bytes, &policy()).unwrap();
        assert_eq!(decoded[0].data, large);
        assert_eq!(decoded[1].data, b"x");
    }

    #[test]
    fn deflate_rejects_false_lengths_truncation_trailing_bytes_and_crc() {
        let original = write_zip(&[ZipEntry {
            path: "a",
            data: &vec![b'a'; 1000],
        }]);
        let cd = read_u32(&original, original.len() - 6).unwrap() as usize;
        for size in [0u32, 999, 1001] {
            let mut bytes = original.clone();
            bytes[22..26].copy_from_slice(&size.to_le_bytes());
            bytes[cd + 24..cd + 28].copy_from_slice(&size.to_le_bytes());
            assert!(read_zip(&bytes, &policy()).is_err());
        }
        for delta in [-1i32, 1] {
            let mut bytes = original.clone();
            let end = cd - 1;
            if delta < 0 {
                bytes.remove(end);
            } else {
                bytes.insert(cd, 0);
            }
            let new_cd = (cd as i32 + delta) as usize;
            let size = (read_u32(&original, 18).unwrap() as i32 + delta) as u32;
            bytes[18..22].copy_from_slice(&size.to_le_bytes());
            bytes[new_cd + 20..new_cd + 24].copy_from_slice(&size.to_le_bytes());
            let eocd_offset = bytes.len() - 6;
            bytes[eocd_offset..eocd_offset + 4].copy_from_slice(&(new_cd as u32).to_le_bytes());
            assert!(read_zip(&bytes, &policy()).is_err());
        }
        let mut bytes = original;
        bytes[14] ^= 1;
        bytes[cd + 16] ^= 1;
        assert!(matches!(
            read_zip(&bytes, &policy()),
            Err(ProjectError::HashMismatch { .. })
        ));
    }

    #[test]
    fn crc32_matches_known_vector() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn round_trips_multiple_entries_exactly() {
        let entries = [
            ZipEntry {
                path: "manifest.json",
                data: b"{}",
            },
            ZipEntry {
                path: "layers/l1.json",
                data: b"[1,2,3]",
            },
        ];
        let bytes = write_zip(&entries);
        let read = read_zip(&bytes, &policy()).unwrap();
        assert_eq!(read.len(), 2);
        assert_eq!(read[0].path, "manifest.json");
        assert_eq!(read[0].data, b"{}");
        assert_eq!(read[1].path, "layers/l1.json");
        assert_eq!(read[1].data, b"[1,2,3]");
    }

    #[test]
    fn encode_is_byte_identical_across_calls() {
        let entries = [ZipEntry {
            path: "a.json",
            data: b"hello",
        }];
        assert_eq!(write_zip(&entries), write_zip(&entries));
    }

    #[test]
    fn rejects_path_traversal_and_absolute_paths() {
        for bad in [
            "../escape.json",
            "/etc/passwd",
            "a/../../b.json",
            "a\\b.json",
        ] {
            let bytes = write_zip(&[ZipEntry {
                path: bad,
                data: b"x",
            }]);
            assert!(matches!(
                read_zip(&bytes, &policy()),
                Err(ProjectError::PathTraversal(_))
            ));
        }
    }

    #[test]
    fn rejects_duplicate_normalized_path() {
        let bytes = write_zip(&[
            ZipEntry {
                path: "a.json",
                data: b"1",
            },
            ZipEntry {
                path: "a.json",
                data: b"2",
            },
        ]);
        assert!(matches!(
            read_zip(&bytes, &policy()),
            Err(ProjectError::DuplicatePath(_))
        ));
    }

    #[test]
    fn rejects_entry_count_over_budget() {
        let names: Vec<String> = (0..5).map(|i| format!("e{i}.json")).collect();
        let entries: Vec<_> = names
            .iter()
            .map(|name| ZipEntry {
                path: name.as_str(),
                data: b"1",
            })
            .collect();
        let bytes = write_zip(&entries);
        let tight = ReadPolicy {
            max_entries: 4,
            ..policy()
        };
        assert!(matches!(
            read_zip(&bytes, &tight),
            Err(ProjectError::ResourceLimit {
                resource: "entries",
                ..
            })
        ));
    }

    #[test]
    fn rejects_uncompressed_size_over_budget() {
        let bytes = write_zip(&[ZipEntry {
            path: "big.json",
            data: &vec![b'a'; 1000],
        }]);
        let tight = ReadPolicy {
            max_uncompressed_bytes: 500,
            ..policy()
        };
        assert!(matches!(
            read_zip(&bytes, &tight),
            Err(ProjectError::ResourceLimit {
                resource: "uncompressed_bytes",
                ..
            })
        ));
    }

    #[test]
    fn rejects_corrupted_entry_bytes() {
        let mut bytes = write_zip(&[ZipEntry {
            path: "a.json",
            data: b"hello",
        }]);
        let last = bytes.len() - 1;
        bytes[last - 25] ^= 0xFF; // flip a byte inside the file data region
        assert!(read_zip(&bytes, &policy()).is_err());
    }
}
