use crust::{Crust, Pane, Input};
use crust::style;

fn main() {
    Crust::init();
    let (cols, rows) = Crust::terminal_size();

    // Top info bar
    let mut top = Pane::new(1, 1, cols, 1, 252, 236);
    top.say(&format!(" crust demo | {}x{} | Press q to quit, j/k to scroll", cols, rows));

    // Main content pane with border
    let mut main_pane = Pane::new(1, 2, cols / 2, rows - 2, 255, 0);
    main_pane.border = true;
    main_pane.border_refresh();

    let mut content = String::new();
    content.push_str(&style::bold(&style::fg("crust - Rust TUI Library", 220)));
    content.push('\n');
    content.push_str(&style::fg("Feature clone of rcurses", 245));
    content.push_str("\n\n");
    for i in 1..50 {
        let line = format!("Line {} - {}", i, style::fg("colored text", (i * 3 + 100) as u8));
        content.push_str(&line);
        content.push('\n');
    }
    main_pane.set_text(&content);
    main_pane.refresh();

    // Right pane
    let mut right = Pane::new(cols / 2 + 1, 2, cols / 2, rows - 2, 81, 234);
    right.border = true;
    right.border_refresh();
    right.set_text(&format!(
        "{}\n\n{}\n{}\n{}\n{}\n{}\n\n{}",
        style::bold("Features:"),
        style::fg("  Pane-based layout", 46),
        style::fg("  256-color support", 214),
        style::fg("  Diff-based rendering", 207),
        style::fg("  Word wrapping", 117),
        style::fg("  Scroll indicators", 220),
        style::fg("  Input handling (getchr)", 147),
    ));
    right.refresh();

    // Status bar
    let mut status = Pane::new(1, rows, cols, 1, 0, 252);
    status.say(&style::fg(" Ready", 22));

    // Main loop
    loop {
        if let Some(key) = Input::getchr(None) {
            match key.as_str() {
                "q" | "ESC" => break,
                "j" | "DOWN" => main_pane.linedown(),
                "k" | "UP" => main_pane.lineup(),
                "PgDOWN" | " " => main_pane.pagedown(),
                "PgUP" => main_pane.pageup(),
                "g" => main_pane.top(),
                "G" => main_pane.bottom(),
                _ => {
                    status.say(&format!(" Key: {}", style::fg(&key, 208)));
                }
            }
        }
    }

    Crust::cleanup();
}
