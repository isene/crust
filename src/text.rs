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

    // Minimal frame: one space padding on either side of each cell + a
    // single `│` between columns. No outer borders, no top/bottom rule.
    // Overhead per row: 1 leading space + (w + 2) per column + (n-1)
    // separators = 1 + sum(w) + 2*n + (n-1) = sum(w) + 3*n.
    let overhead = 3 * n_cols;
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

    // Header row.
    out.push_str(&format_row(&rows[0], &widths, &aligns));
    out.push('\n');

    // Header separator only — no outer borders. Lighter visual weight.
    out.push_str(&header_separator(&widths));
    out.push('\n');

    // Body rows.
    for (i, row) in rows[1..].iter().enumerate() {
        out.push_str(&format_row(row, &widths, &aligns));
        if i + 1 < rows.len() - 1 { out.push('\n'); }
    }

    out
}

fn header_separator(widths: &[usize]) -> String {
    let mut s = String::new();
    // Leading space to match row's " cell │ cell " indentation.
    s.push(' ');
    for (i, w) in widths.iter().enumerate() {
        // w visible chars + 1 space on each side, minus the leading space
        // which is already emitted above (or was consumed by the previous
        // cross).
        s.push_str(&"─".repeat(w + 1));
        if i + 1 < widths.len() {
            s.push('┼');
            s.push('─');
        }
    }
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
        out.push(' ');
        for ci in 0..row.len() {
            let w = widths[ci];
            let cell = wrapped[ci].get(li).cloned().unwrap_or_default();
            out.push_str(&align_cell(&cell, w, aligns[ci]));
            if ci + 1 < row.len() {
                out.push(' ');
                out.push('│');
                out.push(' ');
            }
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

// ── Wikipedia extract cleanup ────────────────────────────────────────
//
// Shared by the suite's article readers (elements, stars, particles).
// The TextExtracts API renders every <math> element as a stack of
// indented lines — one glyph per line — followed by a `{\displaystyle …}`
// annotation, and leaves empty parentheses wherever it dropped a
// pronunciation template. Both read as corruption in a terminal pane.

/// LaTeX commands worth a real character.
const TEX_SYMBOLS: &[(&str, &str)] = &[
    ("\\hbar", "ℏ"), ("\\varepsilon", "ε"), ("\\epsilon", "ε"), ("\\vartheta", "θ"),
    ("\\alpha", "α"), ("\\beta", "β"), ("\\gamma", "γ"), ("\\delta", "δ"),
    ("\\zeta", "ζ"), ("\\eta", "η"), ("\\theta", "θ"), ("\\iota", "ι"),
    ("\\kappa", "κ"), ("\\lambda", "λ"), ("\\mu", "μ"), ("\\nu", "ν"),
    ("\\xi", "ξ"), ("\\pi", "π"), ("\\rho", "ρ"), ("\\sigma", "σ"),
    ("\\tau", "τ"), ("\\upsilon", "υ"), ("\\phi", "φ"), ("\\chi", "χ"),
    ("\\psi", "ψ"), ("\\omega", "ω"),
    ("\\Gamma", "Γ"), ("\\Delta", "Δ"), ("\\Theta", "Θ"), ("\\Lambda", "Λ"),
    ("\\Sigma", "Σ"), ("\\Phi", "Φ"), ("\\Psi", "Ψ"), ("\\Omega", "Ω"),
    ("\\times", "×"), ("\\cdot", "·"), ("\\pm", "±"), ("\\mp", "∓"),
    ("\\rightarrow", "→"), ("\\leftarrow", "←"), ("\\to", "→"),
    ("\\approx", "≈"), ("\\equiv", "≡"), ("\\neq", "≠"), ("\\leq", "≤"),
    ("\\geq", "≥"), ("\\ll", "≪"), ("\\gg", "≫"), ("\\propto", "∝"),
    ("\\infty", "∞"), ("\\partial", "∂"), ("\\nabla", "∇"),
    ("\\sum", "Σ"), ("\\prod", "Π"), ("\\int", "∫"), ("\\pm", "±"),
    ("\\langle", "⟨"), ("\\rangle", "⟩"), ("\\dagger", "†"), ("\\ast", "*"),
];

/// Commands that carry no meaning once the formula is one line of text.
const TEX_NOISE: &[&str] = &[
    "\\displaystyle", "\\textstyle", "\\operatorname", "\\mathrm", "\\mathbf",
    "\\mathbb", "\\mathcal", "\\boldsymbol", "\\overline", "\\underline",
    "\\left", "\\right", "\\text", "\\vec", "\\hat", "\\bar", "\\tilde",
    "\\bigg", "\\Bigg", "\\big", "\\Big", "\\limits", "\\,", "\\;", "\\!", "\\ ",
];

/// Structure a flat glyph join cannot show, so the LaTeX is used instead.
const TEX_STRUCTURE: &[&str] = &["\\frac", "\\dfrac", "\\tfrac", "\\sqrt", "\\over", "\\binom"];

/// Clean a Wikipedia plain-text extract for reading in a pane: math
/// blocks become one inline expression, and the debris left by dropped
/// templates goes away.
pub fn clean_wiki_extract(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        let ln = lines[i];
        // A math block is a run of blank or indented lines carrying a
        // {\displaystyle …} annotation. A run without one is ordinary
        // blank space and falls through untouched.
        if ln.is_empty() || ln.starts_with("  ") {
            // A truly empty first line means the formula stood alone in
            // the source; two spaces means it sat inside a sentence.
            let display = ln.is_empty();
            let mut j = i;
            let mut toks: Vec<&str> = Vec::new();
            let mut latex: Option<&str> = None;
            while j < lines.len() && (lines[j].trim().is_empty() || lines[j].starts_with("  ")) {
                let t = lines[j].trim();
                if t.starts_with("{\\displaystyle") || t.starts_with("{\\textstyle") {
                    latex = Some(t);
                } else if !t.is_empty() {
                    toks.push(t);
                }
                j += 1;
            }
            if let Some(tex) = latex {
                let expr = if TEX_STRUCTURE.iter().any(|s| tex.contains(s)) {
                    tidy_tex(tex)
                } else {
                    toks.concat()
                };
                let inline = !display && out.last().is_some_and(|l| !l.trim().is_empty());
                if inline {
                    if let Some(last) = out.last_mut() {
                        last.push_str(&expr);
                        // An inline formula never breaks its own sentence.
                        if j < lines.len() && !lines[j].trim().is_empty() {
                            last.push_str(lines[j]);
                            j += 1;
                        }
                    }
                } else {
                    out.push(String::new());
                    out.push(format!("    {expr}"));
                }
                i = j;
                continue;
            }
        }
        out.push(ln.to_string());
        i += 1;
    }
    tidy_prose(&out)
}

/// `{\displaystyle E={\frac {a}{b}}}` → `E=(a)/(b)`.
fn tidy_tex(line: &str) -> String {
    let body = match line.find(' ') {
        Some(p) => &line[p + 1..],
        None => "",
    };
    let mut s = body.strip_suffix('}').unwrap_or(body).to_string();
    // Innermost first, so nesting unwinds over a few passes.
    for _ in 0..4 {
        s = two_arg(&s, "\\frac", "(", ")/(", ")");
        s = two_arg(&s, "\\dfrac", "(", ")/(", ")");
        s = two_arg(&s, "\\tfrac", "(", ")/(", ")");
        s = one_arg(&s, "\\sqrt", "√(", ")");
    }
    for (k, v) in TEX_SYMBOLS {
        s = s.replace(k, v);
    }
    for n in TEX_NOISE {
        s = s.replace(n, " ");
    }
    s = s.replace(['{', '}', '~'], " ").replace('\\', "");
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `cmd{A}{B}` → `open A mid B close`, only where neither argument nests.
fn two_arg(s: &str, cmd: &str, open: &str, mid: &str, close: &str) -> String {
    let Some(p) = s.find(cmd) else { return s.to_string() };
    let rest = s[p + cmd.len()..].trim_start();
    let off = s.len() - rest.len();
    let Some(a) = braced(rest) else { return s.to_string() };
    let after = rest[a.1..].trim_start();
    let off2 = s.len() - after.len();
    let Some(b) = braced(after) else { return s.to_string() };
    let _ = off;
    format!(
        "{}{open}{}{mid}{}{close}{}",
        &s[..p],
        &rest[a.0..a.1 - 1],
        &after[b.0..b.1 - 1],
        &s[off2 + b.1..]
    )
}

fn one_arg(s: &str, cmd: &str, open: &str, close: &str) -> String {
    let Some(p) = s.find(cmd) else { return s.to_string() };
    let rest = s[p + cmd.len()..].trim_start();
    let off = s.len() - rest.len();
    let Some(a) = braced(rest) else { return s.to_string() };
    format!("{}{open}{}{close}{}", &s[..p], &rest[a.0..a.1 - 1], &s[off + a.1..])
}

/// A `{…}` group with no nesting: returns (inner start, past the `}`).
fn braced(s: &str) -> Option<(usize, usize)> {
    if !s.starts_with('{') {
        return None;
    }
    let end = s[1..].find('}')? + 1;
    if s[1..end].contains('{') {
        return None;
    }
    Some((1, end + 1))
}

fn tidy_prose(lines: &[String]) -> String {
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    for line in lines {
        let cleaned = tidy_line(line);
        // Never more than one blank line in a row.
        if cleaned.trim().is_empty() && out.last().is_some_and(|l| l.trim().is_empty()) {
            continue;
        }
        out.push(cleaned);
    }
    out.join("\n")
}

/// One line of prose: drop the empty brackets and stray leading `;` the
/// extract leaves where a template used to be, and squeeze the gaps.
fn tidy_line(line: &str) -> String {
    let indent = line.len() - line.trim_start().len();
    let b: Vec<char> = line[indent..].chars().collect();
    let mut out: Vec<char> = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == '(' || c == '[' {
            let close = if c == '(' { ')' } else { ']' };
            let mut k = i + 1;
            while k < b.len() && matches!(b[k], ' ' | ';' | ',') {
                k += 1;
            }
            if b.get(k) == Some(&close) {
                // Nothing survived inside: drop the brackets, and the
                // space before them when punctuation follows.
                let next = b[k + 1..].iter().find(|c| **c != ' ');
                if matches!(next, Some(',' | '.' | ';' | ':')) {
                    while out.last() == Some(&' ') {
                        out.pop();
                    }
                }
                i = k + 1;
                continue;
            }
            out.push(c);
            i = k;
            continue;
        }
        if c == ' ' {
            let mut k = i;
            while k < b.len() && b[k] == ' ' {
                k += 1;
            }
            // No space before closing punctuation, and never a run.
            if !matches!(b.get(k), Some(',' | '.' | ';' | ':' | ')' | ']')) && !out.is_empty() {
                out.push(' ');
            }
            i = k;
            continue;
        }
        out.push(c);
        i += 1;
    }
    while out.last() == Some(&' ') {
        out.pop();
    }
    format!("{}{}", &line[..indent], out.into_iter().collect::<String>())
}
