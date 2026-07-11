//! Block-level parsing helpers: `[…]` extraction, layer-name parsing,
//! tap-row splitting.

pub(crate) fn extract_block(lines: &[&str], mut idx: usize) -> Option<(Vec<String>, usize)> {
    while idx < lines.len() && !lines[idx].contains('[') {
        idx += 1;
    }
    if idx >= lines.len() {
        return None;
    }
    let mut body = vec![];
    let opener = lines[idx];
    let after = &opener[opener.find('[').unwrap() + 1..];

    if let Some(close) = after.rfind(']') {
        let inner = after[..close].trim();
        if !inner.is_empty() {
            body.push(inner.to_string());
        }
        return Some((body, idx));
    }
    let t = after.trim();
    if !t.is_empty() {
        body.push(t.to_string());
    }
    idx += 1;
    while idx < lines.len() {
        let t = lines[idx].trim();
        if t == "]" || (t.ends_with(']') && !t.contains('|')) {
            let before = t.trim_end_matches(']').trim();
            if !before.is_empty() {
                body.push(before.to_string());
            }
            return Some((body, idx));
        }
        if !t.is_empty() {
            body.push(t.to_string());
        }
        idx += 1;
    }
    Some((body, idx))
}

/// Like `extract_block` but starts extraction from the LAST `[` on the opener
/// line. Used for bracket-named blocks like `[d],[k][...]` where the header
/// has bracket-enclosed names before the grid's opening `[`.
pub(crate) fn extract_block_from_last_bracket(
    lines: &[&str],
    idx: usize,
) -> Option<(Vec<String>, usize)> {
    if idx >= lines.len() {
        return None;
    }
    let opener = lines[idx];
    let last_open = opener.rfind('[')?;
    let after = &opener[last_open + 1..];

    let mut body = vec![];
    if let Some(close) = after.rfind(']') {
        let inner = after[..close].trim();
        if !inner.is_empty() {
            body.push(inner.to_string());
        }
        return Some((body, idx));
    }
    let t = after.trim();
    if !t.is_empty() {
        body.push(t.to_string());
    }
    let mut idx = idx + 1;
    while idx < lines.len() {
        let t = lines[idx].trim();
        if t == "]" || (t.ends_with(']') && !t.contains('|')) {
            let before = t.trim_end_matches(']').trim();
            if !before.is_empty() {
                body.push(before.to_string());
            }
            return Some((body, idx));
        }
        if !t.is_empty() {
            body.push(t.to_string());
        }
        idx += 1;
    }
    Some((body, idx))
}

pub(crate) fn normalize_layer_name(raw: &str) -> String {
    let t = raw.trim();
    t.strip_prefix('{')
        .and_then(|s| s.strip_suffix('}'))
        .or_else(|| t.strip_prefix('[').and_then(|s| s.strip_suffix(']')))
        .unwrap_or(t)
        .trim()
        .to_string()
}

pub(crate) fn parse_block_layer_names(starter: &str) -> Vec<String> {
    let head = match starter.find('[') {
        Some(pos) => &starter[..pos],
        None => starter,
    };
    let head = head.trim();
    if head.starts_with('-') {
        let rest = head.trim_start_matches('-').trim();
        return rest
            .split('-')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
    }
    let mut names = vec![];
    let mut rest = head;
    while let Some(pos) = rest.find('{') {
        if let Some(end) = rest[pos + 1..].find('}') {
            let name = rest[pos + 1..pos + 1 + end].trim();
            if !name.is_empty() {
                names.push(name.to_string());
            }
            rest = &rest[pos + 1 + end + 1..];
        } else {
            break;
        }
    }
    names
}

pub(crate) fn first_cell(row: &str) -> Option<String> {
    row.split('|')
        .map(str::trim)
        .find(|c| !c.is_empty())
        .map(|c| c.to_string())
}

pub(crate) fn is_self_marker(cell: &str, names: &[String]) -> bool {
    let inner = normalize_layer_name(cell);
    cell.starts_with('{') && cell.ends_with('}') && names.contains(&inner)
}

pub(crate) fn split_tap_row(body: &[String]) -> (&[String], Option<String>) {
    if body.len() >= 2 {
        if let Some(last) = body.last() {
            let total_cells = last.split('|').count();
            let non_empty = last.split('|').filter(|c| !c.trim().is_empty()).count();
            if total_cells <= 2 && non_empty >= 1 {
                return (&body[..body.len() - 1], first_cell(last));
            }
        }
    }
    (body, None)
}
