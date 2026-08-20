# App icon

Convertalot uses the Camelot castle artwork from `Convertalot_App_Icon_Package/` as its application icon.

## What you see

- Explorer and the Start Menu shortcut show the multi-size Windows `.ico` baked into both executables.
- The running GUI uses the 256 px PNG for the taskbar / Alt-Tab icon.
- The custom title bar draws the 32 px PNG next to the CONVERTALOT label. The window has no OS decorations, so this is the in-app chrome icon.

## How to change it

1. Replace the files in `assets/` (`convertalot.ico`, `convertalot-256.png`, `convertalot-32.png`).
2. Rebuild. `build.rs` re-embeds the `.ico`; the PNGs are compiled into the GUI via `include_bytes!`.

The icon pack is the source artwork (other platforms, extra sizes). The app only needs the three files in `assets/`.

## Key files

- `assets/convertalot.ico` — Windows exe resource
- `assets/convertalot-256.png` — window / taskbar
- `assets/convertalot-32.png` — title bar
- `build.rs` — embeds the `.ico` on Windows (stages files in a temp dir so GNU `windres` does not choke on spaces in the repo path)
- `src/gui/icon.rs` — loads the PNGs
- `src/gui/main.rs` — viewport icon
- `src/gui/windows.rs` — title-bar image
