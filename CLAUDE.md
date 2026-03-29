# Crust

Rust feature clone of [rcurses](https://github.com/isene/rcurses), a Ruby TUI library.

Provides pane-based terminal UI with ANSI colors, scrolling, input handling, popups, and Unicode support. Foundation library for Pointer, Scroll, Kastrup, and Tock.

## Build

```bash
PATH="/usr/bin:$PATH" cargo build --release
```

Note: `PATH` prefix needed to avoid `~/bin/cc` (Claude Code sessions) shadowing the C compiler.
