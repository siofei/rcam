//! S1 semantic adapter and safety writer.
//!
//! The upstream parser is used only as a syntax tree.  This module owns the
//! state machine, numeric checks, resource accounting, and the normalized
//! manufacturing model exposed to the rest of the application.

use editor_core::{
    ApertureDefinition, ApertureShape, ArcDirection, ArcGeometry, ArcSource, Exposure,
    LocalTransform, MacroPrimitive, Mirror, MmPoint, RegionContour, RegionEdge, RegionRole,
    SemanticDocument, SemanticError, SemanticFormat, SemanticGeometry, SemanticLayer,
    SemanticObject, SourceMetadata, ValidationReport,
};
use gerber_parser::gerber_types::{
    Aperture, Command, CoordinateMode, DCode, ExtendedCode, FunctionCode, GCode, InterpolationMode,
    MCode, MacroBoolean, MacroContent, MacroDecimal, Operation, Polarity, QuadrantMode, Unit,
};
use gerber_parser::{GerberDoc, parse};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, Cursor, Read};
use std::path::{Path, PathBuf};

pub const S1_MAX_SOURCE_BYTES: usize = 8 * 1024 * 1024;
pub const S1_MAX_COMMANDS: usize = 2_000_000;
pub const S1_MAX_OBJECTS: usize = 500_000;
pub const S1_MAX_AM_EXPANSIONS: usize = 1_000_000;
pub const S1_MAX_AM_EXPRESSION_TOKENS: usize = 100_000;
pub const S1_MAX_AM_EXPRESSION_DEPTH: usize = 256;
pub const S1_MAX_REGION_EDGES: usize = 2_000_000;
pub const S1_MAX_WRITER_BYTES: usize = 32 * 1024 * 1024;
pub const S1_MAX_VALIDATION_BYTES: usize = 32 * 1024 * 1024;
const S1_ROUNDTRIP_TOLERANCE_MM: f64 = 0.500001e-6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct S1Budget {
    pub max_source_bytes: usize,
    pub max_commands: usize,
    pub max_objects: usize,
    pub max_am_expansions: usize,
    pub max_region_edges: usize,
    pub max_writer_bytes: usize,
    pub max_validation_bytes: usize,
}

impl Default for S1Budget {
    fn default() -> Self {
        Self {
            max_source_bytes: S1_MAX_SOURCE_BYTES,
            max_commands: S1_MAX_COMMANDS,
            max_objects: S1_MAX_OBJECTS,
            max_am_expansions: S1_MAX_AM_EXPANSIONS,
            max_region_edges: S1_MAX_REGION_EDGES,
            max_writer_bytes: S1_MAX_WRITER_BYTES,
            max_validation_bytes: S1_MAX_VALIDATION_BYTES,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct S1Scene {
    pub document: SemanticDocument,
    pub metadata: SourceMetadata,
    pub diagnostics: Vec<String>,
    pub budget: S1Budget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum S1Error {
    InvalidUtf8,
    Empty,
    Unsupported {
        line: usize,
        feature: String,
    },
    Syntax {
        line: usize,
        message: String,
    },
    Semantic {
        line: usize,
        message: String,
    },
    ResourceLimit {
        resource: &'static str,
        limit: usize,
        actual: usize,
    },
    TargetExists(PathBuf),
    Io {
        path: PathBuf,
        message: String,
    },
}

impl std::fmt::Display for S1Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidUtf8 => f.write_str("input is not UTF-8"),
            Self::Empty => f.write_str("input is empty"),
            Self::Unsupported { line, feature } => {
                write!(f, "unsupported at line {line}: {feature}")
            }
            Self::Syntax { line, message } => write!(f, "syntax error at line {line}: {message}"),
            Self::Semantic { line, message } => {
                write!(f, "semantic error at line {line}: {message}")
            }
            Self::ResourceLimit {
                resource,
                limit,
                actual,
            } => {
                write!(f, "resource limit {resource}: {actual} > {limit}")
            }
            Self::TargetExists(path) => write!(f, "target already exists: {}", path.display()),
            Self::Io { path, message } => write!(f, "I/O error at {}: {message}", path.display()),
        }
    }
}

impl std::error::Error for S1Error {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportReport {
    pub path: PathBuf,
    pub bytes_written: usize,
    pub validation: ValidationReport,
}

/// Parse a supported Gerber into the independent S1 model.
pub fn parse_s1(bytes: &[u8], document_id: &str) -> Result<S1Scene, S1Error> {
    parse_s1_with_budget(bytes, document_id, S1Budget::default())
}

pub fn parse_s1_with_budget(
    bytes: &[u8],
    document_id: &str,
    budget: S1Budget,
) -> Result<S1Scene, S1Error> {
    if bytes.len() > budget.max_source_bytes {
        return Err(S1Error::ResourceLimit {
            resource: "source_bytes",
            limit: budget.max_source_bytes,
            actual: bytes.len(),
        });
    }
    let source = std::str::from_utf8(bytes).map_err(|_| S1Error::InvalidUtf8)?;
    if source.trim().is_empty() {
        return Err(S1Error::Empty);
    }
    let (parser_source, metadata, token_count, io_offset) =
        prepare_source(source, budget.max_commands)?;
    if token_count > budget.max_commands {
        return Err(S1Error::ResourceLimit {
            resource: "parser_commands",
            limit: budget.max_commands,
            actual: token_count,
        });
    }
    let doc =
        parse(BufReader::new(Cursor::new(parser_source.as_bytes()))).map_err(|(_, error)| {
            S1Error::Syntax {
                line: 0,
                message: format!("{error:?}"),
            }
        })?;
    if let Some(error) = doc.errors().first() {
        return Err(S1Error::Syntax {
            line: 0,
            message: format!("{error:?}"),
        });
    }
    let parsed_commands = doc.commands().len();
    if parsed_commands > budget.max_commands {
        return Err(S1Error::ResourceLimit {
            resource: "parser_commands",
            limit: budget.max_commands,
            actual: parsed_commands,
        });
    }
    interpret_s1(doc, document_id, metadata, io_offset, budget)
}

fn prepare_source(
    source: &str,
    max_commands: usize,
) -> Result<(String, SourceMetadata, usize, MmPoint), S1Error> {
    let mut parser_lines = Vec::new();
    let mut metadata = SourceMetadata::default();
    let mut token_count: usize = 0;
    let mut seen_data = false;
    let mut seen_image_data = false;
    let mut seen_icas = false;
    let mut io_offset = MmPoint::new(0.0, 0.0);
    let mut io_seen = false;
    let mut legacy_full_width: Option<usize> = None;
    let mut explicit_unit: Option<Unit> = None;
    let mut legacy_unit: Option<Unit> = None;
    let tokens = tokenize_source(source, max_commands)?;
    let has_legacy_coordinate_commands = tokens
        .iter()
        .any(|(line, _)| matches!(line.as_str(), "G90*" | "G91*"));
    let mut source_coordinate_mode: Option<CoordinateMode> = None;
    let mut pending_coordinate_mode: Option<CoordinateMode> = None;
    let mut coordinate_spec: Option<CoordinateSpec> = None;
    let mut legacy_current: Option<MmPoint> = None;
    for (raw, line_no) in tokens {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        // A Gerber command is terminated by `*`; counting non-empty lines is
        // insufficient because the parser accepts many commands on one line.
        // This is deliberately an upper bound (comments and AM content can
        // contain terminators too) so budget checks fail closed.
        token_count = token_count.saturating_add(line.bytes().filter(|byte| *byte == b'*').count());
        if line == "*" {
            // A standalone leading terminator is an empty separator, not
            // image data.  Keep it in the command budget but never pass it to
            // the third-party parser.
            continue;
        }
        let mut parser_line = line.to_string();
        if let Some(comment) = line.strip_prefix("G04")
            && !matches!(comment.trim_start().chars().next(), Some('#' | '@' | '!'))
        {
            // The function command ends at `*`; newlines inside an ordinary
            // G04 payload are comment text, not command boundaries.  Keep
            // metadata comments untouched so the upstream metadata parser
            // remains the authority for those records.
            parser_line = parser_line.replace(['\r', '\n'], " ");
        }
        if line.starts_with("%FSD") {
            if seen_data {
                return Err(S1Error::Semantic {
                    line: line_no,
                    message: "legacy FSD must precede image data".into(),
                });
            }
            let width = parse_fsd_width(line).ok_or_else(|| S1Error::Syntax {
                line: line_no,
                message: "FSD requires X/Y integer and decimal widths".into(),
            })?;
            legacy_full_width = Some(width);
            // The locked parser has no `D` zero-omission variant.  FSD's
            // supported S1 subset is the explicit full-width form, so map it
            // to leading-omission parsing after checking every field below.
            parser_line.replace_range(3..4, "L");
        }
        if line.starts_with("%FS") && line.ends_with("*%") {
            if seen_data {
                return Err(S1Error::Semantic {
                    line: line_no,
                    message: "FS must precede image data".into(),
                });
            }
            coordinate_spec = parse_coordinate_spec(line);
            if metadata.coordinate_format.is_some() {
                return Err(S1Error::Semantic {
                    line: line_no,
                    message: "FS may be declared only once".into(),
                });
            }
            metadata.coordinate_format = Some(line.to_string());
            source_coordinate_mode = Some(
                pending_coordinate_mode
                    .take()
                    .unwrap_or_else(|| parse_fs_coordinate_mode(line)),
            );
            if has_legacy_coordinate_commands {
                // The locked third-party parser has no G90/G91 command AST.
                // Convert those modal records to absolute coordinates and use
                // the parser's leading-omission absolute format.
                if parser_line.len() >= 5 {
                    parser_line.replace_range(3..4, "L");
                    parser_line.replace_range(4..5, "A");
                }
            }
        }
        if line == "%MOMM*%" || line == "%MOIN*%" {
            if seen_data {
                return Err(S1Error::Semantic {
                    line: line_no,
                    message: "MO must precede image data".into(),
                });
            }
            explicit_unit = Some(if line == "%MOMM*%" {
                Unit::Millimeters
            } else {
                Unit::Inches
            });
            metadata.unit_declarations.push(line.to_string());
        }
        if matches!(line, "G70*" | "G71*") {
            if legacy_unit.is_some() || seen_image_data {
                return Err(S1Error::Semantic {
                    line: line_no,
                    message: "G70/G71 may occur at most once".into(),
                });
            }
            legacy_unit = Some(if line == "G70*" {
                Unit::Inches
            } else {
                Unit::Millimeters
            });
            metadata.unit_declarations.push(line.to_string());
            continue;
        }
        if matches!(line, "G90*" | "G91*") {
            let mode = if line == "G91*" {
                CoordinateMode::Incremental
            } else {
                CoordinateMode::Absolute
            };
            if source_coordinate_mode.is_some() {
                source_coordinate_mode = Some(mode);
            } else {
                pending_coordinate_mode = Some(mode);
            }
            metadata.coordinate_modes.push(line.to_string());
            continue;
        }
        if let Some(width) = legacy_full_width {
            check_fsd_coordinate_width(line, width, line_no)?;
        }
        if has_legacy_coordinate_commands
            && let (Some(spec), Some(mode)) = (coordinate_spec, source_coordinate_mode)
            && !line.starts_with('%')
            && !line.starts_with("G04")
        {
            parser_line = normalize_legacy_coordinates(
                &parser_line,
                spec,
                mode,
                &mut legacy_current,
                io_offset,
                line_no,
            )?;
        }
        if line == "%ICAS*%" {
            if seen_data || seen_icas || !source.is_ascii() {
                return Err(S1Error::Semantic {
                    line: line_no,
                    message: "ICAS must be one leading ASCII declaration".into(),
                });
            }
            seen_icas = true;
            metadata.encoding = Some("ASCII".into());
            continue;
        }
        if line.starts_with("%IO") && line.ends_with("*%") {
            if seen_image_data || io_seen || explicit_unit.is_none() {
                return Err(S1Error::Semantic {
                    line: line_no,
                    message: "IO must be a single pre-image offset declaration".into(),
                });
            }
            let body = line.trim_start_matches("%IO").trim_end_matches("*%");
            let (a, b) = parse_io_body(body).ok_or_else(|| S1Error::Semantic {
                line: line_no,
                message: "IO requires finite A/B values".into(),
            })?;
            io_offset = MmPoint::new(a, b);
            io_seen = true;
            continue;
        }
        if line.starts_with("%LN") && line.ends_with("*%") {
            let value = line.trim_start_matches("%LN").trim_end_matches("*%");
            if value.is_empty()
                || value.contains('*')
                || value.contains('%')
                || seen_image_data
                || metadata.layer_name.is_some()
            {
                return Err(S1Error::Semantic {
                    line: line_no,
                    message: "LN name is empty".into(),
                });
            }
            metadata.layer_name = Some(value.to_string());
            continue;
        }
        if line.starts_with("%IN") && line.ends_with("*%") {
            let value = line.trim_start_matches("%IN").trim_end_matches("*%");
            if value.is_empty()
                || value.contains('*')
                || value.contains('%')
                || seen_image_data
                || metadata.image_name.is_some()
            {
                return Err(S1Error::Semantic {
                    line: line_no,
                    message: "IN name is empty".into(),
                });
            }
            metadata.image_name = Some(value.to_string());
        }
        if (line.starts_with("%TF")
            || line.starts_with("%TA")
            || line.starts_with("%TO")
            || line.starts_with("%TD"))
            && line.ends_with("*%")
        {
            // Preserve the source declaration for the service's explicit
            // metadata policy.  It is never copied into normalized output.
            metadata.file_attributes.push(line.to_string());
        }
        if (!line.starts_with('%') && !line.starts_with("G04"))
            || line.starts_with("%AD")
            || line.starts_with("%AM")
        {
            seen_data = true;
        }
        if is_image_data_line(line) {
            seen_image_data = true;
        }
        parser_lines.push(parser_line);
    }
    if let (Some(expected), Some(actual)) = (legacy_unit, explicit_unit)
        && expected != actual
    {
        return Err(S1Error::Semantic {
            line: 0,
            message: "G70/G71 conflicts with MO".into(),
        });
    }
    if token_count > max_commands {
        return Err(S1Error::ResourceLimit {
            resource: "parser_commands",
            limit: max_commands,
            actual: token_count,
        });
    }
    Ok((parser_lines.join("\n"), metadata, token_count, io_offset))
}

fn is_image_data_line(line: &str) -> bool {
    if line.starts_with('%') || line.starts_with("G04") {
        return false;
    }
    if matches!(
        line,
        "G01*"
            | "G02*"
            | "G03*"
            | "G36*"
            | "G37*"
            | "G70*"
            | "G71*"
            | "G74*"
            | "G75*"
            | "G90*"
            | "G91*"
    ) {
        return false;
    }
    if line.starts_with('M') {
        return false;
    }
    // D10 and higher are aperture selections. D01/D02/D03 are the actual
    // image operations, including modal operations without a new coordinate.
    line.contains("D01")
        || line.contains("D02")
        || line.contains("D03")
        || line.contains('X')
        || line.contains('Y')
        || line.contains('I')
        || line.contains('J')
}

fn parse_fs_coordinate_mode(line: &str) -> CoordinateMode {
    if line.as_bytes().get(4) == Some(&b'I') {
        CoordinateMode::Incremental
    } else {
        CoordinateMode::Absolute
    }
}

#[derive(Debug, Clone, Copy)]
struct CoordinateSpec {
    omission: u8,
    integer: usize,
    decimal: usize,
}

fn parse_coordinate_spec(line: &str) -> Option<CoordinateSpec> {
    if !line.starts_with("%FS") || !line.ends_with("*%") {
        return None;
    }
    let bytes = line.as_bytes();
    let omission = *bytes.get(3)?;
    let x = line.find('X')? + 1;
    let y = line[x..].find('Y')? + x;
    let x_digits = line.get(x..y)?;
    let y_digits = line.get(y + 1..line.len().checked_sub(2)?)?;
    if x_digits.len() != 2 || y_digits.len() != 2 || x_digits != y_digits {
        return None;
    }
    let integer = x_digits[0..1].parse().ok()?;
    let decimal = x_digits[1..2].parse().ok()?;
    (1..=6).contains(&integer).then_some(CoordinateSpec {
        omission,
        integer,
        decimal,
    })
}

fn parse_legacy_coordinate(raw: &str, spec: CoordinateSpec) -> Result<f64, S1Error> {
    let (negative, digits) = match raw.as_bytes().first() {
        Some(b'-') => (true, &raw[1..]),
        Some(b'+') => (false, &raw[1..]),
        _ => (false, raw),
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(semantic(0, "invalid legacy coordinate"));
    }
    let mut normalized = digits.to_string();
    if spec.omission == b'T' {
        let width = spec.integer + spec.decimal;
        if normalized.len() > width {
            return Err(semantic(0, "legacy coordinate exceeds FS width"));
        }
        normalized.extend(std::iter::repeat_n('0', width - normalized.len()));
    }
    let integer = normalized
        .parse::<u64>()
        .map_err(|_| semantic(0, "legacy coordinate is out of range"))?;
    let value = integer as f64 / 10_f64.powi(spec.decimal as i32);
    let value = if negative { -value } else { value };
    value
        .is_finite()
        .then_some(value)
        .ok_or_else(|| semantic(0, "legacy coordinate is non-finite"))
}

fn encode_legacy_coordinate(value: f64, spec: CoordinateSpec) -> Result<String, S1Error> {
    let scaled = (value * 10_f64.powi(spec.decimal as i32)).round();
    let width = spec.integer + spec.decimal;
    if !scaled.is_finite() || scaled.abs() >= 10_f64.powi(width as i32) {
        return Err(semantic(0, "legacy coordinate cannot be normalized"));
    }
    let scaled = scaled as i128;
    if scaled < 0 {
        Ok(format!("-{abs:0width$}", abs = -scaled, width = width))
    } else {
        Ok(format!("{scaled:0width$}", width = width))
    }
}

fn normalize_legacy_coordinates(
    line: &str,
    spec: CoordinateSpec,
    mode: CoordinateMode,
    current: &mut Option<MmPoint>,
    _io_offset: MmPoint,
    line_no: usize,
) -> Result<String, S1Error> {
    let bytes = line.as_bytes();
    let mut x: Option<(usize, usize, f64)> = None;
    let mut y: Option<(usize, usize, f64)> = None;
    let mut offsets: Vec<(usize, usize, f64)> = Vec::new();
    let mut index = 0usize;
    while index < bytes.len() {
        if !matches!(bytes[index], b'X' | b'Y' | b'I' | b'J') {
            index += 1;
            continue;
        }
        let marker = bytes[index];
        let value_start = index + 1;
        let mut value_end = value_start;
        if bytes
            .get(value_end)
            .is_some_and(|byte| matches!(byte, b'+' | b'-'))
        {
            value_end += 1;
        }
        while bytes.get(value_end).is_some_and(u8::is_ascii_digit) {
            value_end += 1;
        }
        if value_end == value_start
            || (value_end == value_start + 1 && matches!(bytes[value_start], b'+' | b'-'))
        {
            return Err(S1Error::Syntax {
                line: line_no,
                message: "legacy coordinate has no digits".into(),
            });
        }
        let value = parse_legacy_coordinate(&line[value_start..value_end], spec).map_err(|_| {
            S1Error::Semantic {
                line: line_no,
                message: "legacy coordinate is invalid".into(),
            }
        })?;
        match marker {
            b'X' | b'Y' => {
                let slot = if marker == b'X' { &mut x } else { &mut y };
                if slot.replace((value_start, value_end, value)).is_some() {
                    return Err(S1Error::Semantic {
                        line: line_no,
                        message: "legacy operation repeats an X/Y coordinate".into(),
                    });
                }
            }
            b'I' | b'J' => offsets.push((value_start, value_end, value)),
            _ => unreachable!(),
        }
        index = value_end;
    }
    if x.is_none() && y.is_none() && offsets.is_empty() {
        return Ok(line.to_string());
    }
    // `point_from_coords` applies IO once after parsing.  Keep the normalized
    // modal position in the source coordinate system with an origin of zero
    // so an incremental first move does not receive IO twice.
    let has_endpoint = x.is_some() || y.is_some();
    let first_incremental = matches!(mode, CoordinateMode::Incremental) && current.is_none();
    let base = current.unwrap_or(MmPoint::new(0.0, 0.0));
    let next = match mode {
        CoordinateMode::Absolute => MmPoint::new(
            x.map_or(base.x_mm, |(_, _, value)| value),
            y.map_or(base.y_mm, |(_, _, value)| value),
        ),
        CoordinateMode::Incremental => MmPoint::new(
            base.x_mm + x.map_or(0.0, |(_, _, value)| value),
            base.y_mm + y.map_or(0.0, |(_, _, value)| value),
        ),
    };
    if !next.is_finite() {
        return Err(S1Error::Semantic {
            line: line_no,
            message: "legacy coordinate became non-finite".into(),
        });
    }
    if has_endpoint {
        *current = Some(next);
    }
    let mut rewritten = line.to_string();
    let mut replacements = Vec::new();
    if let Some((start, end, _)) = x {
        replacements.push((start, end, encode_legacy_coordinate(next.x_mm, spec)?));
    }
    if let Some((start, end, _)) = y {
        replacements.push((start, end, encode_legacy_coordinate(next.y_mm, spec)?));
    }
    for (start, end, value) in offsets {
        replacements.push((start, end, encode_legacy_coordinate(value, spec)?));
    }
    replacements.sort_by_key(|(start, _, _)| *start);
    for (start, end, replacement) in replacements.into_iter().rev() {
        rewritten.replace_range(start..end, &replacement);
    }
    if first_incremental && has_endpoint && (x.is_none() || y.is_none()) {
        let missing_x = x.is_none();
        let missing_y = y.is_none();
        let x_value = encode_legacy_coordinate(next.x_mm, spec)?;
        let y_value = encode_legacy_coordinate(next.y_mm, spec)?;
        if missing_x {
            let insert_at = rewritten
                .find('Y')
                .or_else(|| rewritten.find('D'))
                .unwrap_or(rewritten.len());
            rewritten.insert_str(insert_at, &format!("X{x_value}"));
        }
        if missing_y {
            let insert_at = rewritten.find('D').unwrap_or(rewritten.len());
            rewritten.insert_str(insert_at, &format!("Y{y_value}"));
        }
    }
    Ok(rewritten)
}

fn tokenize_source(source: &str, max_commands: usize) -> Result<Vec<(String, usize)>, S1Error> {
    let bytes = source.as_bytes();
    let mut cursor = 0usize;
    let mut line_no = 1usize;
    let mut token_count = 0usize;
    let mut commands = Vec::new();
    while cursor < bytes.len() {
        while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            if bytes[cursor] == b'\n' {
                line_no += 1;
            }
            cursor += 1;
        }
        if cursor == bytes.len() {
            break;
        }
        let start = cursor;
        let start_line = line_no;
        if bytes[cursor] == b'%' {
            let is_macro = source[start..].starts_with("%AM");
            let (end, closing_star) =
                find_extended_end(bytes, start + if is_macro { 3 } else { 1 }, is_macro)
                    .ok_or_else(|| S1Error::Syntax {
                        line: start_line,
                        message: "unterminated extended command".into(),
                    })?;
            if is_macro {
                checked_token_count(
                    &mut token_count,
                    source[start..end]
                        .bytes()
                        .filter(|byte| *byte == b'*')
                        .count(),
                    max_commands,
                )?;
                let mut token = source[start..end].trim().to_string();
                normalize_macro_closer(&mut token);
                if !token.is_empty() {
                    commands.push((token, start_line));
                }
            } else {
                let body = &source[start + 1..closing_star];
                let mut part_line = start_line;
                for part in body.split('*') {
                    checked_token_count(&mut token_count, 1, max_commands)?;
                    let trimmed = part.trim();
                    if !trimmed.is_empty() {
                        commands.push((format!("%{trimmed}*%"), part_line));
                    }
                    part_line += part.bytes().filter(|byte| *byte == b'\n').count();
                }
            }
            line_no += source[start..end]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count();
            cursor = end;
            continue;
        }

        let mut end = start;
        while bytes.get(end).is_some_and(|byte| *byte != b'*') {
            end += 1;
        }
        if end == bytes.len() {
            return Err(S1Error::Syntax {
                line: start_line,
                message: "unterminated function command".into(),
            });
        }
        checked_token_count(&mut token_count, 1, max_commands)?;
        let token = source[start..=end].trim();
        if !token.is_empty() {
            commands.push((token.to_string(), start_line));
        }
        line_no += source[start..=end]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count();
        cursor = end + 1;
    }
    Ok(commands)
}

fn checked_token_count(total: &mut usize, additional: usize, limit: usize) -> Result<(), S1Error> {
    *total = total.saturating_add(additional);
    if *total > limit {
        return Err(S1Error::ResourceLimit {
            resource: "parser_commands",
            limit,
            actual: *total,
        });
    }
    Ok(())
}

fn find_extended_end(
    bytes: &[u8],
    mut cursor: usize,
    allow_whitespace: bool,
) -> Option<(usize, usize)> {
    while cursor < bytes.len() {
        if bytes[cursor] == b'*' {
            let closing_star = cursor;
            cursor += 1;
            if allow_whitespace {
                while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
                    cursor += 1;
                }
            }
            if bytes.get(cursor) == Some(&b'%') {
                return Some((cursor + 1, closing_star));
            }
        } else {
            cursor += 1;
        }
    }
    None
}

fn normalize_macro_closer(token: &mut String) {
    let Some(percent) = token.len().checked_sub(1) else {
        return;
    };
    if token.as_bytes().get(percent) != Some(&b'%') {
        return;
    }
    let Some(star) = token[..percent].rfind('*') else {
        return;
    };
    if token[star + 1..percent]
        .bytes()
        .all(|byte| byte.is_ascii_whitespace())
    {
        token.replace_range(star + 1..percent, "");
    }
}

fn parse_io_body(body: &str) -> Option<(f64, f64)> {
    let body = body.strip_prefix('A')?;
    let separator = body.find('B')?;
    let a = body[..separator].parse::<f64>().ok()?;
    let b = body[separator + 1..].parse::<f64>().ok()?;
    (a.is_finite() && b.is_finite()).then_some((a, b))
}

fn parse_fsd_width(line: &str) -> Option<usize> {
    if !line.starts_with("%FSD") || !line.ends_with("*%") {
        return None;
    }
    let x = line.find('X')? + 1;
    let y = line[x..].find('Y')? + x;
    let x_digits = &line[x..y];
    let y_digits = line[y + 1..].strip_suffix("*%")?;
    if x_digits.len() != 2 || y_digits.len() != 2 {
        return None;
    }
    let x_width = x_digits[0..1].parse::<usize>().ok()? + x_digits[1..2].parse::<usize>().ok()?;
    let y_width = y_digits[0..1].parse::<usize>().ok()? + y_digits[1..2].parse::<usize>().ok()?;
    if x_width != y_width || !(2..=12).contains(&x_width) {
        return None;
    }
    Some(x_width)
}

fn check_fsd_coordinate_width(line: &str, width: usize, line_no: usize) -> Result<(), S1Error> {
    // Extended headers and comments contain arbitrary letters; only image
    // coordinate records are subject to FSD's explicit-width requirement.
    if line.starts_with('%') || line.starts_with("G04") {
        return Ok(());
    }
    let bytes = line.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        if !matches!(byte, b'X' | b'Y' | b'I' | b'J') {
            continue;
        }
        let mut cursor = index + 1;
        if bytes
            .get(cursor)
            .is_some_and(|byte| matches!(byte, b'+' | b'-'))
        {
            cursor += 1;
        }
        let start = cursor;
        while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
            cursor += 1;
        }
        if cursor > start && cursor - start != width {
            return Err(S1Error::Semantic {
                line: line_no,
                message: format!("FSD coordinate {byte} must have exactly {width} digits"),
            });
        }
    }
    Ok(())
}

fn interpret_s1(
    doc: GerberDoc,
    document_id: &str,
    mut metadata: SourceMetadata,
    io_offset_raw: MmPoint,
    budget: S1Budget,
) -> Result<S1Scene, S1Error> {
    let format = doc.format_specification.ok_or_else(|| S1Error::Semantic {
        line: 0,
        message: "FS is required".into(),
    })?;
    if !(1..=6).contains(&format.integer) || !(1..=6).contains(&format.decimal) {
        return Err(S1Error::Semantic {
            line: 0,
            message: "FS integer/decimal digits are outside the supported range".into(),
        });
    }
    let unit = doc.units.ok_or_else(|| S1Error::Semantic {
        line: 0,
        message: "MO is required".into(),
    })?;
    let unit_scale = match unit {
        Unit::Millimeters => 1.0,
        Unit::Inches => 25.4,
    };
    let input_coordinate_mode = format.coordinate_mode;
    let semantic_format = SemanticFormat {
        integer: format.integer,
        decimal: format.decimal,
        leading_zero_omission: matches!(
            format.zero_omission,
            gerber_parser::gerber_types::ZeroOmission::Leading
        ),
        absolute: true,
    };
    let mut macro_defs = HashMap::new();
    let mut macro_expansions = 0usize;
    for command in doc.commands() {
        if let Command::ExtendedCode(ExtendedCode::ApertureMacro(definition)) = command
            && macro_defs
                .insert(definition.name.clone(), definition.clone())
                .is_some()
        {
            return Err(S1Error::Semantic {
                line: 0,
                message: format!("duplicate aperture macro {}", definition.name),
            });
        }
    }
    // Validate the macro language independently of any one invocation.  The
    // argument count is taken from every actual AD use, so a parameterized
    // macro is never evaluated with invented values.  Constant, uninstantiated
    // definitions are evaluated once with an empty argument list; used
    // definitions are fully evaluated by convert_aperture below.
    let mut macro_formal_counts: HashMap<String, usize> = HashMap::new();
    for aperture in doc.apertures.values() {
        if let Aperture::Macro(name, args) = aperture {
            let count = args.as_ref().map_or(0, Vec::len);
            macro_formal_counts
                .entry(name.clone())
                .and_modify(|current| *current = (*current).max(count))
                .or_insert(count);
        }
    }
    for (name, definition) in &macro_defs {
        let Some(formal_count) = macro_formal_counts.get(name).copied() else {
            let has_unbound = validate_macro_definition(definition, 0, true, budget)?;
            validate_known_macro_geometry(definition, unit_scale, name, budget)?;
            if !has_unbound {
                let primitives = expand_macro(definition, &[], 1.0, budget, &mut macro_expansions)?;
                validate_macro_shape(primitives, name)?;
            }
            continue;
        };
        validate_macro_definition(definition, formal_count, false, budget)?;
        validate_known_macro_geometry(definition, unit_scale, name, budget)?;
        if formal_count == 0 {
            let primitives = expand_macro(definition, &[], 1.0, budget, &mut macro_expansions)?;
            validate_macro_shape(primitives, name)?;
        }
    }
    let mut codes: Vec<_> = doc.apertures.keys().copied().collect();
    codes.sort_unstable();
    let mut apertures = Vec::with_capacity(codes.len());
    for code in codes {
        let source = doc.apertures.get(&code).ok_or_else(|| S1Error::Semantic {
            line: 0,
            message: format!("missing aperture D{code}"),
        })?;
        let shape = convert_aperture(
            source,
            unit_scale,
            &macro_defs,
            budget,
            &mut macro_expansions,
        )?;
        apertures.push(ApertureDefinition {
            id: aperture_id(code),
            source_dcode: code,
            shape,
        });
    }
    let mut document = SemanticDocument {
        id: document_id.to_string(),
        unit: "mm".into(),
        format: semantic_format,
        layers: vec![SemanticLayer {
            locked: false,
            id: "layer-1".into(),
            name: metadata
                .layer_name
                .clone()
                .or_else(|| doc.image_name.clone())
                .unwrap_or_else(|| "Gerber layer".into()),
            objects: Vec::new(),
        }],
        apertures,
        source: metadata.clone(),
    };
    metadata.image_name = doc.image_name.clone();
    document.source = metadata.clone();
    let io_offset = MmPoint::new(
        io_offset_raw.x_mm * unit_scale,
        io_offset_raw.y_mm * unit_scale,
    );
    interpret_commands(
        &doc,
        &mut document,
        unit_scale,
        unit,
        input_coordinate_mode,
        io_offset,
        budget,
    )?;
    document.validate().map_err(|error| {
        semantic(
            0,
            format!("{error}; {:?}", document.arc_deviation_summary()),
        )
    })?;
    Ok(S1Scene {
        document,
        metadata,
        diagnostics: vec!["S1 normalized semantic model".into()],
        budget,
    })
}

fn aperture_id(code: i32) -> String {
    format!("aperture-{code}")
}

fn convert_aperture(
    aperture: &Aperture,
    scale: f64,
    macros: &HashMap<String, gerber_parser::gerber_types::ApertureMacro>,
    budget: S1Budget,
    macro_expansions: &mut usize,
) -> Result<ApertureShape, S1Error> {
    let shape = match aperture {
        Aperture::Circle(circle) => ApertureShape::Circle {
            diameter_mm: checked_value(circle.diameter, scale, "circle diameter")?,
            hole_diameter_mm: circle
                .hole_diameter
                .map(|value| checked_value(value, scale, "circle hole"))
                .transpose()?,
        },
        Aperture::Rectangle(rectangle) => ApertureShape::Rectangle {
            width_mm: checked_value(rectangle.x, scale, "rectangle width")?,
            height_mm: checked_value(rectangle.y, scale, "rectangle height")?,
            hole_diameter_mm: rectangle
                .hole_diameter
                .map(|value| checked_value(value, scale, "rectangle hole"))
                .transpose()?,
        },
        Aperture::Obround(rectangle) => ApertureShape::Obround {
            width_mm: checked_value(rectangle.x, scale, "obround width")?,
            height_mm: checked_value(rectangle.y, scale, "obround height")?,
            hole_diameter_mm: rectangle
                .hole_diameter
                .map(|value| checked_value(value, scale, "obround hole"))
                .transpose()?,
        },
        Aperture::Polygon(polygon) => ApertureShape::Polygon {
            diameter_mm: checked_value(polygon.diameter, scale, "polygon diameter")?,
            vertices: polygon.vertices,
            rotation_deg: polygon.rotation.unwrap_or(0.0),
            hole_diameter_mm: polygon
                .hole_diameter
                .map(|value| checked_value(value, scale, "polygon hole"))
                .transpose()?,
        },
        Aperture::Macro(name, args) => {
            let definition = macros.get(name).ok_or_else(|| S1Error::Semantic {
                line: 0,
                message: format!("aperture macro {name} is not defined"),
            })?;
            ApertureShape::Macro {
                primitives: expand_macro(
                    definition,
                    args.as_deref().unwrap_or(&[]),
                    scale,
                    budget,
                    macro_expansions,
                )?,
            }
        }
    };
    if !shape_is_valid(&shape) {
        return Err(S1Error::Semantic {
            line: 0,
            message: "aperture dimensions are invalid".into(),
        });
    }
    Ok(shape)
}

fn validate_macro_shape(primitives: Vec<MacroPrimitive>, name: &str) -> Result<(), S1Error> {
    let document = SemanticDocument {
        id: "macro-validation".into(),
        unit: "mm".into(),
        format: SemanticFormat {
            integer: 2,
            decimal: 6,
            leading_zero_omission: true,
            absolute: true,
        },
        layers: Vec::new(),
        apertures: vec![ApertureDefinition {
            id: "aperture-10".into(),
            source_dcode: 10,
            shape: ApertureShape::Macro { primitives },
        }],
        source: SourceMetadata::default(),
    };
    document.validate().map_err(|error| match error {
        SemanticError::ResourceLimit {
            resource,
            limit,
            actual,
        } => S1Error::ResourceLimit {
            resource,
            limit,
            actual,
        },
        other => semantic(0, format!("invalid aperture macro {name}: {other}")),
    })?;
    Ok(())
}

fn shape_is_valid(shape: &ApertureShape) -> bool {
    match shape {
        ApertureShape::Circle {
            diameter_mm,
            hole_diameter_mm,
        } => {
            finite_positive(*diameter_mm)
                && hole_diameter_mm.is_none_or(|hole| finite_positive(hole) && hole < *diameter_mm)
        }
        ApertureShape::Rectangle {
            width_mm,
            height_mm,
            hole_diameter_mm,
        }
        | ApertureShape::Obround {
            width_mm,
            height_mm,
            hole_diameter_mm,
        } => {
            finite_positive(*width_mm)
                && finite_positive(*height_mm)
                && hole_diameter_mm
                    .is_none_or(|hole| finite_positive(hole) && hole < width_mm.min(*height_mm))
        }
        ApertureShape::Polygon {
            diameter_mm,
            vertices,
            rotation_deg,
            hole_diameter_mm,
        } => {
            finite_positive(*diameter_mm)
                && (3..=12).contains(vertices)
                && rotation_deg.is_finite()
                && hole_diameter_mm.is_none_or(|hole| finite_positive(hole) && hole < *diameter_mm)
        }
        ApertureShape::Macro { primitives } => {
            !primitives.is_empty()
                && primitives.iter().all(|primitive| match primitive {
                    MacroPrimitive::Circle {
                        diameter_mm,
                        center,
                        rotation_deg,
                        ..
                    } => {
                        finite_positive(*diameter_mm)
                            && center.is_finite()
                            && rotation_deg.is_finite()
                    }
                    MacroPrimitive::CenterLine {
                        width_mm,
                        height_mm,
                        center,
                        rotation_deg,
                        ..
                    } => {
                        finite_positive(*width_mm)
                            && finite_positive(*height_mm)
                            && center.is_finite()
                            && rotation_deg.is_finite()
                    }
                    MacroPrimitive::Outline {
                        points,
                        rotation_deg,
                        ..
                    } => {
                        points.len() >= 4
                            && points.first() == points.last()
                            && points.iter().all(|point| point.is_finite())
                            && rotation_deg.is_finite()
                    }
                })
        }
    }
}

fn finite_positive(value: f64) -> bool {
    value.is_finite() && value > editor_core::EPSILON_MM
}

fn validate_macro_definition(
    definition: &gerber_parser::gerber_types::ApertureMacro,
    formal_count: usize,
    allow_unbound: bool,
    budget: S1Budget,
) -> Result<bool, S1Error> {
    let mut defined = HashSet::new();
    let mut has_unbound = false;
    for content in &definition.content {
        match content {
            MacroContent::VariableDefinition(variable) => {
                has_unbound |= validate_macro_expression(
                    &variable.expression,
                    &defined,
                    formal_count,
                    allow_unbound,
                    budget,
                )?;
                defined.insert(variable.number);
            }
            MacroContent::Circle(circle) => {
                has_unbound |= validate_macro_boolean(
                    &circle.exposure,
                    &defined,
                    formal_count,
                    allow_unbound,
                    budget,
                )?;
                has_unbound |= validate_macro_decimal(
                    &circle.diameter,
                    &defined,
                    formal_count,
                    allow_unbound,
                    budget,
                )?;
                has_unbound |= validate_macro_decimal(
                    &circle.center.0,
                    &defined,
                    formal_count,
                    allow_unbound,
                    budget,
                )?;
                has_unbound |= validate_macro_decimal(
                    &circle.center.1,
                    &defined,
                    formal_count,
                    allow_unbound,
                    budget,
                )?;
                if let Some(angle) = &circle.angle {
                    has_unbound |= validate_macro_decimal(
                        angle,
                        &defined,
                        formal_count,
                        allow_unbound,
                        budget,
                    )?;
                }
            }
            MacroContent::CenterLine(line) => {
                has_unbound |= validate_macro_boolean(
                    &line.exposure,
                    &defined,
                    formal_count,
                    allow_unbound,
                    budget,
                )?;
                has_unbound |= validate_macro_decimal(
                    &line.dimensions.0,
                    &defined,
                    formal_count,
                    allow_unbound,
                    budget,
                )?;
                has_unbound |= validate_macro_decimal(
                    &line.dimensions.1,
                    &defined,
                    formal_count,
                    allow_unbound,
                    budget,
                )?;
                has_unbound |= validate_macro_decimal(
                    &line.center.0,
                    &defined,
                    formal_count,
                    allow_unbound,
                    budget,
                )?;
                has_unbound |= validate_macro_decimal(
                    &line.center.1,
                    &defined,
                    formal_count,
                    allow_unbound,
                    budget,
                )?;
                has_unbound |= validate_macro_decimal(
                    &line.angle,
                    &defined,
                    formal_count,
                    allow_unbound,
                    budget,
                )?;
            }
            MacroContent::Outline(outline) => {
                has_unbound |= validate_macro_boolean(
                    &outline.exposure,
                    &defined,
                    formal_count,
                    allow_unbound,
                    budget,
                )?;
                for (x, y) in &outline.points {
                    has_unbound |=
                        validate_macro_decimal(x, &defined, formal_count, allow_unbound, budget)?;
                    has_unbound |=
                        validate_macro_decimal(y, &defined, formal_count, allow_unbound, budget)?;
                }
                has_unbound |= validate_macro_decimal(
                    &outline.angle,
                    &defined,
                    formal_count,
                    allow_unbound,
                    budget,
                )?;
            }
            MacroContent::Comment(_) => {}
            MacroContent::VectorLine(_) => {
                return Err(S1Error::Unsupported {
                    line: 0,
                    feature: "AM primitive 20".into(),
                });
            }
            MacroContent::Polygon(_) | MacroContent::Moire(_) | MacroContent::Thermal(_) => {
                return Err(S1Error::Unsupported {
                    line: 0,
                    feature: "unsupported AM primitive".into(),
                });
            }
        }
    }
    Ok(has_unbound)
}

fn validate_known_macro_geometry(
    definition: &gerber_parser::gerber_types::ApertureMacro,
    scale: f64,
    name: &str,
    budget: S1Budget,
) -> Result<(), S1Error> {
    let mut known = HashMap::new();
    for content in &definition.content {
        match content {
            MacroContent::VariableDefinition(variable) => {
                match eval_expression_partial(&variable.expression, &known)? {
                    Some(value) => {
                        known.insert(variable.number, value);
                    }
                    None => {
                        known.remove(&variable.number);
                    }
                }
            }
            MacroContent::Circle(circle) => {
                if let Some(diameter) = known_macro_decimal(&circle.diameter, &known)?
                    && !finite_positive(diameter * scale)
                {
                    return Err(semantic(
                        0,
                        format!("invalid constant circle in aperture macro {name}"),
                    ));
                }
                if let Some(angle) = circle
                    .angle
                    .as_ref()
                    .map(|value| known_macro_decimal(value, &known))
                    .transpose()?
                    .flatten()
                {
                    checked_angle(angle)?;
                }
            }
            MacroContent::CenterLine(line) => {
                for value in [&line.dimensions.0, &line.dimensions.1] {
                    if let Some(value) = known_macro_decimal(value, &known)?
                        && !finite_positive(value * scale)
                    {
                        return Err(semantic(
                            0,
                            format!("invalid constant center line in aperture macro {name}"),
                        ));
                    }
                }
                if let Some(angle) = known_macro_decimal(&line.angle, &known)? {
                    checked_angle(angle)?;
                }
            }
            MacroContent::Outline(outline) => {
                let mut points = Vec::with_capacity(outline.points.len());
                let mut all_known = true;
                for (x, y) in &outline.points {
                    let x = known_macro_decimal(x, &known)?;
                    let y = known_macro_decimal(y, &known)?;
                    let (Some(x), Some(y)) = (x, y) else {
                        all_known = false;
                        break;
                    };
                    points.push(MmPoint::new(x * scale, y * scale));
                }
                if !all_known {
                    continue;
                }
                check_macro_outline_budget(points.len(), budget)?;
                if points.len() < 4
                    || points.first() != points.last()
                    || !points.iter().all(|point| point.is_finite())
                    || !macro_outline_is_simple(&points)
                {
                    return Err(semantic(
                        0,
                        format!("invalid constant outline in aperture macro {name}"),
                    ));
                }
            }
            MacroContent::Comment(_)
            | MacroContent::VectorLine(_)
            | MacroContent::Polygon(_)
            | MacroContent::Moire(_)
            | MacroContent::Thermal(_) => {}
        }
    }
    Ok(())
}

fn known_macro_decimal(
    value: &MacroDecimal,
    known: &HashMap<u32, f64>,
) -> Result<Option<f64>, S1Error> {
    match value {
        MacroDecimal::Value(value) if value.is_finite() => Ok(Some(*value)),
        MacroDecimal::Variable(number) => Ok(known.get(number).copied()),
        MacroDecimal::Expression(expression) => eval_expression_partial(expression, known),
        MacroDecimal::Value(_) => Err(semantic(0, "macro value is non-finite")),
    }
}

fn check_macro_outline_budget(points: usize, budget: S1Budget) -> Result<(), S1Error> {
    if points > budget.max_region_edges {
        return Err(S1Error::ResourceLimit {
            resource: "am_outline_points",
            limit: budget.max_region_edges,
            actual: points,
        });
    }
    let candidates = points.saturating_mul(points.saturating_sub(1)) / 2;
    let limit = budget.max_region_edges.saturating_mul(2);
    if candidates > limit {
        return Err(S1Error::ResourceLimit {
            resource: "am_outline_candidates",
            limit,
            actual: candidates,
        });
    }
    Ok(())
}

fn macro_outline_is_simple(points: &[MmPoint]) -> bool {
    if points.len() < 4 || points.first() != points.last() {
        return false;
    }
    let edge_count = points.len() - 1;
    for first in 0..edge_count {
        for second in (first + 1)..edge_count {
            if second == first + 1 || (first == 0 && second + 1 == edge_count) {
                continue;
            }
            if segments_intersect(
                points[first],
                points[first + 1],
                points[second],
                points[second + 1],
            ) {
                return false;
            }
        }
    }
    true
}

fn segments_intersect(a: MmPoint, b: MmPoint, c: MmPoint, d: MmPoint) -> bool {
    fn orientation(a: MmPoint, b: MmPoint, c: MmPoint) -> f64 {
        (b.x_mm - a.x_mm) * (c.y_mm - a.y_mm) - (b.y_mm - a.y_mm) * (c.x_mm - a.x_mm)
    }
    fn on_segment(a: MmPoint, b: MmPoint, point: MmPoint) -> bool {
        point.x_mm >= a.x_mm.min(b.x_mm)
            && point.x_mm <= a.x_mm.max(b.x_mm)
            && point.y_mm >= a.y_mm.min(b.y_mm)
            && point.y_mm <= a.y_mm.max(b.y_mm)
    }
    let ab_c = orientation(a, b, c);
    let ab_d = orientation(a, b, d);
    let cd_a = orientation(c, d, a);
    let cd_b = orientation(c, d, b);
    if ab_c == 0.0 && on_segment(a, b, c)
        || ab_d == 0.0 && on_segment(a, b, d)
        || cd_a == 0.0 && on_segment(c, d, a)
        || cd_b == 0.0 && on_segment(c, d, b)
    {
        return true;
    }
    (ab_c > 0.0) != (ab_d > 0.0) && (cd_a > 0.0) != (cd_b > 0.0)
}

fn validate_macro_boolean(
    value: &MacroBoolean,
    defined: &HashSet<u32>,
    formal_count: usize,
    allow_unbound: bool,
    budget: S1Budget,
) -> Result<bool, S1Error> {
    match value {
        MacroBoolean::Value(_) => Ok(false),
        MacroBoolean::Variable(number) => {
            validate_macro_variable(*number, defined, formal_count, allow_unbound)
        }
        MacroBoolean::Expression(expression) => {
            validate_macro_expression(expression, defined, formal_count, allow_unbound, budget)
        }
    }
}

fn validate_macro_decimal(
    value: &MacroDecimal,
    defined: &HashSet<u32>,
    formal_count: usize,
    allow_unbound: bool,
    budget: S1Budget,
) -> Result<bool, S1Error> {
    match value {
        MacroDecimal::Value(value) if value.is_finite() => Ok(false),
        MacroDecimal::Value(_) => Err(semantic(0, "macro value is non-finite")),
        MacroDecimal::Variable(number) => {
            validate_macro_variable(*number, defined, formal_count, allow_unbound)
        }
        MacroDecimal::Expression(expression) => {
            validate_macro_expression(expression, defined, formal_count, allow_unbound, budget)
        }
    }
}

fn validate_macro_variable(
    number: u32,
    defined: &HashSet<u32>,
    formal_count: usize,
    allow_unbound: bool,
) -> Result<bool, S1Error> {
    if defined.contains(&number) || (1..=formal_count as u32).contains(&number) {
        Ok(false)
    } else if allow_unbound {
        Ok(true)
    } else {
        Err(semantic(0, format!("undefined macro variable ${number}")))
    }
}

fn validate_macro_expression(
    expression: &str,
    defined: &HashSet<u32>,
    formal_count: usize,
    allow_unbound: bool,
    _budget: S1Budget,
) -> Result<bool, S1Error> {
    #[derive(Clone, Copy)]
    enum Expect {
        Operand,
        Operator,
    }
    let mut chars = expression.chars().peekable();
    let mut expect = Expect::Operand;
    let mut depth = 0usize;
    let mut tokens = 0usize;
    let mut has_unbound_variable = false;
    while let Some(character) = chars.peek().copied() {
        if character.is_ascii_whitespace() {
            chars.next();
            continue;
        }
        tokens = tokens.saturating_add(1);
        if tokens > S1_MAX_AM_EXPRESSION_TOKENS {
            return Err(S1Error::ResourceLimit {
                resource: "am_expression_tokens",
                limit: S1_MAX_AM_EXPRESSION_TOKENS,
                actual: tokens,
            });
        }
        match character {
            '+' | '-' if matches!(expect, Expect::Operand) => {
                chars.next();
            }
            '+' | '-' | 'x' | 'X' | '/' | '*' if matches!(expect, Expect::Operator) => {
                chars.next();
                expect = Expect::Operand;
            }
            '(' if matches!(expect, Expect::Operand) => {
                chars.next();
                depth += 1;
                if depth > S1_MAX_AM_EXPRESSION_DEPTH {
                    return Err(S1Error::ResourceLimit {
                        resource: "am_expression_depth",
                        limit: S1_MAX_AM_EXPRESSION_DEPTH,
                        actual: depth,
                    });
                }
            }
            ')' if matches!(expect, Expect::Operator) && depth > 0 => {
                chars.next();
                depth -= 1;
            }
            '$' if matches!(expect, Expect::Operand) => {
                chars.next();
                let mut number = 0u32;
                let mut digits = 0usize;
                while chars.peek().is_some_and(char::is_ascii_digit) {
                    digits += 1;
                    number = number
                        .checked_mul(10)
                        .and_then(|value| {
                            value.checked_add(chars.next().unwrap() as u32 - '0' as u32)
                        })
                        .ok_or_else(|| semantic(0, "invalid macro variable"))?;
                }
                if digits == 0 {
                    return Err(semantic(0, "invalid macro variable"));
                }
                let unbound =
                    validate_macro_variable(number, defined, formal_count, allow_unbound)?;
                has_unbound_variable |= unbound;
                expect = Expect::Operator;
            }
            value
                if matches!(expect, Expect::Operand)
                    && (value.is_ascii_digit() || value == '.') =>
            {
                let mut dots = 0usize;
                let mut digits = 0usize;
                while let Some(value) = chars.peek().copied() {
                    if value.is_ascii_digit() {
                        digits += 1;
                        chars.next();
                    } else if value == '.' {
                        dots += 1;
                        chars.next();
                    } else {
                        break;
                    }
                }
                if digits == 0 || dots > 1 {
                    return Err(semantic(0, "invalid macro number"));
                }
                expect = Expect::Operator;
            }
            _ => return Err(semantic(0, "invalid macro expression")),
        }
    }
    if matches!(expect, Expect::Operand) || depth != 0 {
        return Err(semantic(0, "unclosed or incomplete macro expression"));
    }
    // Evaluate with unknown variables so constant subexpressions are still
    // checked (for example `$99+1/0`).  Unknown bindings remain unknown and
    // are only resolved when an actual AD supplies the arguments.
    eval_expression_partial(expression, &HashMap::new())?;
    Ok(has_unbound_variable)
}

fn checked_value(value: f64, scale: f64, what: &'static str) -> Result<f64, S1Error> {
    let result = value * scale;
    if !value.is_finite() || !result.is_finite() || result.abs() > 1e9 {
        return Err(S1Error::Semantic {
            line: 0,
            message: format!("{what} is non-finite or out of range"),
        });
    }
    Ok(result)
}

fn expand_macro(
    definition: &gerber_parser::gerber_types::ApertureMacro,
    args: &[MacroDecimal],
    scale: f64,
    budget: S1Budget,
    macro_expansions: &mut usize,
) -> Result<Vec<MacroPrimitive>, S1Error> {
    let mut variables = HashMap::new();
    for (index, argument) in args.iter().enumerate() {
        variables.insert((index + 1) as u32, eval_decimal(argument, &variables)?);
    }
    let mut primitives = Vec::new();
    for content in &definition.content {
        match content {
            MacroContent::VariableDefinition(variable) => {
                let value = eval_expression(&variable.expression, &variables)?;
                variables.insert(variable.number, value);
            }
            MacroContent::Circle(circle) => {
                consume_macro_expansion(macro_expansions, budget)?;
                primitives.push(MacroPrimitive::Circle {
                    exposure: eval_exposure(&circle.exposure, &variables)?,
                    diameter_mm: checked_value(
                        eval_decimal(&circle.diameter, &variables)?,
                        scale,
                        "macro circle diameter",
                    )?,
                    center: scaled_point(
                        eval_decimal(&circle.center.0, &variables)?,
                        eval_decimal(&circle.center.1, &variables)?,
                        scale,
                        circle
                            .angle
                            .as_ref()
                            .map(|angle| eval_decimal(angle, &variables))
                            .transpose()?
                            .unwrap_or(0.0),
                    )?,
                    rotation_deg: circle
                        .angle
                        .as_ref()
                        .map(|angle| eval_decimal(angle, &variables))
                        .transpose()?
                        .unwrap_or(0.0),
                });
            }
            MacroContent::CenterLine(line) => {
                consume_macro_expansion(macro_expansions, budget)?;
                let rotation = eval_decimal(&line.angle, &variables)?;
                primitives.push(MacroPrimitive::CenterLine {
                    exposure: eval_exposure(&line.exposure, &variables)?,
                    width_mm: checked_value(
                        eval_decimal(&line.dimensions.0, &variables)?,
                        scale,
                        "macro center-line width",
                    )?,
                    height_mm: checked_value(
                        eval_decimal(&line.dimensions.1, &variables)?,
                        scale,
                        "macro center-line height",
                    )?,
                    center: scaled_point(
                        eval_decimal(&line.center.0, &variables)?,
                        eval_decimal(&line.center.1, &variables)?,
                        scale,
                        rotation,
                    )?,
                    rotation_deg: rotation,
                });
            }
            MacroContent::Outline(outline) => {
                consume_macro_expansion(macro_expansions, budget)?;
                reserve_macro_expansions(macro_expansions, outline.points.len(), budget)?;
                let rotation = eval_decimal(&outline.angle, &variables)?;
                let mut points = Vec::with_capacity(outline.points.len());
                for (x, y) in &outline.points {
                    points.push(scaled_point(
                        eval_decimal(x, &variables)?,
                        eval_decimal(y, &variables)?,
                        scale,
                        rotation,
                    )?);
                }
                primitives.push(MacroPrimitive::Outline {
                    exposure: eval_exposure(&outline.exposure, &variables)?,
                    points,
                    rotation_deg: rotation,
                });
            }
            MacroContent::Comment(_) => {}
            MacroContent::VectorLine(_) => {
                return Err(S1Error::Unsupported {
                    line: 0,
                    feature: "AM primitive 20".into(),
                });
            }
            MacroContent::Polygon(_) | MacroContent::Moire(_) | MacroContent::Thermal(_) => {
                return Err(S1Error::Unsupported {
                    line: 0,
                    feature: "unsupported AM primitive".into(),
                });
            }
        }
    }
    if primitives.is_empty() {
        return Err(S1Error::Semantic {
            line: 0,
            message: "aperture macro has no supported primitives".into(),
        });
    }
    Ok(primitives)
}

fn consume_macro_expansion(total: &mut usize, budget: S1Budget) -> Result<(), S1Error> {
    reserve_macro_expansions(total, 1, budget)
}

fn reserve_macro_expansions(
    total: &mut usize,
    additional: usize,
    budget: S1Budget,
) -> Result<(), S1Error> {
    let actual = total.saturating_add(additional);
    check_resource("am_expansions", budget.max_am_expansions, actual)?;
    *total = actual;
    Ok(())
}

fn scaled_point(x: f64, y: f64, scale: f64, _rotation: f64) -> Result<MmPoint, S1Error> {
    let point = MmPoint::new(x * scale, y * scale);
    if !point.is_finite() {
        return Err(S1Error::Semantic {
            line: 0,
            message: "macro point is non-finite".into(),
        });
    }
    Ok(point)
}

fn eval_exposure(value: &MacroBoolean, variables: &HashMap<u32, f64>) -> Result<Exposure, S1Error> {
    let value = match value {
        MacroBoolean::Value(value) => f64::from(*value as u8),
        MacroBoolean::Variable(number) => {
            *variables.get(number).ok_or_else(|| S1Error::Semantic {
                line: 0,
                message: format!("undefined macro variable ${number}"),
            })?
        }
        MacroBoolean::Expression(expression) => eval_expression(expression, variables)?,
    };
    if !value.is_finite() || (value - 0.0).abs() > 1e-9 && (value - 1.0).abs() > 1e-9 {
        return Err(S1Error::Semantic {
            line: 0,
            message: "macro exposure must be 0 or 1".into(),
        });
    }
    Ok(if value >= 0.5 {
        Exposure::Dark
    } else {
        Exposure::Clear
    })
}

fn eval_decimal(value: &MacroDecimal, variables: &HashMap<u32, f64>) -> Result<f64, S1Error> {
    match value {
        MacroDecimal::Value(value) if value.is_finite() => Ok(*value),
        MacroDecimal::Value(_) => Err(S1Error::Semantic {
            line: 0,
            message: "macro value is non-finite".into(),
        }),
        MacroDecimal::Variable(number) => {
            variables
                .get(number)
                .copied()
                .ok_or_else(|| S1Error::Semantic {
                    line: 0,
                    message: format!("undefined macro variable ${number}"),
                })
        }
        MacroDecimal::Expression(expression) => eval_expression(expression, variables),
    }
}

/// Evaluate an expression while preserving unknown variable bindings.  This
/// is used for static template validation: an unknown `$n` does not make the
/// whole expression disappear, so errors in the known parts (especially
/// division by zero and non-finite arithmetic) remain observable.
fn eval_expression_partial(
    expression: &str,
    variables: &HashMap<u32, f64>,
) -> Result<Option<f64>, S1Error> {
    let mut tokens = 0usize;
    let mut depth = 0usize;
    for character in expression.chars() {
        if character.is_ascii_whitespace() {
            continue;
        }
        tokens = tokens.saturating_add(1);
        if tokens > S1_MAX_AM_EXPRESSION_TOKENS {
            return Err(S1Error::ResourceLimit {
                resource: "am_expression_tokens",
                limit: S1_MAX_AM_EXPRESSION_TOKENS,
                actual: tokens,
            });
        }
        match character {
            '(' => {
                depth += 1;
                if depth > S1_MAX_AM_EXPRESSION_DEPTH {
                    return Err(S1Error::ResourceLimit {
                        resource: "am_expression_depth",
                        limit: S1_MAX_AM_EXPRESSION_DEPTH,
                        actual: depth,
                    });
                }
            }
            ')' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    if depth != 0 {
        return Err(semantic(0, "unclosed macro expression"));
    }
    struct Parser<'a> {
        chars: std::iter::Peekable<std::str::Chars<'a>>,
        variables: &'a HashMap<u32, f64>,
        unary_depth: usize,
    }
    impl<'a> Parser<'a> {
        fn ws(&mut self) {
            while self.chars.next_if(|ch| ch.is_ascii_whitespace()).is_some() {}
        }

        fn expr(&mut self) -> Result<Option<f64>, S1Error> {
            let mut result = self.term()?;
            loop {
                self.ws();
                let Some(operator) = self.chars.next_if(|ch| *ch == '+' || *ch == '-') else {
                    break;
                };
                let rhs = self.term()?;
                result = match (result, rhs) {
                    (Some(left), Some(right)) => {
                        let value = if operator == '+' {
                            left + right
                        } else {
                            left - right
                        };
                        if !value.is_finite() {
                            return Err(semantic(0, "macro expression is non-finite"));
                        }
                        Some(value)
                    }
                    _ => None,
                };
            }
            Ok(result)
        }

        fn term(&mut self) -> Result<Option<f64>, S1Error> {
            let mut result = self.factor()?;
            loop {
                self.ws();
                let Some(operator) = self
                    .chars
                    .next_if(|ch| *ch == '/' || *ch == 'x' || *ch == 'X' || *ch == '*')
                else {
                    break;
                };
                let rhs = self.factor()?;
                if operator == '/' && rhs == Some(0.0) {
                    return Err(semantic(0, "division by zero in macro expression"));
                }
                result = match (result, rhs) {
                    (Some(left), Some(right)) => {
                        let value = if operator == '/' {
                            left / right
                        } else {
                            left * right
                        };
                        if !value.is_finite() {
                            return Err(semantic(0, "macro expression is non-finite"));
                        }
                        Some(value)
                    }
                    _ => None,
                };
            }
            Ok(result)
        }

        fn factor(&mut self) -> Result<Option<f64>, S1Error> {
            self.ws();
            if let Some(operator) = self.chars.next_if(|ch| *ch == '+' || *ch == '-') {
                self.unary_depth = self.unary_depth.saturating_add(1);
                if self.unary_depth > S1_MAX_AM_EXPRESSION_DEPTH {
                    let actual = self.unary_depth;
                    self.unary_depth -= 1;
                    return Err(S1Error::ResourceLimit {
                        resource: "am_expression_depth",
                        limit: S1_MAX_AM_EXPRESSION_DEPTH,
                        actual,
                    });
                }
                let result = self.factor();
                self.unary_depth -= 1;
                let value = result?;
                return Ok(value.map(|value| if operator == '-' { -value } else { value }));
            }
            if self.chars.next_if(|ch| *ch == '(').is_some() {
                let value = self.expr()?;
                self.ws();
                if self.chars.next_if(|ch| *ch == ')').is_none() {
                    return Err(semantic(0, "unclosed macro expression"));
                }
                return Ok(value);
            }
            if self.chars.next_if(|ch| *ch == '$').is_some() {
                let mut number = String::new();
                while let Some(ch) = self.chars.next_if(|ch| ch.is_ascii_digit()) {
                    number.push(ch);
                }
                if number.is_empty() {
                    return Err(semantic(0, "invalid macro variable"));
                }
                let number = number
                    .parse::<u32>()
                    .map_err(|_| semantic(0, "invalid macro variable"))?;
                return Ok(self.variables.get(&number).copied());
            }
            let mut number = String::new();
            while let Some(ch) = self.chars.next_if(|ch| ch.is_ascii_digit() || *ch == '.') {
                number.push(ch);
            }
            if number.is_empty() || number.matches('.').count() > 1 {
                return Err(semantic(0, "invalid macro expression"));
            }
            let value = number
                .parse::<f64>()
                .map_err(|_| semantic(0, "invalid macro number"))?;
            if !value.is_finite() {
                return Err(semantic(0, "macro expression is non-finite"));
            }
            Ok(Some(value))
        }
    }
    let mut parser = Parser {
        chars: expression.chars().peekable(),
        variables,
        unary_depth: 0,
    };
    let result = parser.expr()?;
    parser.ws();
    if parser.chars.next().is_some() {
        return Err(semantic(0, "invalid or non-finite macro expression"));
    }
    Ok(result)
}

fn eval_expression(expression: &str, variables: &HashMap<u32, f64>) -> Result<f64, S1Error> {
    let mut tokens = 0usize;
    let mut depth = 0usize;
    for character in expression.chars() {
        if character.is_ascii_whitespace() {
            continue;
        }
        tokens = tokens.saturating_add(1);
        if tokens > S1_MAX_AM_EXPRESSION_TOKENS {
            return Err(S1Error::ResourceLimit {
                resource: "am_expression_tokens",
                limit: S1_MAX_AM_EXPRESSION_TOKENS,
                actual: tokens,
            });
        }
        match character {
            '(' => {
                depth += 1;
                if depth > S1_MAX_AM_EXPRESSION_DEPTH {
                    return Err(S1Error::ResourceLimit {
                        resource: "am_expression_depth",
                        limit: S1_MAX_AM_EXPRESSION_DEPTH,
                        actual: depth,
                    });
                }
            }
            ')' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    if depth != 0 {
        return Err(S1Error::Semantic {
            line: 0,
            message: "unclosed macro expression".into(),
        });
    }
    struct Parser<'a> {
        chars: std::iter::Peekable<std::str::Chars<'a>>,
        variables: &'a HashMap<u32, f64>,
        unary_depth: usize,
    }
    impl<'a> Parser<'a> {
        fn ws(&mut self) {
            while self.chars.next_if(|ch| ch.is_ascii_whitespace()).is_some() {}
        }
        fn expr(&mut self) -> Result<f64, S1Error> {
            let mut result = self.term()?;
            loop {
                self.ws();
                let Some(operator) = self.chars.next_if(|ch| *ch == '+' || *ch == '-') else {
                    break;
                };
                let rhs = self.term()?;
                result = if operator == '+' {
                    result + rhs
                } else {
                    result - rhs
                };
                if !result.is_finite() {
                    return Err(S1Error::Semantic {
                        line: 0,
                        message: "macro expression is non-finite".into(),
                    });
                }
            }
            Ok(result)
        }
        fn term(&mut self) -> Result<f64, S1Error> {
            let mut result = self.factor()?;
            loop {
                self.ws();
                let Some(operator) = self
                    .chars
                    .next_if(|ch| *ch == '/' || *ch == 'x' || *ch == 'X' || *ch == '*')
                else {
                    break;
                };
                let rhs = self.factor()?;
                if operator == '/' {
                    if rhs == 0.0 {
                        return Err(S1Error::Semantic {
                            line: 0,
                            message: "division by zero in macro expression".into(),
                        });
                    }
                    result /= rhs;
                } else {
                    result *= rhs;
                }
                if !result.is_finite() {
                    return Err(S1Error::Semantic {
                        line: 0,
                        message: "macro expression is non-finite".into(),
                    });
                }
            }
            Ok(result)
        }
        fn factor(&mut self) -> Result<f64, S1Error> {
            self.ws();
            if let Some(operator) = self.chars.next_if(|ch| *ch == '+' || *ch == '-') {
                self.unary_depth = self.unary_depth.saturating_add(1);
                if self.unary_depth > S1_MAX_AM_EXPRESSION_DEPTH {
                    let actual = self.unary_depth;
                    self.unary_depth -= 1;
                    return Err(S1Error::ResourceLimit {
                        resource: "am_expression_depth",
                        limit: S1_MAX_AM_EXPRESSION_DEPTH,
                        actual,
                    });
                }
                let result = self.factor();
                self.unary_depth -= 1;
                let value = result?;
                return Ok(if operator == '-' { -value } else { value });
            }
            if self.chars.next_if(|ch| *ch == '(').is_some() {
                let value = self.expr()?;
                self.ws();
                if self.chars.next_if(|ch| *ch == ')').is_none() {
                    return Err(S1Error::Semantic {
                        line: 0,
                        message: "unclosed macro expression".into(),
                    });
                }
                return Ok(value);
            }
            if self.chars.next_if(|ch| *ch == '$').is_some() {
                let mut number = String::new();
                while let Some(ch) = self.chars.next_if(|ch| ch.is_ascii_digit()) {
                    number.push(ch);
                }
                let number = number.parse::<u32>().map_err(|_| S1Error::Semantic {
                    line: 0,
                    message: "invalid macro variable".into(),
                })?;
                return self
                    .variables
                    .get(&number)
                    .copied()
                    .ok_or_else(|| S1Error::Semantic {
                        line: 0,
                        message: format!("undefined macro variable ${number}"),
                    });
            }
            let mut number = String::new();
            while let Some(ch) = self.chars.next_if(|ch| ch.is_ascii_digit() || *ch == '.') {
                number.push(ch);
            }
            if number.is_empty() || number.matches('.').count() > 1 {
                return Err(S1Error::Semantic {
                    line: 0,
                    message: "invalid macro expression".into(),
                });
            }
            number.parse::<f64>().map_err(|_| S1Error::Semantic {
                line: 0,
                message: "invalid macro number".into(),
            })
        }
    }
    let mut parser = Parser {
        chars: expression.chars().peekable(),
        variables,
        unary_depth: 0,
    };
    let result = parser.expr()?;
    parser.ws();
    if parser.chars.next().is_some() || !result.is_finite() {
        return Err(S1Error::Semantic {
            line: 0,
            message: "invalid or non-finite macro expression".into(),
        });
    }
    Ok(result)
}

#[derive(Debug, Default)]
struct RegionState {
    contours: Vec<RegionContour>,
    edges: Vec<RegionEdge>,
    start: Option<MmPoint>,
    current: Option<MmPoint>,
    source_command: usize,
}

fn interpret_commands(
    doc: &GerberDoc,
    document: &mut SemanticDocument,
    unit_scale: f64,
    declared_unit: Unit,
    input_coordinate_mode: CoordinateMode,
    io_offset: MmPoint,
    budget: S1Budget,
) -> Result<(), S1Error> {
    let source_decimal = document
        .source
        .coordinate_format
        .as_deref()
        .and_then(|fs| fs.split_once('X'))
        .and_then(|(_, digits)| digits.chars().nth(1))
        .and_then(|digit| digit.to_digit(10))
        .unwrap_or(u32::from(document.format.decimal));
    let source_resolution = unit_scale * 10_f64.powi(-(source_decimal as i32));
    let mut active_aperture: Option<String> = None;
    let mut current: Option<MmPoint> = None;
    let mut polarity = Exposure::Dark;
    let mut interpolation = InterpolationMode::Linear;
    // Gerber defaults to single-quadrant interpolation.  Requiring an
    // explicit G75 for signed multi-quadrant offsets keeps old input from
    // being silently interpreted with different geometry.
    let mut quadrant = QuadrantMode::Single;
    let mut coordinate_mode = input_coordinate_mode;
    let mut transform = LocalTransform::default();
    let mut region: Option<RegionState> = None;
    let mut pending_g54 = false;
    let mut legacy_unit: Option<Unit> = None;
    let mut defined_apertures = HashSet::new();
    let mut defined_macros = HashSet::new();
    let mut has_drawing = false;
    let mut sr_open = false;
    let mut identity_commands_seen = HashSet::new();
    let mut object_count = 0usize;
    let mut edge_count = 0usize;
    let aperture_shapes: HashMap<String, ApertureShape> = document
        .apertures
        .iter()
        .map(|aperture| (aperture.id.clone(), aperture.shape.clone()))
        .collect();

    for (command_index, command) in doc.commands().into_iter().enumerate() {
        if pending_g54
            && !matches!(
                command,
                Command::FunctionCode(FunctionCode::DCode(DCode::SelectAperture(_)))
            )
        {
            return Err(command_semantic(
                command_index,
                "G54 must be followed by a DCode",
            ));
        }
        match command {
            Command::ExtendedCode(ExtendedCode::CoordinateFormat(_))
            | Command::ExtendedCode(ExtendedCode::Unit(_)) => {}
            Command::ExtendedCode(ExtendedCode::ApertureMacro(definition)) => {
                if !defined_macros.insert(definition.name.clone()) {
                    return Err(semantic(
                        command_index,
                        format!("duplicate aperture macro {}", definition.name),
                    ));
                }
            }
            Command::ExtendedCode(ExtendedCode::ApertureDefinition(definition)) => {
                let id = aperture_id(definition.code);
                if !defined_apertures.insert(id.clone()) {
                    return Err(command_semantic(
                        command_index,
                        format!("duplicate aperture D{}", definition.code),
                    ));
                }
                if let Aperture::Macro(name, _) = &definition.aperture
                    && !defined_macros.contains(name)
                {
                    return Err(semantic(
                        command_index,
                        format!(
                            "aperture D{} uses macro {} before AM",
                            definition.code, name
                        ),
                    ));
                }
            }
            Command::ExtendedCode(ExtendedCode::LoadPolarity(value)) => {
                if region.is_some() {
                    return Err(command_unsupported(command_index, "LP inside G36/G37"));
                }
                polarity = match value {
                    Polarity::Dark => Exposure::Dark,
                    Polarity::Clear => Exposure::Clear,
                };
            }
            Command::ExtendedCode(ExtendedCode::LoadMirroring(value)) => {
                transform.mirror = match value {
                    gerber_parser::gerber_types::Mirroring::None => Mirror::None,
                    gerber_parser::gerber_types::Mirroring::X => Mirror::X,
                    gerber_parser::gerber_types::Mirroring::Y => Mirror::Y,
                    gerber_parser::gerber_types::Mirroring::XY => Mirror::Xy,
                };
            }
            Command::ExtendedCode(ExtendedCode::LoadRotation(value)) => {
                transform.rotation_deg = checked_angle(value.rotation)?;
            }
            Command::ExtendedCode(ExtendedCode::LoadScaling(value)) => {
                if !value.scale.is_finite() || value.scale <= 0.0 {
                    return Err(command_semantic(
                        command_index,
                        "LS scale must be finite and positive",
                    ));
                }
                transform.scale = value.scale;
            }
            Command::ExtendedCode(ExtendedCode::StepAndRepeat(value)) => match value {
                gerber_parser::gerber_types::StepAndRepeat::Open {
                    repeat_x,
                    repeat_y,
                    distance_x,
                    distance_y,
                } if *repeat_x == 1
                    && *repeat_y == 1
                    && *distance_x == 0.0
                    && *distance_y == 0.0 =>
                {
                    if sr_open {
                        return Err(command_semantic(command_index, "nested SR open"));
                    }
                    sr_open = true;
                }
                gerber_parser::gerber_types::StepAndRepeat::Close => {
                    if !sr_open {
                        return Err(command_semantic(command_index, "SR close without SR open"));
                    }
                    sr_open = false;
                }
                _ => return Err(command_unsupported(command_index, "complex SR")),
            },
            Command::ExtendedCode(ExtendedCode::MirrorImage(value)) => {
                if has_drawing || identity_commands_seen.contains("MI") {
                    return Err(command_semantic(
                        command_index,
                        "MI must precede image data and occur once",
                    ));
                }
                identity_commands_seen.insert("MI");
                if !matches!(value, gerber_parser::gerber_types::ImageMirroring::None) {
                    return Err(command_unsupported(command_index, "non-identity MI"));
                }
            }
            Command::ExtendedCode(ExtendedCode::OffsetImage(value)) => {
                if has_drawing || identity_commands_seen.contains("OF") {
                    return Err(command_semantic(
                        command_index,
                        "OF must precede image data and occur once",
                    ));
                }
                identity_commands_seen.insert("OF");
                if !value.a.is_finite() || !value.b.is_finite() {
                    return Err(command_semantic(
                        command_index,
                        "IO/OF image offset is invalid or repeated",
                    ));
                }
                if value.a != 0.0 || value.b != 0.0 {
                    return Err(command_unsupported(command_index, "non-identity OF"));
                }
            }
            Command::ExtendedCode(ExtendedCode::ScaleImage(value)) => {
                if has_drawing || identity_commands_seen.contains("SF") {
                    return Err(command_semantic(
                        command_index,
                        "SF must precede image data and occur once",
                    ));
                }
                identity_commands_seen.insert("SF");
                if !value.a.is_finite() || !value.b.is_finite() || value.a != 1.0 || value.b != 1.0
                {
                    return Err(command_unsupported(command_index, "non-identity SF"));
                }
            }
            Command::ExtendedCode(ExtendedCode::RotateImage(value)) => {
                if has_drawing || identity_commands_seen.contains("IR") {
                    return Err(command_semantic(
                        command_index,
                        "IR must precede image data and occur once",
                    ));
                }
                identity_commands_seen.insert("IR");
                if !matches!(value, gerber_parser::gerber_types::ImageRotation::None) {
                    return Err(command_unsupported(command_index, "non-identity IR"));
                }
            }
            Command::ExtendedCode(ExtendedCode::ImagePolarity(value)) => {
                if !matches!(value, gerber_parser::gerber_types::ImagePolarity::Positive) {
                    return Err(command_unsupported(
                        command_index,
                        "negative image polarity",
                    ));
                }
            }
            Command::ExtendedCode(ExtendedCode::ImageName(value)) => {
                document.source.image_name = Some(value.name.clone());
            }
            Command::ExtendedCode(ExtendedCode::AxisSelect(value)) => {
                return Err(command_unsupported(
                    command_index,
                    format!("axis select {value:?}"),
                ));
            }
            Command::ExtendedCode(
                ExtendedCode::FileAttribute(_)
                | ExtendedCode::ObjectAttribute(_)
                | ExtendedCode::ApertureAttribute(_)
                | ExtendedCode::DeleteAttribute(_),
            ) => {}
            Command::ExtendedCode(ExtendedCode::ApertureBlock(_)) => {
                return Err(command_unsupported(command_index, "AB aperture block"));
            }
            Command::FunctionCode(FunctionCode::GCode(GCode::Unit(value))) => {
                if has_drawing || legacy_unit.replace(*value).is_some() || *value != declared_unit {
                    return Err(command_semantic(command_index, "G70/G71 conflicts with MO"));
                }
            }
            Command::FunctionCode(FunctionCode::GCode(GCode::CoordinateMode(value))) => {
                coordinate_mode = *value;
            }
            Command::FunctionCode(FunctionCode::GCode(GCode::InterpolationMode(value))) => {
                interpolation = *value;
            }
            Command::FunctionCode(FunctionCode::GCode(GCode::QuadrantMode(value))) => {
                quadrant = *value;
            }
            Command::FunctionCode(FunctionCode::GCode(GCode::RegionMode(open))) => {
                if *open {
                    if region.is_some() {
                        return Err(command_semantic(command_index, "nested G36 region"));
                    }
                    region = Some(RegionState {
                        source_command: command_index,
                        ..RegionState::default()
                    });
                } else {
                    let state = region
                        .take()
                        .ok_or_else(|| command_semantic(command_index, "G37 without G36"))?;
                    let mut contours = state.contours;
                    if !state.edges.is_empty() {
                        contours.push(close_contour(state.edges, state.start)?);
                    }
                    if contours.is_empty() {
                        return Err(command_semantic(command_index, "empty region"));
                    }
                    push_object(
                        document,
                        SemanticObject {
                            object_id: format!("object-{}", object_count + 1),
                            geometry: SemanticGeometry::Region { contours },
                            exposure: polarity,
                            source_command: state.source_command,
                        },
                        &mut object_count,
                        budget.max_objects,
                    )?;
                }
            }
            Command::FunctionCode(FunctionCode::GCode(GCode::SelectAperture)) => {
                pending_g54 = true;
            }
            Command::FunctionCode(FunctionCode::GCode(GCode::Comment(_))) => {}
            Command::FunctionCode(FunctionCode::DCode(DCode::SelectAperture(code))) => {
                let id = aperture_id(*code);
                if !defined_apertures.contains(&id) || !aperture_shapes.contains_key(&id) {
                    return Err(command_semantic(
                        command_index,
                        format!("undefined aperture D{code}"),
                    ));
                }
                active_aperture = Some(id);
                pending_g54 = false;
            }
            Command::FunctionCode(FunctionCode::DCode(DCode::Operation(operation))) => {
                match operation {
                    Operation::Move(coords) => {
                        has_drawing = true;
                        let next = point_from_coords(
                            coords.as_ref(),
                            current,
                            coordinate_mode,
                            unit_scale,
                            io_offset,
                        )?;
                        if let Some(state) = region.as_mut() {
                            if !state.edges.is_empty() {
                                state.contours.push(close_contour(
                                    std::mem::take(&mut state.edges),
                                    state.start.take(),
                                )?);
                            }
                            state.start = Some(next);
                            state.current = Some(next);
                        }
                        current = Some(next);
                    }
                    Operation::Flash(coords) => {
                        has_drawing = true;
                        if region.is_some() {
                            return Err(command_semantic(command_index, "D03 inside G36/G37"));
                        }
                        let aperture_id = active_aperture.clone().ok_or_else(|| {
                            command_semantic(command_index, "D03 before aperture selection")
                        })?;
                        let next = point_from_coords(
                            coords.as_ref(),
                            current,
                            coordinate_mode,
                            unit_scale,
                            io_offset,
                        )?;
                        push_object(
                            document,
                            SemanticObject {
                                object_id: format!("object-{}", object_count + 1),
                                geometry: SemanticGeometry::Flash {
                                    center: next,
                                    aperture_id,
                                    transform,
                                },
                                exposure: polarity,
                                source_command: command_index,
                            },
                            &mut object_count,
                            budget.max_objects,
                        )?;
                        current = Some(next);
                    }
                    Operation::Interpolate(coords, offset) => {
                        has_drawing = true;
                        let start = current.ok_or_else(|| {
                            command_semantic(command_index, "D01 before a current point")
                        })?;
                        let end = point_from_coords(
                            coords.as_ref(),
                            Some(start),
                            coordinate_mode,
                            unit_scale,
                            io_offset,
                        )?;
                        if let Some(state) = region.as_mut() {
                            let edge = interpolation_edge(
                                start,
                                end,
                                offset.as_ref(),
                                interpolation,
                                quadrant,
                                unit_scale,
                                source_resolution,
                                command_index,
                            )?;
                            state.edges.push(edge);
                            edge_count = edge_count.saturating_add(1);
                            check_resource("region_edges", budget.max_region_edges, edge_count)?;
                            state.current = Some(end);
                            current = Some(end);
                            continue;
                        }
                        let aperture_id = active_aperture.clone().ok_or_else(|| {
                            command_semantic(command_index, "D01 before aperture selection")
                        })?;
                        let shape = aperture_shapes.get(&aperture_id).ok_or_else(|| {
                            command_semantic(command_index, "D01 aperture disappeared")
                        })?;
                        let geometry = interpolation_geometry(
                            start,
                            end,
                            offset.as_ref(),
                            interpolation,
                            quadrant,
                            unit_scale,
                            source_resolution,
                            transform,
                            shape,
                            command_index,
                        )?;
                        push_object(
                            document,
                            SemanticObject {
                                object_id: format!("object-{}", object_count + 1),
                                geometry,
                                exposure: polarity,
                                source_command: command_index,
                            },
                            &mut object_count,
                            budget.max_objects,
                        )?;
                        current = Some(end);
                    }
                }
            }
            Command::FunctionCode(FunctionCode::MCode(MCode::EndOfFile)) => {}
        }
    }
    if pending_g54 {
        return Err(command_semantic(
            doc.commands().len(),
            "G54 has no associated DCode",
        ));
    }
    if region.is_some() {
        return Err(command_semantic(
            doc.commands().len(),
            "G36 is not closed by G37",
        ));
    }
    if document.object_count() == 0 {
        return Err(command_semantic(
            doc.commands().len(),
            "document has no drawable objects",
        ));
    }
    Ok(())
}

fn push_object(
    document: &mut SemanticDocument,
    object: SemanticObject,
    count: &mut usize,
    limit: usize,
) -> Result<(), S1Error> {
    *count += 1;
    check_resource("semantic_objects", limit, *count)?;
    document.layers[0].objects.push(object);
    Ok(())
}

fn point_from_coords(
    coords: Option<&gerber_parser::gerber_types::Coordinates>,
    current: Option<MmPoint>,
    mode: CoordinateMode,
    unit_scale: f64,
    io_offset: MmPoint,
) -> Result<MmPoint, S1Error> {
    let (x, y) = coords
        .map(|coords| (coords.x.map(f64::from), coords.y.map(f64::from)))
        .unwrap_or((None, None));
    match mode {
        CoordinateMode::Absolute => {
            if current.is_none() && (x.is_none() || y.is_none()) {
                return Err(S1Error::Semantic {
                    line: 0,
                    message: "first absolute coordinate must specify X and Y".into(),
                });
            }
            let old = current.unwrap_or(io_offset);
            let x = x.map_or(old.x_mm, |value| value * unit_scale + io_offset.x_mm);
            let y = y.map_or(old.y_mm, |value| value * unit_scale + io_offset.y_mm);
            finite_point(MmPoint::new(x, y))
        }
        CoordinateMode::Incremental => {
            let old = current.unwrap_or(io_offset);
            finite_point(MmPoint::new(
                old.x_mm + x.unwrap_or(0.0) * unit_scale,
                old.y_mm + y.unwrap_or(0.0) * unit_scale,
            ))
        }
    }
}

fn finite_point(point: MmPoint) -> Result<MmPoint, S1Error> {
    if point.is_finite() && point.x_mm.abs() <= 1e9 && point.y_mm.abs() <= 1e9 {
        Ok(point)
    } else {
        Err(S1Error::Semantic {
            line: 0,
            message: "coordinate is non-finite or out of range".into(),
        })
    }
}

fn checked_angle(value: f64) -> Result<f64, S1Error> {
    if !value.is_finite() || value.abs() > 1e9 {
        Err(S1Error::Semantic {
            line: 0,
            message: "angle is non-finite or out of range".into(),
        })
    } else {
        Ok(value)
    }
}

fn semantic(line: usize, message: impl Into<String>) -> S1Error {
    S1Error::Semantic {
        line,
        message: message.into(),
    }
}

fn command_semantic(command_index: usize, message: impl Into<String>) -> S1Error {
    S1Error::Semantic {
        line: 0,
        message: format!("command {command_index}: {}", message.into()),
    }
}

fn unsupported(line: usize, feature: impl Into<String>) -> S1Error {
    S1Error::Unsupported {
        line,
        feature: feature.into(),
    }
}

fn command_unsupported(command_index: usize, feature: impl Into<String>) -> S1Error {
    S1Error::Unsupported {
        line: 0,
        feature: format!("command {command_index}: {}", feature.into()),
    }
}

fn check_resource(resource: &'static str, limit: usize, actual: usize) -> Result<(), S1Error> {
    if actual > limit {
        Err(S1Error::ResourceLimit {
            resource,
            limit,
            actual,
        })
    } else {
        Ok(())
    }
}

fn check_writer_output(output: &str, budget: S1Budget) -> Result<(), S1Error> {
    check_resource("writer_bytes", budget.max_writer_bytes, output.len())
}

fn close_contour(edges: Vec<RegionEdge>, start: Option<MmPoint>) -> Result<RegionContour, S1Error> {
    let start = start.ok_or_else(|| semantic(0, "region contour has no start"))?;
    let end = edges
        .last()
        .map(edge_end)
        .ok_or_else(|| semantic(0, "empty region contour"))?;
    // Gerber regions are closed by the quantized endpoint itself.  Accepting a
    // geometric tolerance here would turn a near miss into manufacturing data.
    if end != start {
        return Err(semantic(0, "region contour is not closed"));
    }
    if edges.is_empty()
        || (edges.len() == 1
            && !matches!(edges.first(), Some(RegionEdge::Arc(arc)) if arc.full_circle))
    {
        return Err(semantic(0, "region contour has too few edges"));
    }
    Ok(RegionContour {
        edges,
        role: RegionRole::Solid,
    })
}

fn edge_start(edge: &RegionEdge) -> MmPoint {
    match edge {
        RegionEdge::Line { start, .. } | RegionEdge::Arc(ArcGeometry { start, .. }) => *start,
    }
}

fn edge_end(edge: &RegionEdge) -> MmPoint {
    match edge {
        RegionEdge::Line { end, .. } | RegionEdge::Arc(ArcGeometry { end, .. }) => *end,
    }
}

#[allow(clippy::too_many_arguments)]
fn interpolation_edge(
    start: MmPoint,
    end: MmPoint,
    offset: Option<&gerber_parser::gerber_types::CoordinateOffset>,
    interpolation: InterpolationMode,
    quadrant: QuadrantMode,
    unit_scale: f64,
    source_resolution: f64,
    command_index: usize,
) -> Result<RegionEdge, S1Error> {
    match interpolation {
        InterpolationMode::Linear => Ok(RegionEdge::Line { start, end }),
        InterpolationMode::ClockwiseCircular | InterpolationMode::CounterclockwiseCircular => {
            Ok(RegionEdge::Arc(arc_from_command(
                start,
                end,
                offset,
                if matches!(interpolation, InterpolationMode::ClockwiseCircular) {
                    ArcDirection::Clockwise
                } else {
                    ArcDirection::CounterClockwise
                },
                quadrant,
                unit_scale,
                source_resolution,
                command_index,
            )?))
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn interpolation_geometry(
    start: MmPoint,
    end: MmPoint,
    offset: Option<&gerber_parser::gerber_types::CoordinateOffset>,
    interpolation: InterpolationMode,
    quadrant: QuadrantMode,
    unit_scale: f64,
    source_resolution: f64,
    transform: LocalTransform,
    aperture: &ApertureShape,
    command_index: usize,
) -> Result<SemanticGeometry, S1Error> {
    if !matches!(
        transform,
        LocalTransform {
            mirror: Mirror::None,
            rotation_deg: 0.0,
            scale: 1.0
        }
    ) {
        return Err(command_unsupported(
            command_index,
            "transformed non-flash interpolation",
        ));
    }
    match interpolation {
        InterpolationMode::Linear => match aperture {
            ApertureShape::Circle {
                diameter_mm,
                hole_diameter_mm: None,
            } => Ok(SemanticGeometry::Line {
                start,
                end,
                width_mm: *diameter_mm,
            }),
            ApertureShape::Rectangle {
                width_mm,
                height_mm,
                hole_diameter_mm: None,
            } if start.x_mm == end.x_mm || start.y_mm == end.y_mm => {
                Ok(SemanticGeometry::RectangularSweep {
                    start,
                    end,
                    width_mm: *width_mm,
                    height_mm: *height_mm,
                })
            }
            _ => Err(unsupported(
                command_index,
                "linear interpolation requires an unholed circle or axis-aligned R aperture",
            )),
        },
        InterpolationMode::ClockwiseCircular | InterpolationMode::CounterclockwiseCircular => {
            let width_mm = match aperture {
                ApertureShape::Circle {
                    diameter_mm,
                    hole_diameter_mm: None,
                } => *diameter_mm,
                _ => {
                    return Err(unsupported(
                        command_index,
                        "circular interpolation requires an unholed circle aperture",
                    ));
                }
            };
            Ok(SemanticGeometry::Arc {
                path: arc_from_command(
                    start,
                    end,
                    offset,
                    if matches!(interpolation, InterpolationMode::ClockwiseCircular) {
                        ArcDirection::Clockwise
                    } else {
                        ArcDirection::CounterClockwise
                    },
                    quadrant,
                    unit_scale,
                    source_resolution,
                    command_index,
                )?,
                width_mm,
            })
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn arc_from_command(
    start: MmPoint,
    end: MmPoint,
    offset: Option<&gerber_parser::gerber_types::CoordinateOffset>,
    direction: ArcDirection,
    quadrant: QuadrantMode,
    unit_scale: f64,
    source_resolution: f64,
    command_index: usize,
) -> Result<ArcGeometry, S1Error> {
    let offset = offset
        .ok_or_else(|| command_semantic(command_index, "circular interpolation needs I or J"))?;
    let i = offset.x.map(f64::from).unwrap_or(0.0) * unit_scale;
    let j = offset.y.map(f64::from).unwrap_or(0.0) * unit_scale;
    if !i.is_finite() || !j.is_finite() || i.abs() > 1e9 || j.abs() > 1e9 {
        return Err(command_semantic(command_index, "arc offset is invalid"));
    }
    if i == 0.0 && j == 0.0 && !(matches!(quadrant, QuadrantMode::Single) && start == end) {
        return Err(command_semantic(command_index, "arc offset cannot be zero"));
    }
    if matches!(quadrant, QuadrantMode::Single) && (i < 0.0 || j < 0.0) {
        return Err(command_semantic(command_index, "G74 I/J must be unsigned"));
    }
    if matches!(quadrant, QuadrantMode::Multi) {
        let path = ArcGeometry {
            start,
            end,
            center: MmPoint::new(start.x_mm + i, start.y_mm + j),
            direction,
            full_circle: start == end,
            source: Some(ArcSource {
                resolution_mm: source_resolution,
                single_quadrant: matches!(quadrant, QuadrantMode::Single),
            }),
        };
        if !path.is_valid() {
            return Err(command_semantic(
                command_index,
                "G75 arc has a nonsensical center, zero radius or invalid sweep",
            ));
        }
        return Ok(path);
    }

    // A G74 start=end operation sweeps only a dot. Center signs do not affect
    // its coverage; retain a deterministic declared candidate for provenance.
    if start == end {
        return Ok(ArcGeometry {
            start,
            end,
            center: MmPoint::new(start.x_mm + i, start.y_mm + j),
            direction,
            full_circle: false,
            source: Some(ArcSource {
                resolution_mm: source_resolution,
                single_quadrant: true,
            }),
        });
    }
    // Ucamco 2026.05 §8.2.4: direction, <=90 degrees, then least deviation.
    let mut candidates = Vec::new();
    for sign_x in [-1.0, 1.0] {
        for sign_y in [-1.0, 1.0] {
            let center = MmPoint::new(start.x_mm + sign_x * i, start.y_mm + sign_y * j);
            let path = ArcGeometry {
                start,
                end,
                center,
                direction,
                full_circle: false,
                source: Some(ArcSource {
                    resolution_mm: source_resolution,
                    single_quadrant: matches!(quadrant, QuadrantMode::Single),
                }),
            };
            if path.is_valid()
                && path.sweep_radians().is_some_and(|sweep| {
                    sweep > 0.0 && sweep <= std::f64::consts::FRAC_PI_2 + path.angular_uncertainty()
                })
                && !candidates
                    .iter()
                    .any(|candidate: &ArcGeometry| candidate.center == center)
            {
                candidates.push(path);
            }
        }
    }
    candidates.sort_by(|a, b| a.arc_deviation().total_cmp(&b.arc_deviation()));
    let Some(best) = candidates.first().copied() else {
        return Err(command_semantic(
            command_index,
            "G74 has no valid single-quadrant center",
        ));
    };
    if let Some(next) = candidates.get(1) {
        let uncertainty = best.numeric_tolerance().max(next.numeric_tolerance());
        if (next.arc_deviation() - best.arc_deviation()).abs() <= uncertainty {
            return Err(command_semantic(
                command_index,
                "G74 least-deviation center is indeterminate",
            ));
        }
    }
    Ok(best)
}

/// Serialize only a validated semantic snapshot.  The result is reparsed by
/// the same product parser before it is returned to a caller.
pub fn write_s1(document: &SemanticDocument) -> Result<Vec<u8>, S1Error> {
    write_s1_with_budget(document, S1Budget::default())
}

pub fn write_s1_with_budget(
    document: &SemanticDocument,
    budget: S1Budget,
) -> Result<Vec<u8>, S1Error> {
    let validation = document.validate().map_err(core_error)?;
    if validation.region_edge_count > budget.max_region_edges {
        return Err(S1Error::ResourceLimit {
            resource: "region_edges",
            limit: budget.max_region_edges,
            actual: validation.region_edge_count,
        });
    }
    if validation.object_count > budget.max_objects {
        return Err(S1Error::ResourceLimit {
            resource: "semantic_objects",
            limit: budget.max_objects,
            actual: validation.object_count,
        });
    }
    let macro_work_count = document
        .apertures
        .iter()
        .map(|aperture| match &aperture.shape {
            ApertureShape::Macro { primitives } => {
                primitives.iter().fold(0usize, |total, primitive| {
                    total.saturating_add(1).saturating_add(match primitive {
                        MacroPrimitive::Outline { points, .. } => points.len(),
                        MacroPrimitive::Circle { .. } | MacroPrimitive::CenterLine { .. } => 0,
                    })
                })
            }
            _ => 0,
        })
        .sum::<usize>();
    if macro_work_count > budget.max_am_expansions {
        return Err(S1Error::ResourceLimit {
            resource: "am_expansions",
            limit: budget.max_am_expansions,
            actual: macro_work_count,
        });
    }
    if document.layers.len() != 1 {
        return Err(unsupported(0, "S1 writer exports one layer at a time"));
    }
    let mut out = String::from("G04 RCam normalized S1 output*\n%FSLAX66Y66*%\n%MOMM*%\n");
    check_writer_output(&out, budget)?;
    let mut next_dcode = 10_i32;
    let mut codes = HashMap::new();
    for aperture in &document.apertures {
        codes.insert(aperture.id.clone(), next_dcode);
        emit_aperture_definition(&mut out, next_dcode, &aperture.shape, budget)?;
        check_writer_output(&out, budget)?;
        next_dcode += 1;
    }

    // Dynamic stroke apertures are collected and declared before the first
    // image command.  Reusing equal shapes keeps large documents bounded and
    // avoids a late AD command changing the source-order contract.
    let mut dynamic_codes = HashMap::new();
    let mut dynamic_shapes: Vec<(i32, ApertureShape)> = Vec::new();
    for (object_index, object) in document.layers[0].objects.iter().enumerate() {
        let shape = match object.geometry {
            SemanticGeometry::Line { width_mm, .. } | SemanticGeometry::Arc { width_mm, .. } => {
                Some(ApertureShape::Circle {
                    diameter_mm: width_mm,
                    hole_diameter_mm: None,
                })
            }
            SemanticGeometry::RectangularSweep {
                width_mm,
                height_mm,
                ..
            } => Some(ApertureShape::Rectangle {
                width_mm,
                height_mm,
                hole_diameter_mm: None,
            }),
            SemanticGeometry::Flash { .. } | SemanticGeometry::Region { .. } => None,
        };
        if let Some(shape) = shape {
            let code = dynamic_shapes
                .iter()
                .find(|(_, existing)| *existing == shape)
                .map(|(code, _)| *code)
                .unwrap_or_else(|| {
                    let code = next_dcode + dynamic_shapes.len() as i32;
                    dynamic_shapes.push((code, shape));
                    code
                });
            dynamic_codes.insert(object_index, code);
        }
    }
    for (code, shape) in &dynamic_shapes {
        emit_aperture_definition(&mut out, *code, shape, budget)?;
        check_writer_output(&out, budget)?;
    }
    let mut polarity = Exposure::Dark;
    for layer in &document.layers {
        for (object_index, object) in layer.objects.iter().enumerate() {
            set_polarity(&mut out, &mut polarity, object.exposure);
            match &object.geometry {
                SemanticGeometry::Flash {
                    center,
                    aperture_id,
                    transform,
                } => {
                    let code = *codes.get(aperture_id).ok_or_else(|| S1Error::Semantic {
                        line: 0,
                        message: format!("missing aperture {aperture_id}"),
                    })?;
                    out.push_str(&format!("D{code}*\n"));
                    emit_transform(
                        &mut out,
                        transform.mirror,
                        transform.rotation_deg,
                        transform.scale,
                    );
                    emit_flash(&mut out, *center)?;
                    emit_transform(&mut out, Mirror::None, 0.0, 1.0);
                }
                SemanticGeometry::Line {
                    start,
                    end,
                    width_mm,
                } => {
                    let _ = width_mm;
                    let code = *dynamic_codes
                        .get(&object_index)
                        .ok_or_else(|| semantic(0, "missing dynamic line aperture"))?;
                    out.push_str(&format!("D{code}*\n"));
                    emit_move(&mut out, *start)?;
                    emit_line(&mut out, *end)?;
                }
                SemanticGeometry::RectangularSweep {
                    start,
                    end,
                    width_mm,
                    height_mm,
                } => {
                    let _ = (width_mm, height_mm);
                    let code = *dynamic_codes
                        .get(&object_index)
                        .ok_or_else(|| semantic(0, "missing dynamic rectangle aperture"))?;
                    out.push_str(&format!("D{code}*\n"));
                    emit_move(&mut out, *start)?;
                    emit_line(&mut out, *end)?;
                }
                SemanticGeometry::Arc { path, width_mm } => {
                    let _ = width_mm;
                    let code = *dynamic_codes
                        .get(&object_index)
                        .ok_or_else(|| semantic(0, "missing dynamic arc aperture"))?;
                    out.push_str(&format!("D{code}*\n"));
                    emit_move(&mut out, path.start)?;
                    emit_arc(&mut out, *path)?;
                }
                SemanticGeometry::Region { contours } => {
                    if contours.iter().any(|contour| {
                        contour
                            .edges
                            .iter()
                            .any(|edge| matches!(edge, RegionEdge::Arc(_)))
                    }) {
                        out.push_str("G75*\n");
                    }
                    out.push_str("G36*\n");
                    for contour in contours {
                        if !matches!(contour.role, RegionRole::Solid) {
                            return Err(unsupported(
                                0,
                                "writer cannot encode unverified region hole",
                            ));
                        }
                        for (index, edge) in contour.edges.iter().enumerate() {
                            if index == 0 {
                                emit_move(&mut out, edge_start(edge))?;
                            }
                            match edge {
                                RegionEdge::Line { end, .. } => emit_line(&mut out, *end)?,
                                RegionEdge::Arc(path) => emit_arc(&mut out, *path)?,
                            }
                        }
                    }
                    out.push_str("G37*\n");
                }
            }
            check_writer_output(&out, budget)?;
        }
    }
    set_polarity(&mut out, &mut polarity, Exposure::Dark);
    out.push_str("M02*\n");
    if out.len() > budget.max_writer_bytes {
        return Err(S1Error::ResourceLimit {
            resource: "writer_bytes",
            limit: budget.max_writer_bytes,
            actual: out.len(),
        });
    }
    let bytes = out.into_bytes();
    verify_roundtrip_with_budget(document, &bytes, budget)?;
    Ok(bytes)
}

/// Reparse an output using the larger validation budget and compare the
/// resulting manufacturing semantics with the snapshot that was written.
/// This is public so the service and headless callers use the same gate.
pub fn verify_roundtrip(
    expected: &SemanticDocument,
    bytes: &[u8],
) -> Result<ValidationReport, S1Error> {
    verify_roundtrip_with_budget(expected, bytes, S1Budget::default())
}

pub fn verify_roundtrip_with_budget(
    expected: &SemanticDocument,
    bytes: &[u8],
    budget: S1Budget,
) -> Result<ValidationReport, S1Error> {
    expected.validate().map_err(core_error)?;
    if bytes.len() > budget.max_validation_bytes {
        return Err(S1Error::ResourceLimit {
            resource: "validation_bytes",
            limit: budget.max_validation_bytes,
            actual: bytes.len(),
        });
    }
    let mut validation_budget = budget;
    validation_budget.max_source_bytes = budget.max_validation_bytes;
    let actual = parse_s1_with_budget(bytes, &expected.id, validation_budget)?;
    let validation = actual.document.validate().map_err(core_error)?;
    if !documents_semantically_equal(expected, &actual.document) {
        return Err(semantic(0, "writer round-trip changed semantic geometry"));
    }
    Ok(validation)
}

fn documents_semantically_equal(
    expected_document: &SemanticDocument,
    actual_document: &SemanticDocument,
) -> bool {
    if expected_document.layers.len() != actual_document.layers.len() {
        return false;
    }
    let tolerance = S1_ROUNDTRIP_TOLERANCE_MM;
    expected_document
        .layers
        .iter()
        .zip(&actual_document.layers)
        .all(|(a, b)| {
            a.objects.len() == b.objects.len()
                && a.objects.iter().zip(&b.objects).all(|(expected, actual)| {
                    expected.exposure == actual.exposure
                        && geometry_semantically_equal(
                            expected_document,
                            actual_document,
                            &expected.geometry,
                            &actual.geometry,
                            tolerance,
                        )
                })
        })
}

fn geometry_semantically_equal(
    left_document: &SemanticDocument,
    right_document: &SemanticDocument,
    left: &SemanticGeometry,
    right: &SemanticGeometry,
    tolerance: f64,
) -> bool {
    match (left, right) {
        (
            SemanticGeometry::Arc {
                path,
                width_mm: arc_width,
            },
            SemanticGeometry::Line {
                start,
                end,
                width_mm: line_width,
            },
        )
        | (
            SemanticGeometry::Line {
                start,
                end,
                width_mm: line_width,
            },
            SemanticGeometry::Arc {
                path,
                width_mm: arc_width,
            },
        ) => {
            path.zero_sweep()
                && start == end
                && points_close(path.start, *start, tolerance)
                && close(*arc_width, *line_width, tolerance)
        }

        (
            SemanticGeometry::Flash {
                center: left_center,
                aperture_id: left_aperture,
                transform: left_transform,
            },
            SemanticGeometry::Flash {
                center: right_center,
                aperture_id: right_aperture,
                transform: right_transform,
            },
        ) => {
            points_close(*left_center, *right_center, tolerance)
                && aperture_shapes_equal(
                    aperture_shape(left_document, left_aperture),
                    aperture_shape(right_document, right_aperture),
                    tolerance,
                )
                && left_transform.mirror == right_transform.mirror
                && close(
                    left_transform.rotation_deg,
                    right_transform.rotation_deg,
                    tolerance,
                )
                && close(left_transform.scale, right_transform.scale, tolerance)
        }
        (
            SemanticGeometry::Line {
                start: left_start,
                end: left_end,
                width_mm: left_width,
            },
            SemanticGeometry::Line {
                start: right_start,
                end: right_end,
                width_mm: right_width,
            },
        ) => {
            points_close(*left_start, *right_start, tolerance)
                && points_close(*left_end, *right_end, tolerance)
                && close(*left_width, *right_width, tolerance)
        }
        (
            SemanticGeometry::RectangularSweep {
                start: left_start,
                end: left_end,
                width_mm: left_width,
                height_mm: left_height,
            },
            SemanticGeometry::RectangularSweep {
                start: right_start,
                end: right_end,
                width_mm: right_width,
                height_mm: right_height,
            },
        ) => {
            points_close(*left_start, *right_start, tolerance)
                && points_close(*left_end, *right_end, tolerance)
                && close(*left_width, *right_width, tolerance)
                && close(*left_height, *right_height, tolerance)
        }
        (
            SemanticGeometry::Arc {
                path: left_path,
                width_mm: left_width,
            },
            SemanticGeometry::Arc {
                path: right_path,
                width_mm: right_width,
            },
        ) => {
            arc_semantically_equal(left_path, right_path, tolerance)
                && close(*left_width, *right_width, tolerance)
        }
        (
            SemanticGeometry::Region { contours: left },
            SemanticGeometry::Region { contours: right },
        ) => {
            left.len() == right.len()
                && left.iter().zip(right).all(|(left, right)| {
                    left.role == right.role
                        && left.edges.len() == right.edges.len()
                        && left
                            .edges
                            .iter()
                            .zip(&right.edges)
                            .all(|(left, right)| region_edges_equal(left, right, tolerance))
                })
        }
        _ => false,
    }
}

fn aperture_shape<'a>(document: &'a SemanticDocument, id: &str) -> &'a ApertureShape {
    document
        .apertures
        .iter()
        .find(|aperture| aperture.id == id)
        .map(|aperture| &aperture.shape)
        .expect("validated semantic document must contain every flash aperture")
}

fn aperture_shapes_equal(left: &ApertureShape, right: &ApertureShape, tolerance: f64) -> bool {
    match (left, right) {
        (
            ApertureShape::Circle {
                diameter_mm: left_diameter,
                hole_diameter_mm: left_hole,
            },
            ApertureShape::Circle {
                diameter_mm: right_diameter,
                hole_diameter_mm: right_hole,
            },
        ) => {
            close(*left_diameter, *right_diameter, tolerance)
                && optional_close(*left_hole, *right_hole, tolerance)
        }
        (
            ApertureShape::Rectangle {
                width_mm: left_width,
                height_mm: left_height,
                hole_diameter_mm: left_hole,
            },
            ApertureShape::Rectangle {
                width_mm: right_width,
                height_mm: right_height,
                hole_diameter_mm: right_hole,
            },
        )
        | (
            ApertureShape::Obround {
                width_mm: left_width,
                height_mm: left_height,
                hole_diameter_mm: left_hole,
            },
            ApertureShape::Obround {
                width_mm: right_width,
                height_mm: right_height,
                hole_diameter_mm: right_hole,
            },
        ) => {
            close(*left_width, *right_width, tolerance)
                && close(*left_height, *right_height, tolerance)
                && optional_close(*left_hole, *right_hole, tolerance)
        }
        (
            ApertureShape::Polygon {
                diameter_mm: left_diameter,
                vertices: left_vertices,
                rotation_deg: left_rotation,
                hole_diameter_mm: left_hole,
            },
            ApertureShape::Polygon {
                diameter_mm: right_diameter,
                vertices: right_vertices,
                rotation_deg: right_rotation,
                hole_diameter_mm: right_hole,
            },
        ) => {
            left_vertices == right_vertices
                && close(*left_diameter, *right_diameter, tolerance)
                && close(*left_rotation, *right_rotation, tolerance)
                && optional_close(*left_hole, *right_hole, tolerance)
        }
        (ApertureShape::Macro { primitives: left }, ApertureShape::Macro { primitives: right }) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(left, right)| match (left, right) {
                        (
                            MacroPrimitive::Circle {
                                exposure: left_exposure,
                                diameter_mm: left_diameter,
                                center: left_center,
                                rotation_deg: left_rotation,
                            },
                            MacroPrimitive::Circle {
                                exposure: right_exposure,
                                diameter_mm: right_diameter,
                                center: right_center,
                                rotation_deg: right_rotation,
                            },
                        ) => {
                            left_exposure == right_exposure
                                && close(*left_diameter, *right_diameter, tolerance)
                                && points_close(*left_center, *right_center, tolerance)
                                && close(*left_rotation, *right_rotation, tolerance)
                        }
                        (
                            MacroPrimitive::CenterLine {
                                exposure: left_exposure,
                                width_mm: left_width,
                                height_mm: left_height,
                                center: left_center,
                                rotation_deg: left_rotation,
                            },
                            MacroPrimitive::CenterLine {
                                exposure: right_exposure,
                                width_mm: right_width,
                                height_mm: right_height,
                                center: right_center,
                                rotation_deg: right_rotation,
                            },
                        ) => {
                            left_exposure == right_exposure
                                && close(*left_width, *right_width, tolerance)
                                && close(*left_height, *right_height, tolerance)
                                && points_close(*left_center, *right_center, tolerance)
                                && close(*left_rotation, *right_rotation, tolerance)
                        }
                        (
                            MacroPrimitive::Outline {
                                exposure: left_exposure,
                                points: left_points,
                                rotation_deg: left_rotation,
                            },
                            MacroPrimitive::Outline {
                                exposure: right_exposure,
                                points: right_points,
                                rotation_deg: right_rotation,
                            },
                        ) => {
                            left_exposure == right_exposure
                                && left_points.len() == right_points.len()
                                && left_points
                                    .iter()
                                    .zip(right_points)
                                    .all(|(left, right)| points_close(*left, *right, tolerance))
                                && close(*left_rotation, *right_rotation, tolerance)
                        }
                        _ => false,
                    })
        }
        _ => false,
    }
}

fn arc_semantically_equal(left: &ArcGeometry, right: &ArcGeometry, tolerance: f64) -> bool {
    points_close(left.start, right.start, tolerance)
        && points_close(left.end, right.end, tolerance)
        && ((left.zero_sweep() && right.zero_sweep())
            || points_close(left.center, right.center, tolerance))
        && (left.zero_sweep()
            || ((left.arc_deviation() - right.arc_deviation()).abs() <= 6.0 * tolerance
                && (left.radius() - right.radius()).abs() <= 3.0 * tolerance
                && (left.end_radius() - right.end_radius()).abs() <= 3.0 * tolerance))
        && left.direction == right.direction
        && left.full_circle == right.full_circle
}

fn region_edges_equal(left: &RegionEdge, right: &RegionEdge, tolerance: f64) -> bool {
    match (left, right) {
        (
            RegionEdge::Line {
                start: left_start,
                end: left_end,
            },
            RegionEdge::Line {
                start: right_start,
                end: right_end,
            },
        ) => {
            points_close(*left_start, *right_start, tolerance)
                && points_close(*left_end, *right_end, tolerance)
        }
        (RegionEdge::Arc(left), RegionEdge::Arc(right)) => {
            arc_semantically_equal(left, right, tolerance)
        }
        _ => false,
    }
}

fn points_close(left: MmPoint, right: MmPoint, tolerance: f64) -> bool {
    close(left.x_mm, right.x_mm, tolerance) && close(left.y_mm, right.y_mm, tolerance)
}

fn optional_close(left: Option<f64>, right: Option<f64>, tolerance: f64) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => close(left, right, tolerance),
        (None, None) => true,
        _ => false,
    }
}

fn close(left: f64, right: f64, tolerance: f64) -> bool {
    (left - right).abs() <= tolerance
}

pub fn export_s1_new_path(
    document: &SemanticDocument,
    path: &Path,
) -> Result<ExportReport, S1Error> {
    let bytes = write_s1(document)?;
    if path.exists() {
        return Err(S1Error::TargetExists(path.to_path_buf()));
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("output.gbr");
    let temporary = parent.join(format!(".{file_name}.rcam-{}.tmp", std::process::id()));
    let mut temporary_owned = false;
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| io_error(&temporary, error))?;
        temporary_owned = true;
        std::io::Write::write_all(&mut file, &bytes)
            .map_err(|error| io_error(&temporary, error))?;
        file.sync_all()
            .map_err(|error| io_error(&temporary, error))?;
        let persisted = read_bounded_file(&temporary, S1_MAX_VALIDATION_BYTES)?;
        verify_roundtrip(document, &persisted)?;
        if path.exists() {
            return Err(S1Error::TargetExists(path.to_path_buf()));
        }
        match fs::hard_link(&temporary, path) {
            Ok(()) => {
                // The hard link is the no-clobber publish operation.  The
                // temporary name is ours, so cleanup failure does not turn a
                // successfully published file into an error.
                let _ = fs::remove_file(&temporary);
                Ok(())
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                Err(S1Error::TargetExists(path.to_path_buf()))
            }
            Err(error) => Err(io_error(path, error)),
        }
    })();
    if result.is_err() && temporary_owned {
        let _ = fs::remove_file(&temporary);
    }
    match result {
        Ok(()) => Ok(ExportReport {
            path: path.to_path_buf(),
            bytes_written: bytes.len(),
            validation: document.validate().map_err(core_error)?,
        }),
        Err(error) => Err(error),
    }
}

fn read_bounded_file(path: &Path, limit: usize) -> Result<Vec<u8>, S1Error> {
    let file = File::open(path).map_err(|error| io_error(path, error))?;
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| io_error(path, error))?;
    if bytes.len() > limit {
        return Err(S1Error::ResourceLimit {
            resource: "validation_bytes",
            limit,
            actual: bytes.len(),
        });
    }
    Ok(bytes)
}

fn core_error(error: SemanticError) -> S1Error {
    match error {
        SemanticError::ResourceLimit {
            resource,
            limit,
            actual,
        } => S1Error::ResourceLimit {
            resource,
            limit,
            actual,
        },
        other => S1Error::Semantic {
            line: 0,
            message: other.to_string(),
        },
    }
}

fn io_error(path: &Path, error: std::io::Error) -> S1Error {
    S1Error::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    }
}

fn emit_aperture_definition(
    output: &mut String,
    code: i32,
    shape: &ApertureShape,
    budget: S1Budget,
) -> Result<(), S1Error> {
    match shape {
        ApertureShape::Circle {
            diameter_mm,
            hole_diameter_mm,
        } => {
            output.push_str(&format!(
                "%ADD{code}C,{}{}*%\n",
                number(*diameter_mm),
                hole_diameter(*hole_diameter_mm)
            ));
            check_writer_output(output, budget)?;
        }
        ApertureShape::Rectangle {
            width_mm,
            height_mm,
            hole_diameter_mm,
        } => {
            output.push_str(&format!(
                "%ADD{code}R,{}X{}{}*%\n",
                number(*width_mm),
                number(*height_mm),
                hole_diameter(*hole_diameter_mm)
            ));
            check_writer_output(output, budget)?;
        }
        ApertureShape::Obround {
            width_mm,
            height_mm,
            hole_diameter_mm,
        } => {
            output.push_str(&format!(
                "%ADD{code}O,{}X{}{}*%\n",
                number(*width_mm),
                number(*height_mm),
                hole_diameter(*hole_diameter_mm)
            ));
            check_writer_output(output, budget)?;
        }
        ApertureShape::Polygon {
            diameter_mm,
            vertices,
            rotation_deg,
            hole_diameter_mm,
        } => {
            output.push_str(&format!(
                "%ADD{code}P,{}X{}X{}{}*%\n",
                number(*diameter_mm),
                vertices,
                number(*rotation_deg),
                hole_diameter(*hole_diameter_mm)
            ));
            check_writer_output(output, budget)?;
        }
        ApertureShape::Macro { primitives } => {
            let name = format!("RCAM{code}");
            output.push_str(&format!("%AM{name}*"));
            check_writer_output(output, budget)?;
            for primitive in primitives {
                match primitive {
                    MacroPrimitive::Circle {
                        exposure,
                        diameter_mm,
                        center,
                        rotation_deg,
                    } => {
                        output.push_str(&format!(
                            "1,{},{},{},{},{}*",
                            exposure_number(*exposure),
                            number(*diameter_mm),
                            number(center.x_mm),
                            number(center.y_mm),
                            number(*rotation_deg)
                        ));
                        check_writer_output(output, budget)?;
                    }
                    MacroPrimitive::CenterLine {
                        exposure,
                        width_mm,
                        height_mm,
                        center,
                        rotation_deg,
                    } => {
                        output.push_str(&format!(
                            "21,{},{},{},{},{},{}*",
                            exposure_number(*exposure),
                            number(*width_mm),
                            number(*height_mm),
                            number(center.x_mm),
                            number(center.y_mm),
                            number(*rotation_deg)
                        ));
                        check_writer_output(output, budget)?;
                    }
                    MacroPrimitive::Outline {
                        exposure,
                        points,
                        rotation_deg,
                    } => {
                        if points.len() < 4 || points.first() != points.last() {
                            return Err(semantic(0, "macro outline is not closed"));
                        }
                        output.push_str(&format!(
                            "4,{},{}",
                            exposure_number(*exposure),
                            points.len() - 1
                        ));
                        check_writer_output(output, budget)?;
                        for point in points {
                            output.push_str(&format!(
                                ",{},{}",
                                number(point.x_mm),
                                number(point.y_mm)
                            ));
                            check_writer_output(output, budget)?;
                        }
                        output.push_str(&format!(",{}*", number(*rotation_deg)));
                        check_writer_output(output, budget)?;
                    }
                }
            }
            output.push_str("%\n");
            check_writer_output(output, budget)?;
            output.push_str(&format!("%ADD{code}{name}*%\n"));
            check_writer_output(output, budget)?;
        }
    }
    Ok(())
}

fn hole_diameter(hole: Option<f64>) -> String {
    hole.map(|value| format!("X{}", number(value)))
        .unwrap_or_default()
}

fn exposure_number(exposure: Exposure) -> u8 {
    if matches!(exposure, Exposure::Dark) {
        1
    } else {
        0
    }
}

fn number(value: f64) -> String {
    let value = if value == 0.0 { 0.0 } else { value };
    format!("{value:.15}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

fn coord(value: f64) -> Result<String, S1Error> {
    let scaled = (value * 1_000_000.0).round();
    if !scaled.is_finite() || scaled.abs() >= 1_000_000_000_000.0 {
        return Err(semantic(0, "coordinate cannot be represented by FS66"));
    }
    Ok(format!("{scaled:.0}"))
}

fn emit_flash(output: &mut String, point: MmPoint) -> Result<(), S1Error> {
    output.push_str(&format!(
        "X{}Y{}D03*\n",
        coord(point.x_mm)?,
        coord(point.y_mm)?
    ));
    Ok(())
}

fn emit_move(output: &mut String, point: MmPoint) -> Result<(), S1Error> {
    output.push_str(&format!(
        "X{}Y{}D02*\n",
        coord(point.x_mm)?,
        coord(point.y_mm)?
    ));
    Ok(())
}

fn emit_line(output: &mut String, point: MmPoint) -> Result<(), S1Error> {
    output.push_str(&format!(
        "G01X{}Y{}D01*\n",
        coord(point.x_mm)?,
        coord(point.y_mm)?
    ));
    Ok(())
}

fn emit_arc(output: &mut String, path: ArcGeometry) -> Result<(), S1Error> {
    // A zero-angle circular sweep is exactly a zero-length circular-aperture
    // linear sweep. Emit G01 so older readers cannot turn G74 into a circle.
    if path.zero_sweep() {
        return emit_line(output, path.end);
    }

    // The parser reconstructs the center as quantized start + quantized I/J.
    // Quantize the start first when calculating the offsets so center error is
    // bounded by one output coordinate quantum's half, rather than summing
    // two unrelated roundings.
    let start_x = quantized(path.start.x_mm);
    let start_y = quantized(path.start.y_mm);
    output.push_str("G75*\n");
    let i = coord(path.center.x_mm - start_x)?;
    let j = coord(path.center.y_mm - start_y)?;
    let command = if matches!(path.direction, ArcDirection::Clockwise) {
        "G02"
    } else {
        "G03"
    };
    output.push_str(&format!(
        "{command}X{}Y{}I{i}J{j}D01*\n",
        coord(path.end.x_mm)?,
        coord(path.end.y_mm)?
    ));
    Ok(())
}

fn quantized(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}

fn emit_transform(output: &mut String, mirror: Mirror, rotation: f64, scale: f64) {
    let mirror = match mirror {
        Mirror::None => "N",
        Mirror::X => "X",
        Mirror::Y => "Y",
        Mirror::Xy => "XY",
    };
    output.push_str(&format!(
        "%LM{mirror}*%\n%LR{}*%\n%LS{}*%\n",
        number(rotation),
        number(scale)
    ));
}

fn set_polarity(output: &mut String, current: &mut Exposure, next: Exposure) {
    if *current != next {
        output.push_str(if matches!(next, Exposure::Dark) {
            "%LPD*%\n"
        } else {
            "%LPC*%\n"
        });
        *current = next;
    }
}
