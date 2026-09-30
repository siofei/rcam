//! Data-only input adapters. ZIP/XML and source-layout errors never expose payloads.
use editor_core::pnp::*;
use quick_xml::{NsReader, XmlVersion, events::Event, name::ResolveResult};
use rcam_project::zip_codec::{ReadPolicy, read_zip};
use std::collections::BTreeMap;

const MAIN: &[u8] = b"http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const REL: &[u8] = b"http://schemas.openxmlformats.org/package/2006/relationships";
type Rows = Vec<(usize, Vec<String>)>;
type Failure = (usize, &'static str);
type Attributes = BTreeMap<String, String>;
enum Part {
    Start(String, Attributes),
    End(String),
    Text(String),
}
fn xml(
    data: &[u8],
    namespace: &[u8],
    mut visit: impl FnMut(Part) -> Result<(), Failure>,
) -> Result<(), Failure> {
    let text = std::str::from_utf8(data).map_err(|_| (0, "xml_encoding"))?;
    let mut reader = NsReader::from_str(text);
    reader.config_mut().check_comments = true;
    let mut depth = 0usize;
    let mut roots = 0;
    let mut workbook_root = false;
    let mut ignored_depth = None;
    loop {
        let decoder = reader.decoder();
        let (ns, event) = reader
            .read_resolved_event()
            .map_err(|_| (0, "malformed_xml"))?;
        let empty = matches!(&event, Event::Empty(_));
        match event {
            Event::Start(e) | Event::Empty(e) => {
                if ignored_depth.is_none()
                    && !matches!(ns, ResolveResult::Bound(n) if n.as_ref()==namespace)
                {
                    return Err((0, "xml_namespace"));
                }
                if depth == 0 {
                    roots += 1;
                    if roots > 1 {
                        return Err((0, "xml_root"));
                    }
                }
                depth += 1;
                if depth > 64 {
                    return Err((0, "xml_depth"));
                }
                let name = std::str::from_utf8(e.local_name().as_ref())
                    .map_err(|_| (0, "xml_name"))?
                    .to_owned();
                if depth == 1 {
                    workbook_root = namespace == MAIN && name == "workbook";
                }
                // Workbook extension metadata carries no sheet rows/cells. Skip
                // it as a bounded subtree; never relax namespaces in sheet data.
                if workbook_root && depth == 2 && name == "extLst" {
                    ignored_depth = Some(depth);
                }
                let mut attrs = Attributes::new();
                for a in e.attributes() {
                    let a = a.map_err(|_| (0, "xml_attribute"))?;
                    let key = std::str::from_utf8(a.key.as_ref())
                        .map_err(|_| (0, "xml_attribute"))?
                        .to_owned();
                    let value = a
                        .decoded_and_normalized_value(XmlVersion::Implicit1_0, decoder)
                        .map_err(|_| (0, "xml_attribute"))?;
                    if value.len() > MAX_FIELD_BYTES || attrs.len() > 32 {
                        return Err((0, "field_budget"));
                    }
                    if attrs.insert(key, value.into_owned()).is_some() {
                        return Err((0, "xml_attribute"));
                    }
                }
                if ignored_depth.is_none() {
                    visit(Part::Start(name.clone(), attrs))?;
                }
                if empty {
                    if ignored_depth.is_none() {
                        visit(Part::End(name))?;
                    }
                    if ignored_depth == Some(depth) {
                        ignored_depth = None;
                    }
                    depth -= 1;
                }
            }
            Event::End(e) => {
                if depth == 0 {
                    return Err((0, "malformed_xml"));
                }
                if ignored_depth.is_none() {
                    visit(Part::End(
                        std::str::from_utf8(e.local_name().as_ref())
                            .map_err(|_| (0, "xml_name"))?
                            .into(),
                    ))?;
                }
                if ignored_depth == Some(depth) {
                    ignored_depth = None;
                }
                depth -= 1;
            }
            Event::Text(t) => {
                let t = t
                    .xml_content(XmlVersion::Implicit1_0)
                    .map_err(|_| (0, "xml_encoding"))?;
                if depth == 0 && !t.trim().is_empty() {
                    return Err((0, "xml_root"));
                }
                if ignored_depth.is_none() {
                    visit(Part::Text(t.into_owned()))?;
                }
            }
            Event::CData(t) => {
                if depth == 0 {
                    return Err((0, "xml_root"));
                }
                visit(Part::Text(
                    t.decode().map_err(|_| (0, "xml_encoding"))?.into_owned(),
                ))?;
            }
            Event::GeneralRef(r) => {
                if depth == 0 {
                    return Err((0, "xml_root"));
                }
                let v = if let Some(c) = r.resolve_char_ref().map_err(|_| (0, "xml_entity"))? {
                    c.to_string()
                } else {
                    let name = r.decode().map_err(|_| (0, "xml_entity"))?;
                    quick_xml::escape::resolve_predefined_entity(&name)
                        .ok_or((0, "xml_entity"))?
                        .into()
                };
                if ignored_depth.is_none() {
                    visit(Part::Text(v))?;
                }
            }
            Event::DocType(_) => return Err((0, "xml_doctype")),
            Event::Decl(d) => {
                if roots != 0 || depth != 0 {
                    return Err((0, "xml_declaration"));
                }
                if let Some(enc) = d.encoding() {
                    let enc = enc.map_err(|_| (0, "xml_encoding"))?;
                    if !enc.eq_ignore_ascii_case(b"UTF-8") && !enc.eq_ignore_ascii_case(b"UTF8") {
                        return Err((0, "xml_encoding"));
                    }
                }
            }
            Event::Eof => {
                if roots != 1 || depth != 0 {
                    return Err((0, "malformed_xml"));
                }
                break;
            }
            _ => {}
        }
    }
    Ok(())
}
fn attr<'a>(a: &'a Attributes, key: &str) -> Result<&'a str, Failure> {
    a.get(key)
        .map(String::as_str)
        .ok_or((0, "missing_attribute"))
}
fn add(dst: &mut String, text: &str) -> Result<(), Failure> {
    if dst.len().saturating_add(text.len()) > MAX_FIELD_BYTES {
        return Err((0, "field_budget"));
    }
    dst.push_str(text);
    Ok(())
}
fn strings(data: &[u8]) -> Result<Vec<String>, Failure> {
    let mut result = vec![];
    let mut current = None;
    let mut stack = vec![];
    let mut total = 0usize;
    xml(data, MAIN, |part| {
        match part {
            Part::Start(name, _) => {
                if stack.is_empty() && name != "sst" {
                    return Err((0, "xml_root"));
                }
                if name == "si" {
                    if current.is_some() || stack.last().is_none_or(|s| s != "sst") {
                        return Err((0, "malformed_strings"));
                    }
                    current = Some(String::new());
                }
                if name == "rPh" {
                    return Err((0, "phonetic_strings_unsupported"));
                }
                stack.push(name);
            }
            Part::Text(t) => {
                if stack.last().is_some_and(|s| s == "t")
                    && let Some(c) = &mut current
                {
                    add(c, &t)?;
                }
            }
            Part::End(name) => {
                if name == "si" {
                    let c = current.take().ok_or((0, "malformed_strings"))?;
                    total = total.saturating_add(c.len());
                    if total > MAX_PNP_BYTES || result.len() >= MAX_COMPONENTS * MAX_COLUMNS {
                        return Err((0, "string_budget"));
                    }
                    result.push(c);
                }
                stack.pop();
            }
        }
        Ok(())
    })?;
    Ok(result)
}
fn coordinate(s: &str) -> Result<(usize, usize), Failure> {
    let n = s.bytes().take_while(u8::is_ascii_uppercase).count();
    if n == 0 || n >= s.len() {
        return Err((0, "cell_coordinate"));
    }
    let mut col = 0usize;
    for b in s[..n].bytes() {
        col = col
            .checked_mul(26)
            .and_then(|c| c.checked_add((b - b'A' + 1) as usize))
            .ok_or((0, "column_budget"))?;
    }
    let row = s[n..]
        .parse::<usize>()
        .map_err(|_| (0, "cell_coordinate"))?;
    if col > MAX_COLUMNS || row == 0 || row > MAX_PNP_LINES {
        return Err((row, "cell_budget"));
    }
    Ok((row, col - 1))
}
fn sheet(data: &[u8], shared: &[String], header: usize) -> Result<Rows, Failure> {
    let mut rows = vec![];
    let mut row = 0;
    let mut previous = 0;
    let mut fields = vec![];
    let mut cell: Option<(usize, String, String)> = None;
    let mut stack = vec![];
    let mut total = 0usize;
    let mut width = 0;
    let mut value_seen = false;
    xml(data, MAIN, |part| {
        match part {
            Part::Start(name, a) => {
                if stack.is_empty() && name != "worksheet" {
                    return Err((0, "xml_root"));
                }
                if name == "row" {
                    if stack.last().is_none_or(|s| s != "sheetData") || row != 0 {
                        return Err((0, "row_structure"));
                    }
                    row = attr(&a, "r")?.parse().map_err(|_| (0, "row_coordinate"))?;
                    if row <= previous || row > MAX_PNP_LINES {
                        return Err((row, "row_budget"));
                    }
                    previous = row;
                    fields = vec![];
                }
                if name == "c" {
                    if row == 0 || cell.is_some() || stack.last().is_none_or(|s| s != "row") {
                        return Err((row, "cell_structure"));
                    }
                    let (r, col) = coordinate(attr(&a, "r")?)?;
                    if r != row || col < fields.len() {
                        return Err((row, "cell_coordinate"));
                    }
                    fields.resize(col, String::new());
                    value_seen = false;
                    cell = Some((
                        col,
                        a.get("t").cloned().unwrap_or_else(|| "n".into()),
                        String::new(),
                    ));
                }
                if name == "v" {
                    if stack.last().is_none_or(|s| s != "c") || cell.is_none() || value_seen {
                        return Err((row, "cell_structure"));
                    }
                    if cell
                        .as_ref()
                        .is_some_and(|(_, kind, _)| kind == "inlineStr")
                    {
                        return Err((row, "cell_structure"));
                    }
                    value_seen = true;
                }
                if name == "t"
                    && cell
                        .as_ref()
                        .is_some_and(|(_, kind, _)| kind != "inlineStr")
                {
                    return Err((row, "cell_structure"));
                }
                if name == "f" {
                    return Err((row, "formula_unsupported"));
                }
                stack.push(name);
            }
            Part::Text(t) => {
                if matches!(stack.last().map(String::as_str), Some("v" | "t"))
                    && let Some((_, _, value)) = &mut cell
                {
                    add(value, &t).map_err(|(_, code)| (row, code))?;
                }
            }
            Part::End(name) => {
                if name == "c" {
                    let (_, kind, raw) = cell.take().ok_or((row, "cell_structure"))?;
                    let value = match kind.as_str() {
                        "s" => shared
                            .get(
                                raw.parse::<usize>()
                                    .map_err(|_| (row, "shared_string_index"))?,
                            )
                            .ok_or((row, "shared_string_index"))?
                            .clone(),
                        "inlineStr" => raw,
                        "n" => {
                            if !raw.is_empty() && !raw.parse::<f64>().is_ok_and(f64::is_finite) {
                                return Err((row, "invalid_numeric_cell"));
                            }
                            raw
                        }
                        "b" => match raw.as_str() {
                            "0" | "1" => raw,
                            _ => return Err((row, "invalid_boolean")),
                        },
                        _ => return Err((row, "cell_type_unsupported")),
                    };
                    total = total.saturating_add(value.len());
                    if total > MAX_PNP_BYTES {
                        return Err((row, "text_budget"));
                    }
                    fields.push(value);
                }
                if name == "row" {
                    if row == header {
                        width = fields.len();
                        rows.push((row, std::mem::take(&mut fields)));
                    } else if row > header && fields.iter().any(|f| !f.trim().is_empty()) {
                        if header == 0 && rows.is_empty() {
                            width = fields.len();
                            rows.push((0, (1..=width).map(|i| format!("Column {i}")).collect()));
                        }
                        fields.resize(fields.len().max(width), String::new());
                        rows.push((row, std::mem::take(&mut fields)));
                    }
                    if rows.len() > MAX_COMPONENTS + 1 {
                        return Err((row, "row_budget"));
                    }
                    row = 0;
                }
                stack.pop();
            }
        }
        Ok(())
    })?;
    if rows.first().is_none_or(|r| r.0 != header) {
        return Err((header, "missing_header"));
    }
    Ok(rows)
}
fn workbook(
    bytes: &[u8],
    selected: &str,
    header: usize,
    names: &mut Vec<String>,
) -> Result<Rows, Failure> {
    let entries = read_zip(
        bytes,
        &ReadPolicy {
            max_entries: 256,
            max_uncompressed_bytes: 64 * 1024 * 1024,
            max_entry_bytes: 32 * 1024 * 1024,
            max_path_len: 512,
        },
    )
    .map_err(|_| (0, "xlsx_archive_rejected"))?;
    let entries: BTreeMap<_, _> = entries.into_iter().map(|e| (e.path, e.data)).collect();
    if entries.keys().any(|n| {
        n.to_ascii_lowercase().ends_with("vbaproject.bin") || n.starts_with("xl/externalLinks/")
    }) {
        return Err((0, "active_workbook_unsupported"));
    }
    let get = |name: &str| {
        entries
            .get(name)
            .map(Vec::as_slice)
            .ok_or((0, "xlsx_part_missing"))
    };
    let mut rels = BTreeMap::new();
    let mut rel_stack = vec![];
    xml(get("xl/_rels/workbook.xml.rels")?, REL, |part| {
        match part {
            Part::Start(n, a) => {
                if rel_stack.is_empty() && n != "Relationships" {
                    return Err((0, "xml_root"));
                }
                if n == "Relationship" {
                    if rel_stack.last().is_none_or(|n| n != "Relationships") {
                        return Err((0, "relationship_structure"));
                    }
                    if a.get("TargetMode").is_some_and(|s| s == "External") {
                        return Err((0, "external_relationship"));
                    }
                    let id = attr(&a, "Id")?.to_owned();
                    if rels.insert(id, attr(&a, "Target")?.to_owned()).is_some() {
                        return Err((0, "duplicate_relationship"));
                    }
                }
                rel_stack.push(n);
            }
            Part::End(_) => {
                rel_stack.pop();
            }
            Part::Text(_) => {}
        }
        Ok(())
    })?;
    let mut path = None;
    let mut wb_stack = vec![];
    xml(get("xl/workbook.xml")?, MAIN, |part| {
        match part {
            Part::Start(n, a) => {
                if wb_stack.is_empty() && n != "workbook" {
                    return Err((0, "xml_root"));
                }
                if n == "sheet" {
                    if wb_stack.last().is_none_or(|n| n != "sheets") {
                        return Err((0, "worksheet_structure"));
                    }
                    let name = attr(&a, "name")?;
                    if names.iter().any(|s| s == name) || names.len() >= 32 {
                        return Err((0, "worksheet_budget"));
                    }
                    names.push(name.to_owned());
                    if name == selected {
                        let id = a
                            .iter()
                            .find(|(k, _)| k.ends_with(":id"))
                            .map(|(_, v)| v)
                            .ok_or((0, "sheet_relationship"))?;
                        path = Some(rels.get(id).ok_or((0, "sheet_relationship"))?.clone());
                    }
                }
                wb_stack.push(n);
            }
            Part::End(_) => {
                wb_stack.pop();
            }
            Part::Text(_) => {}
        }
        Ok(())
    })?;
    if selected.is_empty() {
        return Err((header, "worksheet_selection_required"));
    }
    let path = path.ok_or((header, "worksheet_not_found"))?;
    let path = if path.starts_with('/') {
        path.trim_start_matches('/').to_owned()
    } else {
        format!("xl/{path}")
    };
    if !path.starts_with("xl/worksheets/")
        || path
            .split('/')
            .any(|s| s.is_empty() || s == ".." || s == ".")
    {
        return Err((0, "worksheet_path"));
    }
    let shared = entries
        .get("xl/sharedStrings.xml")
        .map(|d| strings(d))
        .transpose()?
        .unwrap_or_default();
    sheet(get(&path)?, &shared, header)
}
fn fixed(bytes: &[u8], skip: usize, spans: &[PnpColumnSpan]) -> Result<Rows, Failure> {
    let text = std::str::from_utf8(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes))
        .map_err(|_| (0, "invalid_utf8"))?;
    let mut result = vec![(
        skip.max(1),
        (1..=spans.len()).map(|i| format!("Column {i}")).collect(),
    )];
    let mut total = 0usize;
    for (i, line) in text.lines().enumerate() {
        if i >= MAX_PNP_LINES {
            return Err((i + 1, "line_budget"));
        }
        if i < skip {
            continue;
        }
        if line.len() > MAX_FIELD_BYTES {
            return Err((i + 1, "field_budget"));
        }
        let mut fields = vec![];
        for c in spans {
            let end = c.end.unwrap_or(line.len()).min(line.len());
            let value = line
                .get(c.start..end)
                .ok_or((i + 1, "fixed_width_boundary"))?;
            total = total.saturating_add(value.len());
            if total > MAX_PNP_BYTES {
                return Err((i + 1, "text_budget"));
            }
            fields.push(value.trim().to_owned());
        }
        result.push((i + 1, fields));
        if result.len() > MAX_COMPONENTS + 1 {
            return Err((i + 1, "row_budget"));
        }
    }
    Ok(result)
}
fn suggestion(rows: &Rows, mapping: &PnpMapping) -> Option<PnpMapping> {
    let headers = &rows.first()?.1;
    let normalized: Vec<_> = headers
        .iter()
        .map(|h| {
            h.chars()
                .filter(|c| !c.is_whitespace() && *c != '.')
                .flat_map(char::to_lowercase)
                .collect::<String>()
        })
        .collect();
    let column = |aliases: &[&str]| -> Option<usize> {
        for alias in aliases {
            let matches: Vec<_> = normalized
                .iter()
                .enumerate()
                .filter(|(_, h)| h == alias)
                .map(|(i, _)| i)
                .collect();
            if matches.len() > 1 {
                return None;
            }
            if let Some(i) = matches.first() {
                return Some(*i);
            }
        }
        None
    };
    let mut m = mapping.clone();
    m.refdes = column(&["refdes", "ref", "designator"])?;
    m.x = column(&["posx", "x"])?;
    m.y = column(&["posy", "y"])?;
    m.rotation = column(&["rotation", "orient", "rot"])?;
    m.side = column(&["side", "layer"])?;
    m.footprint = column(&["footprint", "package", "partdecal", "cellname", "partname"]);
    m.value = column(&["value", "val", "parttype"]);
    let mut top = None;
    let mut bottom = None;
    for (_, fields) in rows.iter().skip(1) {
        let token = fields.get(m.side)?.trim();
        let slot = if token.eq_ignore_ascii_case("top") {
            &mut top
        } else if token.eq_ignore_ascii_case("bottom") {
            &mut bottom
        } else {
            return None;
        };
        if slot.as_ref().is_some_and(|v: &String| v != token) {
            return None;
        }
        *slot = Some(token.to_owned());
    }
    if let Some(top) = top {
        m.top_token = top;
    }
    if let Some(bottom) = bottom {
        m.bottom_token = bottom;
    }
    m.validate().then_some(m)
}
pub(crate) fn preview(
    bytes: &[u8],
    mapping: &PnpMapping,
) -> (PnpPreview, Vec<String>, Option<PnpMapping>, Option<PnpUnit>) {
    let mut names = vec![];
    let Some(source) = &mapping.source else {
        return (parse_pnp(bytes, mapping), names, None, None);
    };
    let declared_unit = if matches!(source, PnpSource::FixedWidth { .. }) {
        std::str::from_utf8(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes))
            .ok()
            .and_then(|text| text.lines().next())
            .and_then(|line| line.strip_prefix("UUNITS"))
            .and_then(|rest| rest.split_once('='))
            .and_then(|(_, unit)| match unit.trim() {
                "MILLIMETERS" => Some(PnpUnit::Mm),
                "INCHES" => Some(PnpUnit::Inch),
                _ => None,
            })
    } else {
        None
    };
    let result = if bytes.len() > MAX_PNP_BYTES {
        Err((0, "byte_budget"))
    } else {
        match source {
            PnpSource::Delimited {
                skip_lines,
                has_header,
            } => {
                if !source.valid() {
                    Err((0, "invalid_mapping"))
                } else {
                    read_pnp_rows(bytes, mapping.delimiter).map(|mut rows| {
                        rows.retain(|(line, _)| *line > *skip_lines);
                        if !has_header {
                            let width = rows.first().map_or(0, |(_, fields)| fields.len());
                            rows.insert(
                                0,
                                (0, (1..=width).map(|i| format!("Column {i}")).collect()),
                            );
                        }
                        rows
                    })
                }
            }
            PnpSource::Xlsx {
                worksheet,
                header_row,
            } => workbook(bytes, worksheet, *header_row, &mut names),
            PnpSource::FixedWidth {
                skip_lines,
                columns,
            } => {
                if source.valid() {
                    fixed(bytes, *skip_lines, columns)
                } else {
                    Err((0, "invalid_mapping"))
                }
            }
        }
    };
    let suggested = result
        .as_ref()
        .ok()
        .and_then(|rows| suggestion(rows, mapping));
    let mut out = match result {
        Ok(rows) => parse_pnp_table(&rows, mapping),
        Err((line, code)) => {
            let mut p = PnpPreview {
                sample_rows: vec![],
                headers: vec![],
                components: vec![],
                diagnostics: vec![],
                diagnostic_count: 0,
                row_count: 0,
            };
            p.error(line, "source", code);
            p
        }
    };
    if declared_unit.is_some_and(|u| u != mapping.unit) {
        out.components.clear();
        out.error(1, "source", "declared_unit_mismatch");
    }
    (out, names, suggested, declared_unit)
}
