# Convertalot

Convertalot is a fast Windows image converter with a native drag-and-drop egui workbench and a scriptable CLI. Both entry points use the same two-stage, parallel Rust conversion engine: inputs are scanned and output names are reserved first, then image jobs run across the available CPU cores.

## Install

Install or update the latest release from PowerShell:

```powershell
irm https://raw.githubusercontent.com/sniffle6/image-converter/main/install.ps1 | iex
```

This downloads the newest [GitHub release](https://github.com/sniffle6/image-converter/releases), verifies its SHA-256 checksum, installs both executables to `%LOCALAPPDATA%\Programs\Convertalot`, adds that folder to your user PATH, and creates a Start Menu shortcut for the GUI.

Options are controlled with environment variables (the piped form cannot take parameters):

```powershell
# Pin a specific version.
$env:CONVERTALOT_VERSION = 'v0.1.0'; irm https://raw.githubusercontent.com/sniffle6/image-converter/main/install.ps1 | iex

# Uninstall.
$env:CONVERTALOT_UNINSTALL = '1'; irm https://raw.githubusercontent.com/sniffle6/image-converter/main/install.ps1 | iex
```

`CONVERTALOT_INSTALL_DIR` overrides the install folder, and `CONVERTALOT_NO_PATH=1` / `CONVERTALOT_NO_SHORTCUT=1` skip the PATH update and shortcut. Alternatively, download `install.ps1` and run it directly with `-Version`, `-InstallDir`, `-NoPath`, `-NoShortcut`, or `-Uninstall`.

Releases are cut automatically when a `vX.Y.Z` tag is pushed — see [docs/RELEASING.md](docs/RELEASING.md).

## Desktop app

The GUI opens at 900 × 620 and defaults to JPEG quality 90, fit inside 1920 × 1080. Drop files or recursive folders onto the window, review the expanded queue and destination names, then convert. Each row reports queued, converting, done, failed, or cancelled state. Cancelling stops work that has not started; completed files remain saved.

The conversion sidebar supports:

- PNG, JPEG, and lossless WebP output.
- Original, fit, exact, and percentage resizing.
- JPEG white, black, or custom `#RRGGBB` transparency backgrounds.
- A chosen output folder or a `converted` folder beside each source.
- Overwrite mode and four collision-safe naming styles.

Appearance settings include the built-in light, dark, and glass/fallback themes plus editable custom color tokens. Theme and JPEG background preferences persist under `%APPDATA%\Convertalot\settings.json`; invalid settings safely fall back to dark defaults.

Glass uses a supported Windows Composition host-backdrop brush with a Gaussian blur and tint, allowing windows and the desktop behind Convertalot to remain visible through the app. **Blur** controls diffusion from 0–64 px and **Translucency** controls how much of the backdrop passes through the tint. Glass remains active when Convertalot loses focus by default; enable **Solid when inactive** to opt into an opaque inactive surface. Unsupported or transparency-restricted systems retain a readable dark fallback.

The native material is isolated in the renderer-agnostic `crates/aero-glass` package so another Rust desktop application can attach the same effect through `raw-window-handle` without depending on egui.

## Formats

| Format | Read | Write | Notes |
|---|---:|---:|---|
| PNG | Yes | Yes | Alpha preserved |
| JPEG | Yes | Yes | Quality 1–100; alpha is composited onto the selected matte |
| WebP | Yes | Yes | Lossless; alpha preserved |
| BMP | Yes | No | Convert to PNG, JPEG, or WebP |
| TIFF | Yes | No | Convert to PNG, JPEG, or WebP |
| GIF | Yes | No | The first frame is converted |

EXIF orientation is applied before resizing. Other metadata is not copied in v1.

## Build

```powershell
cargo build --release
```

The executables are:

- `target\release\image-converter-gui.exe`
- `target\release\image-converter.exe`

## CLI

CLI defaults remain PNG, original size, white JPEG background, and dash-number collision names.

```powershell
# Convert a folder recursively to JPEG, fitting inside 1920×1080.
image-converter.exe .\photos --format jpeg --width 1920 --height 1080 --quality 88

# Composite transparent pixels onto a custom JPEG matte.
image-converter.exe .\logo.png --format jpeg --background "#102030"

# Use photo-copy.jpg, then photo-copy-2.jpg for collisions.
image-converter.exe .\photos --format webp --duplicate-style copy

# Convert selected files into one folder.
image-converter.exe .\one.png .\two.jpg --format webp --output .\converted

# Resize exactly (aspect ratio may change), or proportionally.
image-converter.exe .\input.png --format png --width 512 --height 512 --exact
image-converter.exe .\input.png --percent 50
```

`--duplicate-style` accepts `dash`, `underscore`, `parenthesized`, or `copy`. `--background` requires an exact `#RRGGBB` value and is rejected unless `--format jpeg` is selected. Run `image-converter.exe --help` for every option.

Without `--output`, files go into a `converted` folder beside each source. Every destination is reserved before parallel execution so duplicate stems cannot race into the same file.
