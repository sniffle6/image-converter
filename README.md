# Image Converter

A fast Windows image converter with a native drag-and-drop desktop interface and a scriptable CLI. Both entry points use the same parallel conversion engine.

## Formats

| Format | Read | Write | Notes |
|---|---:|---:|---|
| PNG | Yes | Yes | Alpha preserved |
| JPEG | Yes | Yes | Quality 1–100; alpha is flattened by the JPEG color model |
| WebP | Yes | Yes | Lossless output |
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

```powershell
# Convert a folder recursively to JPEG, fitting each image inside 1920×1080.
image-converter.exe .\photos --format jpeg --width 1920 --height 1080 --quality 88

# Convert selected files to lossless WebP in one output folder.
image-converter.exe .\one.png .\two.jpg --format webp --output .\converted

# Resize to exact dimensions (aspect ratio may change).
image-converter.exe .\input.png --format png --width 512 --height 512 --exact

# Scale proportionally.
image-converter.exe .\input.png --percent 50
```

Run `image-converter.exe --help` for every option. Without `--output`, files go into a `converted` folder beside each source. Existing names are kept safe with `-2`, `-3`, and so on unless `--overwrite` is supplied.
