//! Bounded, detached PnP data. No manufacturing geometry or filesystem access.
use crate::{MmPoint, board::*};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const MAX_PNP_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_COMPONENTS: usize = 100_000;
pub const MAX_FIELD_BYTES: usize = 1024;
pub const MAX_PNP_LINES: usize = 200_001;
pub const MAX_COLUMNS: usize = 32;
pub const MAX_DIAGNOSTICS: usize = 100;
pub const MAX_BOARD_MM: f64 = 1_000_000.;
pub const REGISTRATION_TOLERANCE_MM: f64 = 0.001;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PnpUnit {
    Mm,
    Inch,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Delimiter {
    Csv,
    Tsv,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PnpColumnSpan {
    pub start: usize,
    pub end: Option<usize>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PnpSource {
    Delimited {
        skip_lines: usize,
        has_header: bool,
    },
    Xlsx {
        worksheet: String,
        header_row: usize,
    },
    FixedWidth {
        skip_lines: usize,
        columns: Vec<PnpColumnSpan>,
    },
}
impl PnpSource {
    pub fn valid(&self) -> bool {
        match self {
            Self::Delimited { skip_lines, .. } => *skip_lines <= MAX_PNP_LINES,
            Self::Xlsx {
                worksheet,
                header_row,
            } => valid_text(worksheet) && *header_row <= MAX_PNP_LINES,
            Self::FixedWidth {
                skip_lines,
                columns,
            } => {
                *skip_lines <= MAX_PNP_LINES
                    && !columns.is_empty()
                    && columns.len() <= MAX_COLUMNS
                    && columns.iter().enumerate().all(|(i, c)| {
                        c.start <= MAX_FIELD_BYTES
                            && c.end.is_none_or(|e| e > c.start && e <= MAX_FIELD_BYTES)
                            && (c.end.is_some() || i + 1 == columns.len())
                            && (i == 0 || columns[i - 1].end.is_some_and(|e| e <= c.start))
                    })
            }
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PnpMapping {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<PnpSource>,
    pub delimiter: Delimiter,
    pub unit: PnpUnit,
    pub refdes: usize,
    pub x: usize,
    pub y: usize,
    pub rotation: usize,
    pub side: usize,
    pub footprint: Option<usize>,
    pub value: Option<usize>,
    pub top_token: String,
    pub bottom_token: String,
    pub clockwise: bool,
    pub rotation_offset_deg: f64,
    pub invert_y: bool,
}
impl PnpMapping {
    pub fn validate(&self) -> bool {
        let cols: Vec<_> = [
            Some(self.refdes),
            Some(self.x),
            Some(self.y),
            Some(self.rotation),
            Some(self.side),
            self.footprint,
            self.value,
        ]
        .into_iter()
        .flatten()
        .collect();
        let unique: HashSet<_> = cols.iter().collect();
        cols.iter().all(|c| *c < MAX_COLUMNS)
            && unique.len() == cols.len()
            && self.source.as_ref().is_none_or(PnpSource::valid)
            && (valid_text(&self.top_token)
                || (self.top_token.is_empty()
                    && matches!(self.source, Some(PnpSource::FixedWidth { .. }))))
            && (valid_text(&self.bottom_token)
                || (self.bottom_token.is_empty()
                    && matches!(self.source, Some(PnpSource::FixedWidth { .. }))))
            && self.top_token.trim() == self.top_token
            && self.bottom_token.trim() == self.bottom_token
            && self.top_token != self.bottom_token
            && self.rotation_offset_deg.is_finite()
            && self.rotation_offset_deg.abs() <= 360_000.
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PnpProvenance {
    pub basename: String,
    pub sha256: String,
    pub imported_unix_seconds: u64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RegistrationInput {
    Manual {
        transform: CoordinateTransform2D,
    },
    TwoPoint {
        board: [MmPoint; 2],
        world: [MmPoint; 2],
        reflect_x: bool,
    },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoardRegistration {
    pub input: RegistrationInput,
    pub transform: CoordinateTransform2D,
    pub board_distance_mm: Option<f64>,
    pub world_distance_mm: Option<f64>,
    pub residual_mm: f64,
}
fn bounded(p: MmPoint) -> bool {
    p.is_finite() && p.x_mm.abs() <= MAX_BOARD_MM && p.y_mm.abs() <= MAX_BOARD_MM
}
pub fn normalized_angle(a: f64) -> f64 {
    a.rem_euclid(360.)
}
impl CoordinateTransform2D {
    pub fn apply_direction_deg(&self, angle: f64) -> f64 {
        normalized_angle(self.rotation_deg + if self.reflect_x { 180. - angle } else { angle })
    }
}
pub fn registration(input: RegistrationInput) -> Result<BoardRegistration, &'static str> {
    let (transform, bd, wd, residual) = match &input {
        RegistrationInput::Manual { transform } => {
            if !transform.is_valid()
                || !bounded(transform.translation)
                || transform.rotation_deg.abs() > 360_000.
            {
                return Err("invalid_transform");
            }
            (*transform, None, None, 0.)
        }
        RegistrationInput::TwoPoint {
            board,
            world,
            reflect_x,
        } => {
            if !board.iter().chain(world).all(|p| bounded(*p)) {
                return Err("invalid_point");
            }
            let bd = board[0].distance_mm(board[1]);
            let wd = world[0].distance_mm(world[1]);
            if bd < 0.001 || wd < 0.001 {
                return Err("degenerate_baseline");
            }
            if (bd - wd).abs() > REGISTRATION_TOLERANCE_MM {
                return Err("distance_mismatch");
            }
            let dx = board[1].x_mm - board[0].x_mm;
            let dy = board[1].y_mm - board[0].y_mm;
            let angle = (world[1].y_mm - world[0].y_mm).atan2(world[1].x_mm - world[0].x_mm)
                - dy.atan2(if *reflect_x { -dx } else { dx });
            let mut t = CoordinateTransform2D {
                reflect_x: *reflect_x,
                rotation_deg: angle.to_degrees(),
                translation: MmPoint::new(0., 0.),
            };
            let p = t.apply(board[0]);
            t.translation = MmPoint::new(world[0].x_mm - p.x_mm, world[0].y_mm - p.y_mm);
            if !bounded(t.translation) {
                return Err("invalid_transform");
            }
            let residual = t
                .apply(board[0])
                .distance_mm(world[0])
                .max(t.apply(board[1]).distance_mm(world[1]));
            if residual > REGISTRATION_TOLERANCE_MM {
                return Err("residual_mismatch");
            }
            (t, Some(bd), Some(wd), residual)
        }
    };
    Ok(BoardRegistration {
        input,
        transform,
        board_distance_mm: bd,
        world_distance_mm: wd,
        residual_mm: residual,
    })
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoardState {
    pub components: std::sync::Arc<Vec<ComponentPlacement>>,
    pub mapping: PnpMapping,
    pub provenance: PnpProvenance,
    pub registration: Option<BoardRegistration>,
}
fn valid_text(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= MAX_FIELD_BYTES && !s.chars().any(char::is_control)
}
impl BoardState {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.components.is_empty()
            || self.components.len() > MAX_COMPONENTS
            || !self.mapping.validate()
        {
            return Err("component_budget_or_mapping");
        }
        if !valid_text(&self.provenance.basename)
            || self.provenance.basename.contains(['/', '\\', ':'])
            || self.provenance.basename == "."
            || self.provenance.basename == ".."
            || self.provenance.sha256.len() != 64
            || !self
                .provenance
                .sha256
                .bytes()
                .all(|c| c.is_ascii_hexdigit())
        {
            return Err("invalid_provenance");
        }
        if let Some(r) = &self.registration
            && registration(r.input.clone())? != *r
        {
            return Err("registration_proof_mismatch");
        }
        let text_bytes = self
            .components
            .iter()
            .try_fold(0usize, |n, c| {
                n.checked_add(c.id.0.len())?
                    .checked_add(c.refdes.len())?
                    .checked_add(c.footprint.as_ref().map_or(0, String::len))?
                    .checked_add(c.value.as_ref().map_or(0, String::len))
            })
            .ok_or("text_budget")?;
        if text_bytes > 40 * 1024 * 1024 {
            return Err("text_budget");
        }
        let mut ids = HashSet::new();
        let mut identities = HashSet::new();
        for c in self.components.iter() {
            let p = MmPoint::new(c.position.x_mm, c.position.y_mm);
            if !valid_text(&c.id.0)
                || !ids.insert(&c.id.0)
                || !valid_text(&c.refdes)
                || c.refdes.trim() != c.refdes
                || !identities.insert((c.side, &c.refdes))
                || !bounded(p)
                || !c.rotation_deg.is_finite()
                || !(0. ..360.).contains(&c.rotation_deg)
                || c.footprint.as_ref().is_some_and(|v| !valid_text(v))
                || c.value.as_ref().is_some_and(|v| !valid_text(v))
            {
                return Err("invalid_component");
            }
            if self
                .registration
                .as_ref()
                .is_some_and(|r| !bounded(r.transform.apply(p)))
            {
                return Err("world_out_of_bounds");
            }
        }
        Ok(())
    }
    pub fn history_bytes(&self) -> usize {
        // Reserved backing storage plus conservative allocation overhead. The
        // component table is shared by registration snapshots, not serialized
        // or copied into history. Saturation is a rejecting budget, never wrap.
        let base = 8192usize.saturating_add(
            self.components
                .capacity()
                .saturating_mul(std::mem::size_of::<ComponentPlacement>()),
        );
        self.components.iter().fold(base, |n, c| {
            n.saturating_add(64)
                .saturating_add(c.id.0.capacity())
                .saturating_add(c.refdes.capacity())
                .saturating_add(c.footprint.as_ref().map_or(0, String::capacity))
                .saturating_add(c.value.as_ref().map_or(0, String::capacity))
        })
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PnpDiagnostic {
    pub line: usize,
    pub field: String,
    pub code: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PnpSampleRow {
    pub line: usize,
    pub fields: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PnpPreview {
    #[serde(default)]
    pub sample_rows: Vec<PnpSampleRow>,
    pub headers: Vec<String>,
    pub components: Vec<ComponentPlacement>,
    pub diagnostics: Vec<PnpDiagnostic>,
    pub diagnostic_count: usize,
    pub row_count: usize,
}
impl PnpPreview {
    pub fn error(&mut self, line: usize, field: &str, code: &str) {
        self.diagnostic_count += 1;
        if self.diagnostics.len() < MAX_DIAGNOSTICS {
            self.diagnostics.push(PnpDiagnostic {
                line,
                field: field.into(),
                code: code.into(),
            });
        }
    }
    pub fn valid(&self) -> bool {
        self.diagnostic_count == 0 && !self.components.is_empty()
    }
}
/// Physical start-line numbers survive quoted newlines. Scanner has bounded work,
/// storage, fields and diagnostics even for malformed attacker-controlled input.
pub type PnpTableRows = Vec<(usize, Vec<String>)>;
fn rows(text: &str, delimiter: u8) -> Result<PnpTableRows, (usize, &'static str)> {
    let bytes = text.as_bytes();
    let mut result = vec![];
    let mut fields = vec![];
    let mut field = Vec::new();
    let (mut i, mut line, mut start) = (0, 1, 1);
    let (mut quoted, mut closed) = (false, false);
    while i < bytes.len() {
        let b = bytes[i];
        if quoted {
            if b == b'"' {
                if bytes.get(i + 1) == Some(&b'"') {
                    field.push(b'"');
                    i += 1;
                } else {
                    quoted = false;
                    closed = true;
                }
            } else {
                field.push(b);
                if b == b'\n' {
                    line += 1;
                }
            }
        } else if b == b'"' {
            if !field.is_empty() || closed {
                return Err((line, "invalid_quote"));
            }
            quoted = true;
        } else if b == delimiter || b == b'\n' || b == b'\r' {
            fields.push(
                String::from_utf8(std::mem::take(&mut field))
                    .map_err(|_| (line, "invalid_utf8"))?,
            );
            closed = false;
            if fields.len() > MAX_COLUMNS {
                return Err((line, "column_budget"));
            }
            if b != delimiter {
                if b == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
                    i += 1;
                }
                result.push((start, std::mem::take(&mut fields)));
                line += 1;
                start = line;
                if result.len() > MAX_COMPONENTS + 1 {
                    return Err((line, "row_budget"));
                }
            }
        } else {
            if closed {
                return Err((line, "trailing_quote_data"));
            }
            field.push(b);
        }
        if field.len() > MAX_FIELD_BYTES {
            return Err((line, "field_budget"));
        }
        if line > MAX_PNP_LINES {
            return Err((line, "line_budget"));
        }
        i += 1;
    }
    if quoted {
        return Err((start, "unclosed_quote"));
    }
    if !field.is_empty() || !fields.is_empty() || closed {
        fields.push(String::from_utf8(field).map_err(|_| (line, "invalid_utf8"))?);
        result.push((start, fields));
    }
    if result
        .last()
        .is_some_and(|(_, fields)| fields.len() > MAX_COLUMNS)
    {
        return Err((line, "column_budget"));
    }
    if result.len() > MAX_COMPONENTS + 1 {
        return Err((line, "row_budget"));
    }
    Ok(result)
}
/// Read a bounded delimited table without interpreting field roles or units.
pub fn read_pnp_rows(
    bytes: &[u8],
    delimiter: Delimiter,
) -> Result<PnpTableRows, (usize, &'static str)> {
    if bytes.len() > MAX_PNP_BYTES {
        return Err((0, "byte_budget"));
    }
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
    let text = std::str::from_utf8(bytes).map_err(|e| {
        (
            bytes[..e.valid_up_to()]
                .iter()
                .filter(|b| **b == b'\n')
                .count()
                + 1,
            "invalid_utf8",
        )
    })?;
    rows(
        text,
        match delimiter {
            Delimiter::Csv => b',',
            Delimiter::Tsv => b'\t',
        },
    )
}
pub fn parse_pnp(bytes: &[u8], mapping: &PnpMapping) -> PnpPreview {
    let result = if mapping.source.is_some() {
        Err((0, "requires_input_adapter"))
    } else {
        read_pnp_rows(bytes, mapping.delimiter)
    };
    match result {
        Ok(rows) => parse_pnp_table(&rows, mapping),
        Err((line, code)) => {
            let mut out = PnpPreview {
                sample_rows: vec![],
                headers: vec![],
                components: vec![],
                diagnostics: vec![],
                diagnostic_count: 0,
                row_count: 0,
            };
            out.error(line, "file", code);
            out
        }
    }
}
/// Source adapters preserve physical row numbers and share all business validation.
pub fn parse_pnp_table(rows: &[(usize, Vec<String>)], mapping: &PnpMapping) -> PnpPreview {
    let mut out = PnpPreview {
        sample_rows: vec![],
        headers: vec![],
        components: vec![],
        diagnostics: vec![],
        diagnostic_count: 0,
        row_count: 0,
    };
    let mut text_bytes = 0usize;
    if rows.len() > MAX_COMPONENTS + 1
        || rows.iter().enumerate().any(|(i, (line, fields))| {
            (*line == 0 && i != 0)
                || *line > MAX_PNP_LINES
                || fields.len() > MAX_COLUMNS
                || fields.iter().any(|f| {
                    text_bytes = text_bytes.saturating_add(f.len());
                    f.len() > MAX_FIELD_BYTES || text_bytes > MAX_PNP_BYTES
                })
        })
    {
        out.error(0, "file", "table_budget");
        return out;
    }
    let Some((_, headers)) = rows.first() else {
        out.error(1, "header", "missing_header");
        return out;
    };
    out.headers = headers.clone();
    out.row_count = rows.len().saturating_sub(1);
    out.sample_rows = rows
        .iter()
        .skip(1)
        .take(20)
        .map(|(line, fields)| PnpSampleRow {
            line: *line,
            fields: fields.clone(),
        })
        .collect();
    if !mapping.validate() {
        out.error(0, "mapping", "invalid_mapping");
        return out;
    }
    let indices: Vec<_> = [
        Some(mapping.refdes),
        Some(mapping.x),
        Some(mapping.y),
        Some(mapping.rotation),
        Some(mapping.side),
        mapping.footprint,
        mapping.value,
    ]
    .into_iter()
    .flatten()
    .collect();
    if headers.len() > MAX_COLUMNS
        || indices.iter().any(|i| *i >= headers.len())
        || (mapping.source.is_none()
            && (headers.iter().any(|h| !valid_text(h))
                || headers
                    .iter()
                    .map(|s| s.trim())
                    .collect::<HashSet<_>>()
                    .len()
                    != headers.len()))
    {
        out.error(rows[0].0, "header", "missing_or_duplicate_field");
        return out;
    }
    let mut identities = HashSet::new();
    let factor = match mapping.unit {
        PnpUnit::Mm => 1.,
        PnpUnit::Inch => 25.4,
    };
    for (line, fields) in rows.iter().skip(1) {
        let before = out.diagnostic_count;
        if fields.len() != headers.len() {
            out.error(*line, "row", "field_count");
            continue;
        }
        let refdes = fields[mapping.refdes].trim();
        if !valid_text(refdes) {
            out.error(*line, "refdes", "empty_or_invalid_refdes");
        }
        let side = if fields[mapping.side].trim() == mapping.top_token {
            Some(BoardSide::Top)
        } else if fields[mapping.side].trim() == mapping.bottom_token {
            Some(BoardSide::Bottom)
        } else {
            out.error(*line, "side", "unknown_side");
            None
        };
        if let Some(side) = side
            && !identities.insert((side, refdes.to_owned()))
        {
            out.error(*line, "refdes", "duplicate_identity");
        }
        let mut number = |index: usize, field: &str, mult: f64, limit: f64| -> Option<f64> {
            match fields[index].trim().parse::<f64>() {
                Ok(n) if n.is_finite() && (n * mult).is_finite() && (n * mult).abs() <= limit => {
                    Some(n * mult)
                }
                _ => {
                    out.error(*line, field, "invalid_number");
                    None
                }
            }
        };
        let x = number(mapping.x, "x", factor, MAX_BOARD_MM);
        let y = number(mapping.y, "y", factor, MAX_BOARD_MM);
        let a = number(mapping.rotation, "rotation", 1., 360_000.);
        let fp = mapping
            .footprint
            .map(|i| fields[i].trim().to_owned())
            .filter(|s| !s.is_empty());
        let value = mapping
            .value
            .map(|i| fields[i].trim().to_owned())
            .filter(|s| !s.is_empty());
        if fp.as_ref().is_some_and(|s| !valid_text(s))
            || value.as_ref().is_some_and(|s| !valid_text(s))
        {
            out.error(*line, "optional", "invalid_text");
        }
        if before == out.diagnostic_count {
            out.components.push(ComponentPlacement {
                id: ComponentId(format!("preview-{}", out.components.len())),
                refdes: refdes.into(),
                position: BoardPoint {
                    x_mm: x.unwrap(),
                    y_mm: y.unwrap() * if mapping.invert_y { -1. } else { 1. },
                },
                rotation_deg: normalized_angle(
                    (if mapping.clockwise {
                        -a.unwrap()
                    } else {
                        a.unwrap()
                    }) * if mapping.invert_y { -1. } else { 1. }
                        + mapping.rotation_offset_deg,
                ),
                side: side.unwrap(),
                footprint: fp,
                value,
            });
        }
    }
    if out.row_count == 0 {
        out.error(2, "row", "no_components");
    }
    // No partial successful batch may be mistaken for a usable import.
    if out.diagnostic_count > 0 {
        out.components.clear();
    }
    out
}
