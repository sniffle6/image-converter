# aero-glass

`aero-glass` is a small Windows-only native backdrop material for Rust desktop applications. It owns the Windows Composition and DWM details behind one thread-affine handle while accepting any live Win32 window exposed through `raw-window-handle`.

```rust
use aero_glass::{AeroGlass, GlassConfig};

let mut glass = AeroGlass::attach(&window, GlassConfig::default())?;
glass.update(GlassConfig {
    translucency: 72,
    blur_radius: 18.0,
    ..GlassConfig::default()
})?;
glass.set_active(window_is_active)?;
```

The host window must have an alpha-capable transparent surface wherever the native material should show. Create, update, and drop `AeroGlass` on the window's UI thread. `InactiveBehavior::KeepGlass` is the default; `InactiveBehavior::Solid` is an explicit opt-in.

The Windows implementation uses documented Windows Composition host-backdrop, Direct2D Gaussian blur, and DWM host-backdrop APIs. It falls back with an error on non-Windows targets so the application can retain an opaque readable surface.

Run the deterministic Windows probe with:

```powershell
cargo run -p aero-glass --example probe -- --blur 18 --translucency 72
```
