//! Strict S0 adapter around the real MakerPnP parser.
//!
//! The upstream parser recognizes more syntax than this proof can interpret.
//! We therefore scan the source before and after parsing and fail closed for
//! every command outside the S0 circle-flash subset.

use editor_core::{CircleAperture, Document, DrawObject, Exposure, Geometry, Layer, MmPoint};
use gerber_parser::gerber_types::{
    Aperture, Command, DCode, ExtendedCode, FunctionCode, GCode, MCode, Operation, Polarity, Unit,
};
use gerber_parser::{GerberDoc, parse};
use std::collections::HashMap;
use std::io::{BufReader, Cursor};

#[derive(Debug, Clone, PartialEq)]
pub struct S0Scene {
    pub document: Document,
    pub format: CoordinateFormatInfo,
    pub diagnostics: Vec<String>,
}

pub const MAX_SOURCE_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_OBJECTS: usize = 100_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoordinateFormatInfo {
    pub integer: u8,
    pub decimal: u8,
    pub leading_zero_omission: bool,
    pub absolute: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum S0Error {
    InvalidUtf8,
    Empty,
    Unsupported {
        line: usize,
        source: String,
    },
    DuplicateModal {
        kind: &'static str,
        line: usize,
    },
    ContentAfterEnd {
        line: usize,
    },
    MissingModal(&'static str),
    Parser(String),
    ParserCommand(String),
    InvalidGeometry(String),
    ResourceLimit {
        resource: &'static str,
        limit: usize,
        actual: usize,
    },
}

impl std::fmt::Display for S0Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for S0Error {}

/// Parse the deliberately small S0 subset using `gerber_parser` 0.5.0.
pub fn parse_s0(bytes: &[u8], document_id: &str) -> Result<S0Scene, S0Error> {
    if bytes.len() > MAX_SOURCE_BYTES {
        return Err(S0Error::ResourceLimit {
            resource: "source_bytes",
            limit: MAX_SOURCE_BYTES,
            actual: bytes.len(),
        });
    }
    let source = std::str::from_utf8(bytes).map_err(|_| S0Error::InvalidUtf8)?;
    let lines: Vec<(usize, &str)> = source
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let line = line.trim();
            (!line.is_empty()).then_some((index + 1, line))
        })
        .collect();
    if lines.is_empty() {
        return Err(S0Error::Empty);
    }
    scan_strict_lines(&lines)?;

    let doc = parse(BufReader::new(Cursor::new(bytes)))
        .map_err(|(_, error)| S0Error::Parser(format!("{error:?}")))?;
    if let Some(error) = doc.errors().first() {
        return Err(S0Error::Parser(format!("{error:?}")));
    }
    interpret(doc, document_id)
}

fn scan_strict_lines(lines: &[(usize, &str)]) -> Result<(), S0Error> {
    let mut fs_seen = false;
    let mut mo_seen = false;
    let mut end_seen = false;
    let mut apertures = std::collections::HashSet::new();
    let mut has_aperture_selection = false;
    let mut has_coordinate = false;
    let mut object_count = 0;
    for (line_no, line) in lines.iter().copied() {
        if end_seen {
            return Err(S0Error::ContentAfterEnd { line: line_no });
        }
        if line == "M02*" {
            end_seen = true;
            continue;
        }
        if line.starts_with("G04") {
            if !line.ends_with('*') || line.matches('*').count() != 1 {
                return Err(S0Error::Unsupported {
                    line: line_no,
                    source: line.to_string(),
                });
            }
            continue;
        }
        if line.starts_with('%') {
            if !line.ends_with('%') || line.matches('%').count() != 2 {
                return Err(S0Error::Unsupported {
                    line: line_no,
                    source: line.to_string(),
                });
            }
            let body = line
                .strip_prefix('%')
                .and_then(|value| value.strip_suffix('%'))
                .and_then(|value| value.strip_suffix('*'))
                .ok_or_else(|| S0Error::Unsupported {
                    line: line_no,
                    source: line.to_string(),
                })?;
            if body.starts_with("FS") {
                if fs_seen || !valid_fs(body) {
                    return Err(if fs_seen {
                        S0Error::DuplicateModal {
                            kind: "FS",
                            line: line_no,
                        }
                    } else {
                        S0Error::Unsupported {
                            line: line_no,
                            source: line.to_string(),
                        }
                    });
                }
                fs_seen = true;
            } else if body.starts_with("MO") {
                if mo_seen || !(body == "MOMM" || body == "MOIN") {
                    return Err(if mo_seen {
                        S0Error::DuplicateModal {
                            kind: "MO",
                            line: line_no,
                        }
                    } else {
                        S0Error::Unsupported {
                            line: line_no,
                            source: line.to_string(),
                        }
                    });
                }
                mo_seen = true;
            } else if body.starts_with("ADD") {
                if !fs_seen || !mo_seen {
                    return Err(S0Error::Unsupported {
                        line: line_no,
                        source: "aperture definition before FS/MO".into(),
                    });
                }
                if !valid_circle_aperture_definition(body) {
                    return Err(S0Error::Unsupported {
                        line: line_no,
                        source: line.to_string(),
                    });
                }
                let code = body
                    .strip_prefix("ADD")
                    .and_then(|rest| rest.find('C').map(|pos| &rest[..pos]))
                    .and_then(|code| code.parse::<u16>().ok())
                    .expect("valid_circle_aperture_definition checked code");
                if !apertures.insert(code) {
                    return Err(S0Error::DuplicateModal {
                        kind: "AD",
                        line: line_no,
                    });
                }
            } else if body == "LPD" || body == "LPC" {
                // S0 uses only explicit local polarity changes.
                if !fs_seen || !mo_seen {
                    return Err(S0Error::Unsupported {
                        line: line_no,
                        source: "polarity before FS/MO".into(),
                    });
                }
            } else {
                return Err(S0Error::Unsupported {
                    line: line_no,
                    source: line.to_string(),
                });
            }
            continue;
        }
        if line.ends_with('*') && (line == "D01*" || line == "D02*" || line.starts_with("G")) {
            return Err(S0Error::Unsupported {
                line: line_no,
                source: line.to_string(),
            });
        }
        if is_aperture_select(line) {
            if !fs_seen || !mo_seen {
                return Err(S0Error::Unsupported {
                    line: line_no,
                    source: "D code before FS/MO".into(),
                });
            }
            let code = line
                .strip_suffix('*')
                .and_then(|line| line.strip_prefix('D'))
                .and_then(|code| code.parse::<u16>().ok())
                .expect("is_aperture_select checked code");
            if !apertures.contains(&code) {
                return Err(S0Error::Unsupported {
                    line: line_no,
                    source: format!("D{code} selected before its AD definition"),
                });
            }
            has_aperture_selection = true;
            continue;
        } else if is_flash_command(line) {
            if !has_aperture_selection {
                return Err(S0Error::Unsupported {
                    line: line_no,
                    source: "flash before aperture selection".into(),
                });
            }
            if line == "D03*" && !has_coordinate {
                return Err(S0Error::Unsupported {
                    line: line_no,
                    source: "first flash must specify coordinates".into(),
                });
            }
            has_coordinate |= line != "D03*";
            object_count += 1;
            if object_count > MAX_OBJECTS {
                return Err(S0Error::ResourceLimit {
                    resource: "objects",
                    limit: MAX_OBJECTS,
                    actual: object_count,
                });
            }
            continue;
        }
        return Err(S0Error::Unsupported {
            line: line_no,
            source: line.to_string(),
        });
    }
    if !fs_seen {
        return Err(S0Error::MissingModal("FS"));
    }
    if !mo_seen {
        return Err(S0Error::MissingModal("MO"));
    }
    if !end_seen {
        return Err(S0Error::MissingModal("M02"));
    }
    if object_count == 0 {
        return Err(S0Error::ParserCommand(
            "S0 scene has no flash objects".into(),
        ));
    }
    Ok(())
}

fn valid_fs(body: &str) -> bool {
    let bytes = body.as_bytes();
    bytes.len() == 10
        && (bytes[0..3] == *b"FSL" || bytes[0..3] == *b"FST")
        && bytes[3] == b'A'
        && bytes[4] == b'X'
        && bytes[7] == b'Y'
        && bytes[5].is_ascii_digit()
        && bytes[6].is_ascii_digit()
        && bytes[8].is_ascii_digit()
        && bytes[9].is_ascii_digit()
        && bytes[5] == bytes[8]
        && bytes[6] == bytes[9]
        && (bytes[6] == b'4' || bytes[6] == b'5' || bytes[6] == b'6')
}

fn valid_circle_aperture_definition(body: &str) -> bool {
    let rest = body.strip_prefix("ADD").unwrap_or_default();
    let Some(shape_position) = rest.find('C') else {
        return false;
    };
    let (code, parameters) = rest.split_at(shape_position);
    if code.len() < 2 || code.parse::<u16>().ok().is_none() {
        return false;
    }
    let Some(parameters) = parameters.strip_prefix("C,") else {
        return false;
    };
    let values: Vec<_> = parameters.split('X').collect();
    if !(1..=2).contains(&values.len()) {
        return false;
    }
    if !values.iter().all(|value| valid_decimal_token(value)) {
        return false;
    }
    let Ok(diameter) = values[0].parse::<f64>() else {
        return false;
    };
    diameter.is_finite()
        && diameter > 0.0
        && values.get(1).is_none_or(|hole| {
            hole.parse::<f64>()
                .map(|value| value.is_finite() && value > 0.0 && value < diameter)
                .unwrap_or(false)
        })
}

fn valid_decimal_token(value: &str) -> bool {
    let value = value.strip_prefix(['+', '-']).unwrap_or(value);
    if value.is_empty() || value.matches('.').count() > 1 {
        return false;
    }
    let mut digits = 0;
    for character in value.chars() {
        if character == '.' {
            continue;
        }
        if !character.is_ascii_digit() {
            return false;
        }
        digits += 1;
    }
    digits > 0
}

fn is_aperture_select(line: &str) -> bool {
    line.strip_suffix('*')
        .and_then(|line| line.strip_prefix('D'))
        .is_some_and(|code| {
            code.len() >= 2
                && code.chars().all(|ch| ch.is_ascii_digit())
                && code.parse::<u16>().is_ok_and(|code| code >= 10)
        })
}

fn is_flash_command(line: &str) -> bool {
    let Some(body) = line.strip_suffix("D03*") else {
        return false;
    };
    if body.is_empty() {
        return true;
    }
    let mut seen_coordinate = false;
    let mut seen_x = false;
    let mut seen_y = false;
    let mut position = 0;
    while position < body.len() {
        let marker = body.as_bytes()[position] as char;
        if marker != 'X' && marker != 'Y' {
            return false;
        }
        if marker == 'X' {
            if seen_x || seen_y {
                return false;
            }
            seen_x = true;
        } else if seen_y {
            return false;
        } else {
            seen_y = true;
        }
        seen_coordinate = true;
        position += 1;
        let start = position;
        if position < body.len() && body.as_bytes()[position] as char == '-' {
            position += 1;
        }
        while position < body.len() && body.as_bytes()[position].is_ascii_digit() {
            position += 1;
        }
        if position == start || (position == start + 1 && body.as_bytes()[start] == b'-') {
            return false;
        }
    }
    seen_coordinate
}

fn interpret(doc: GerberDoc, document_id: &str) -> Result<S0Scene, S0Error> {
    let unit = doc.units.ok_or(S0Error::MissingModal("MO"))?;
    let format = doc
        .format_specification
        .ok_or(S0Error::MissingModal("FS"))?;
    let format_info = CoordinateFormatInfo {
        integer: format.integer,
        decimal: format.decimal,
        leading_zero_omission: matches!(
            format.zero_omission,
            gerber_parser::gerber_types::ZeroOmission::Leading
        ),
        absolute: matches!(
            format.coordinate_mode,
            gerber_parser::gerber_types::CoordinateMode::Absolute
        ),
    };
    if !format_info.absolute {
        return Err(S0Error::Unsupported {
            line: 0,
            source: "incremental coordinates".into(),
        });
    }
    let scale = match unit {
        Unit::Millimeters => 1.0,
        Unit::Inches => 25.4,
    };
    let apertures = collect_circles(&doc, scale)?;
    let mut document = Document::new(document_id);
    document.unit = "mm".to_string();
    let mut layer = Layer::new("layer-1", "S0 parsed layer");
    let mut active_aperture: Option<i32> = None;
    let mut polarity = Exposure::Dark;
    let mut current_x: Option<f64> = None;
    let mut current_y: Option<f64> = None;
    let mut object_index = 0_u64;
    for command in doc.commands().iter() {
        match command {
            Command::ExtendedCode(ExtendedCode::CoordinateFormat(_))
            | Command::ExtendedCode(ExtendedCode::Unit(_))
            | Command::ExtendedCode(ExtendedCode::ApertureDefinition(_)) => {}
            Command::ExtendedCode(ExtendedCode::LoadPolarity(value)) => {
                polarity = match value {
                    Polarity::Dark => Exposure::Dark,
                    Polarity::Clear => Exposure::Clear,
                };
            }
            Command::FunctionCode(FunctionCode::DCode(DCode::SelectAperture(code))) => {
                if !apertures.contains_key(code) {
                    return Err(S0Error::ParserCommand(format!(
                        "undefined aperture D{code}"
                    )));
                }
                active_aperture = Some(*code);
            }
            Command::FunctionCode(FunctionCode::DCode(DCode::Operation(Operation::Flash(
                coords,
            )))) => {
                let code = active_aperture
                    .ok_or_else(|| S0Error::ParserCommand("flash before aperture".into()))?;
                if let Some(coords) = coords {
                    if let Some(x) = coords.x {
                        let value = f64::from(x) * scale;
                        if !value.is_finite() {
                            return Err(S0Error::InvalidGeometry("non-finite X coordinate".into()));
                        }
                        current_x = Some(value);
                    } else if current_x.is_none() {
                        return Err(S0Error::ParserCommand("first flash must specify X".into()));
                    }
                    if let Some(y) = coords.y {
                        let value = f64::from(y) * scale;
                        if !value.is_finite() {
                            return Err(S0Error::InvalidGeometry("non-finite Y coordinate".into()));
                        }
                        current_y = Some(value);
                    } else if current_y.is_none() {
                        return Err(S0Error::ParserCommand("first flash must specify Y".into()));
                    }
                } else if current_x.is_none() || current_y.is_none() {
                    return Err(S0Error::ParserCommand(
                        "first flash must specify coordinates".into(),
                    ));
                }
                let current =
                    MmPoint::new(current_x.unwrap_or_default(), current_y.unwrap_or_default());
                let aperture = *apertures
                    .get(&code)
                    .ok_or_else(|| S0Error::ParserCommand(format!("undefined aperture D{code}")))?;
                object_index += 1;
                layer.objects.push(DrawObject {
                    object_id: format!("object-{object_index}"),
                    geometry: Geometry::CircleFlash {
                        center: current,
                        aperture,
                    },
                    exposure: polarity,
                });
            }
            Command::FunctionCode(FunctionCode::MCode(MCode::EndOfFile)) => {}
            Command::FunctionCode(FunctionCode::GCode(GCode::Comment(_))) => {}
            other => {
                return Err(S0Error::ParserCommand(format!(
                    "unsupported parsed command: {other:?}"
                )));
            }
        }
    }
    if layer.objects.is_empty() {
        return Err(S0Error::ParserCommand(
            "S0 scene has no flash objects".into(),
        ));
    }
    document.layers.push(layer);
    Ok(S0Scene {
        document,
        format: format_info,
        diagnostics: vec!["S0 strict circle flash parser; editing/export are unavailable".into()],
    })
}

fn collect_circles(doc: &GerberDoc, scale: f64) -> Result<HashMap<i32, CircleAperture>, S0Error> {
    let mut circles = HashMap::new();
    for (code, aperture) in &doc.apertures {
        let Aperture::Circle(circle) = aperture else {
            return Err(S0Error::Unsupported {
                line: 0,
                source: format!("non-circle aperture D{code}"),
            });
        };
        circles.insert(
            *code,
            CircleAperture::new(
                circle.diameter * scale,
                circle.hole_diameter.map(|hole| hole * scale),
            )
            .map_err(|error| S0Error::InvalidGeometry(error.to_string()))?,
        );
    }
    if circles.is_empty() {
        return Err(S0Error::ParserCommand("no aperture definitions".into()));
    }
    Ok(circles)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &[u8] = br#"G04 S0 sample*
%FSLAX24Y24*%
%MOMM*%
%ADD10C,10X4*%
%ADD11C,6*%
%ADD12C,2*%
%LPD*%
D10*
X0Y0D03*
%LPC*%
D11*
X0Y0D03*
%LPD*%
D12*
X0Y0D03*
M02*
"#;

    #[test]
    fn parser_adapter_reads_strict_circle_flash_fixture() {
        let scene = parse_s0(SAMPLE, "s0").unwrap();
        let layer = &scene.document.layers[0];
        assert_eq!(layer.objects.len(), 3);
        assert!(layer.coverage_at(MmPoint::new(0.0, 0.0)));
        assert!(!layer.coverage_at(MmPoint::new(2.0, 0.0)));
        assert!(layer.coverage_at(MmPoint::new(4.0, 0.0)));
        assert!(!layer.coverage_at(MmPoint::new(6.0, 0.0)));
    }

    #[test]
    fn parser_rejects_m99_and_trailing_content_even_if_upstream_misparses_it() {
        let bad = br#"%FSLAX24Y24*%
%MOMM*%
%ADD10C,1*%
D10*
X0Y0D03*
M99*
"#;
        assert!(matches!(
            parse_s0(bad, "bad"),
            Err(S0Error::Unsupported { .. }) | Err(S0Error::MissingModal("M02"))
        ));
        let trailing = [SAMPLE, b"X100Y100D03*\n"].concat();
        assert!(matches!(
            parse_s0(&trailing, "bad"),
            Err(S0Error::ContentAfterEnd { .. })
        ));
    }

    #[test]
    fn parser_rejects_duplicate_modal_headers() {
        let duplicate = br#"%FSLAX24Y24*%
%FSLAX24Y24*%
%MOMM*%
%ADD10C,1*%
D10*
X0Y0D03*
M02*
"#;
        assert!(matches!(
            parse_s0(duplicate, "bad"),
            Err(S0Error::DuplicateModal { kind: "FS", .. })
        ));
    }

    #[test]
    fn parser_rejects_scientific_aperture_numbers() {
        let scientific = br#"%FSLAX24Y24*%
%MOMM*%
%ADD10C,1e1*%
D10*
X0Y0D03*
M02*
"#;
        assert!(matches!(
            parse_s0(scientific, "bad"),
            Err(S0Error::Unsupported { .. })
        ));
    }
}
