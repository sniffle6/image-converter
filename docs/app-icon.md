# App icon

Convertalot uses `assets/convertalot-icon.png` as the source artwork for its application icon.

## What you see

- Explorer and the Start Menu shortcut show the multi-size Windows `.ico` baked into both executables.
- The running GUI uses the 256 px PNG for the taskbar / Alt-Tab icon.
- The custom title bar draws the 32 px PNG next to the CONVERTALOT label. The window has no OS decorations, so this is the in-app chrome icon.

## How to change it

1. Replace `assets/convertalot-icon.png` with square RGBA source artwork.
2. Regenerate `convertalot.ico`, `convertalot-256.png`, and `convertalot-32.png` from that source.
3. Rebuild. `build.rs` re-embeds the `.ico`; the PNGs are compiled into the GUI via `include_bytes!`.

Keep the source artwork and all three generated files in `assets/` so future replacements have a clear source of truth.

## Key files

- `assets/convertalot-icon.png` — full-size source artwork
- `assets/convertalot.ico` — Windows exe resource
- `assets/convertalot-256.png` — window / taskbar
- `assets/convertalot-32.png` — title bar
- `build.rs` — embeds the `.ico` on Windows. GNU builds compile with `windres` from a space-free temp dir (the preprocessor splits unquoted paths). MSVC builds look up `rc.exe` in the Windows SDK; GitHub Actions does not put it on PATH.
- `src/gui/icon.rs` — loads the PNGs
- `src/gui/main.rs` — viewport icon
- `src/gui/windows.rs` — title-bar image
