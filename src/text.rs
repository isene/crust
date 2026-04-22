//! Text-formatting helpers shared across fe2o3 TUIs.
//!
//! Currently: Markdown-table detection + Unicode-box rendering. Plugs into
//! body-render pipelines (kastrup message view, pointer markdown preview).
//! HTML table extraction is intentionally out of scope here — scroll has
//! its own HTML renderer, and kastrup only needs simple HTML table
//! replacement which it can do via a tiny wrapper before calling us.

/// Maximum width per column before word-wrapping kicks in. Keeps tables
/// readable in narrow panes.
const MAX_COL_WIDTH: usize = 40;

/// Scan `body` for Markdown tables and replace each with a Unicode-box
/// formatted block sized to fit within `max_width` columns. Non-table text
/// passes through verbatim.
///
/// A Markdown table is recognised by:
/// - a header row of the form `| cell | cell | ... |`
/// - followed by a separator `| --- | ---: |` etc.
/// - followed by zero or more body rows of the same shape.
pub fn format_markdown_tables(body: &str, max_width: usize) -> String {
    let lines: Vec<&str> = body.lines().collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        if is_pipe_row(lines[i])
            && i + 1 < lines.len()
            && is_md_separator(lines[i + 1])
        {
            let header = split_pipe_row(lines[i]);
            let aligns = parse_md_alignments(lines[i + 1]);
            let mut rows: Vec<Vec<String>> = vec![header];
            let mut j = i + 2;
            while j < lines.len() && is_pipe_row(lines[j]) {
                rows.push(split_pipe_row(lines[j]));
                j += 1;
            }
            out.push(format_table(&rows, &aligns, max_width));
            i = j;
            continue;
        }
        out.push(lines[i].to_string());
        i += 1;
    }
    out.join("\n")
}

#[derive(Clone, Copy, Debug)]
pub enum Align { Left, Right, Center }

/// Render a table given a cell matrix and per-column alignments. The first
/// row is treated as the header. `max_width` is the total output budget;
/// the renderer clamps column widths so the whole frame fits.
pub fn format_table(rows: &[Vec<String>], aligns: &[Align], max_width: usize) -> String {
    if rows.is_empty() { return String::new(); }
    let n_cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    if n_cols == 0 { return String::new(); }

    // Normalize rows to same column count.
    let rows: Vec<Vec<String>> = rows.iter()
        .map(|r| {
            let mut r2 = r.clone();
            while r2.len() < n_cols { r2.push(String::new()); }
            r2
        })
        .collect();

    // Column widths: natural width = max cell display-width, clamped to
    // MAX_COL_WIDTH. Then if the overall frame exceeds max_width, shrink
    // the widest columns until we fit or can't shrink further.
    let mut widths: Vec<usize> = (0..n_cols).map(|c| {
        rows.iter()
            .map(|r| display_width_cell(&r[c]))
            .max().unwrap_or(0)
            .min(MAX_COL_WIDTH)
            .max(1)
    }).collect();

    // Frame overhead: "│ " prefix + " │ " between cols + " │" suffix.
    // = 2 + 3 * (n_cols - 1) + 2 = 3 * n_cols + 1
    let overhead = 3 * n_cols + 1;
    while widths.iter().sum::<usize>() + overhead > max_width {
        // Shrink the widest column by 1 each pass.
        let max_w = *widths.iter().max().unwrap_or(&0);
        if max_w <= 3 { break; }
        if let Some(ix) = widths.iter().position(|w| *w == max_w) {
            widths[ix] -= 1;
        }
    }

    let aligns: Vec<Align> = (0..n_cols)
        .map(|c| aligns.get(c).copied().unwrap_or(Align::Left))
        .collect();

    let mut out = String::new();

    // Top border: ┌────┬────┐
    out.push_str(&border_line(&widths, '┌', '┬', '┐'));
    out.push('\n');

    // Header row.
    out.push_str(&format_row(&rows[0], &widths, &aligns));
    out.push('\n');

    // Header separator.
    out.push_str(&border_line(&widths, '├', '┼', '┤'));
    out.push('\n');

    // Body rows.
    for row in &rows[1..] {
        out.push_str(&format_row(row, &widths, &aligns));
        out.push('\n');
    }

    // Bottom border.
    out.push_str(&border_line(&widths, '└', '┴', '┘'));

    out
}

fn border_line(widths: &[usize], left: char, mid: char, right: char) -> String {
    let mut s = String::new();
    s.push(left);
    for (i, w) in widths.iter().enumerate() {
        s.push_str(&"─".repeat(w + 2));
        if i + 1 < widths.len() { s.push(mid); }
    }
    s.push(right);
    s
}

fn format_row(row: &[String], widths: &[usize], aligns: &[Align]) -> String {
    // Wrap each cell into physical lines bounded by its column width.
    let wrapped: Vec<Vec<String>> = row.iter().zip(widths.iter())
        .map(|(cell, &w)| wrap_cell(cell, w))
        .collect();
    let max_lines = wrapped.iter().map(|v| v.len()).max().unwrap_or(1);

    let mut out = String::new();
    for li in 0..max_lines {
        out.push('│');
        for ci in 0..row.len() {
            let w = widths[ci];
            let cell = wrapped[ci].get(li).cloned().unwrap_or_default();
            out.push(' ');
            out.push_str(&align_cell(&cell, w, aligns[ci]));
            out.push(' ');
            out.push('│');
        }
        if li + 1 < max_lines { out.push('\n'); }
    }
    out
}

fn align_cell(s: &str, w: usize, a: Align) -> String {
    let cw = display_width_cell(s);
    if cw >= w { return s.to_string(); }
    let pad = w - cw;
    match a {
        Align::Left   => format!("{}{}", s, " ".repeat(pad)),
        Align::Right  => format!("{}{}", " ".repeat(pad), s),
        Align::Center => {
            let l = pad / 2;
            let r = pad - l;
            format!("{}{}{}", " ".repeat(l), s, " ".repeat(r))
        }
    }
}

/// Word-wrap `s` so every returned line's display-width is ≤ `w`. Breaks
/// on whitespace when possible; long tokens get hard-cut at `w`.
fn wrap_cell(s: &str, w: usize) -> Vec<String> {
    if w == 0 { return vec![String::new()]; }
    let mut lines: Vec<String> = Vec::new();
    for para in s.split('\n') {
        if display_width_cell(para) <= w {
            lines.push(para.to_string());
            continue;
        }
        let mut cur = String::new();
        for word in para.split_whitespace() {
            let wd = display_width_cell(word);
            if wd > w {
                // Hard-cut long token.
                if !cur.is_empty() { lines.push(std::mem::take(&mut cur)); }
                let mut remaining = word.to_string();
                while display_width_cell(&remaining) > w {
                    let head: String = remaining.chars().take(w).collect();
                    lines.push(head);
                    remaining = remaining.chars().skip(w).collect();
                }
                if !remaining.is_empty() { cur = remaining; }
                continue;
            }
            if cur.is_empty() {
                cur = word.to_string();
            } else if display_width_cell(&cur) + 1 + wd <= w {
                cur.push(' ');
                cur.push_str(word);
            } else {
                lines.push(std::mem::take(&mut cur));
                cur = word.to_string();
            }
        }
        if !cur.is_empty() { lines.push(cur); }
    }
    if lines.is_empty() { lines.push(String::new()); }
    lines
}

fn is_pipe_row(line: &str) -> bool {
    let t = line.trim();
    t.starts_with('|') && t.ends_with('|') && t.matches('|').count() >= 2
}

/// Match `| --- | :---: | ---: |` separator rows.
fn is_md_separator(line: &str) -> bool {
    let t = line.trim();
    if !t.starts_with('|') || !t.ends_with('|') { return false; }
    t.trim_matches('|')
        .split('|')
        .all(|seg| {
            let s = seg.trim();
            !s.is_empty()
                && s.chars().all(|c| c == '-' || c == ':' || c == ' ')
                && s.contains('-')
        })
}

fn parse_md_alignments(line: &str) -> Vec<Align> {
    line.trim().trim_matches('|').split('|').map(|seg| {
        let s = seg.trim();
        let left = s.starts_with(':');
        let right = s.ends_with(':');
        match (left, right) {
            (true, true)  => Align::Center,
            (_,    true)  => Align::Right,
            _             => Align::Left,
        }
    }).collect()
}

fn split_pipe_row(line: &str) -> Vec<String> {
    line.trim().trim_matches('|').split('|')
        .map(|s| s.trim().to_string())
        .collect()
}

/// Display width ignoring ANSI escape sequences. Enough for our tables,
/// which get fed plain text; if callers pass pre-styled cells the counts
/// are still right.
fn display_width_cell(s: &str) -> usize {
    crate::display_width(s)
}
