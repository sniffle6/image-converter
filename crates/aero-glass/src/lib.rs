//! Supported Windows backdrop blur for native Rust windows.
//!
//! The interface is renderer-agnostic: callers provide a live Win32 window through
//! `raw-window-handle`, then update one material configuration. The Windows composition
//! objects, DispatcherQueue, effect graph, focus policy, and fallbacks stay inside.

use raw_window_handle::HasWindowHandle;
use std::{fmt, marker::PhantomData, rc::Rc};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RgbColor {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl RgbColor {
    pub const fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InactiveBehavior {
    KeepGlass,
    Solid,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlassConfig {
    pub tint: RgbColor,
    /// Percentage of the desktop allowed through the tint, from 0 through 100.
    pub translucency: u8,
    /// Gaussian blur radius in device-independent pixels, clamped to 0 through 64.
    pub blur_radius: f32,
    pub inactive_behavior: InactiveBehavior,
}

impl Default for GlassConfig {
    fn default() -> Self {
        Self {
            tint: RgbColor::new(25, 27, 31),
            translucency: 72,
            blur_radius: 18.0,
            inactive_behavior: InactiveBehavior::KeepGlass,
        }
    }
}

impl GlassConfig {
    fn normalized(self) -> Self {
        Self {
            translucency: self.translucency.min(100),
            blur_radius: self.blur_radius.clamp(0.0, 64.0),
            ..self
        }
    }

    fn tint_opacity(self, active: bool) -> u8 {
        if !active && self.inactive_behavior == InactiveBehavior::Solid {
            u8::MAX
        } else {
            let opacity = 100_u16 - u16::from(self.translucency.min(100));
            ((opacity * u16::from(u8::MAX)) / 100) as u8
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GlassStatus {
    Active,
    Unsupported,
}

#[derive(Debug)]
pub enum GlassError {
    UnsupportedWindowHandle,
    Platform(String),
}

impl fmt::Display for GlassError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedWindowHandle => formatter.write_str("the window is not a Win32 HWND"),
            Self::Platform(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for GlassError {}

/// Ask the native window manager to round a top-level window's corners.
///
/// Windows automatically suppresses rounding while the window is maximized. Callers should
/// treat an error as an unsupported-platform fallback rather than a fatal application error.
pub fn set_rounded_corners(window: &impl HasWindowHandle) -> Result<(), GlassError> {
    platform::set_rounded_corners(window)
}

/// A thread-affine native material attached to one window.
///
/// Create, update, and drop this value on the window's UI thread. Dropping it detaches the
/// composition target and restores the host-backdrop preference.
pub struct AeroGlass {
    backend: platform::Backend,
    config: GlassConfig,
    active: bool,
    _thread_affinity: PhantomData<Rc<()>>,
}

impl AeroGlass {
    pub fn attach(window: &impl HasWindowHandle, config: GlassConfig) -> Result<Self, GlassError> {
        let config = config.normalized();
        let backend = platform::Backend::attach(window, config)?;
        Ok(Self {
            backend,
            config,
            active: true,
            _thread_affinity: PhantomData,
        })
    }

    pub fn update(&mut self, config: GlassConfig) -> Result<GlassStatus, GlassError> {
        let config = config.normalized();
        self.backend.update(config, self.active)?;
        self.config = config;
        Ok(self.status())
    }

    pub fn set_active(&mut self, active: bool) -> Result<GlassStatus, GlassError> {
        if self.active != active {
            self.active = active;
            self.backend.update(self.config, active)?;
        }
        Ok(self.status())
    }

    pub fn status(&self) -> GlassStatus {
        self.backend.status()
    }
}

#[cfg(windows)]
mod platform;

#[cfg(not(windows))]
mod platform {
    use super::*;

    pub(super) fn set_rounded_corners(_window: &impl HasWindowHandle) -> Result<(), GlassError> {
        Err(GlassError::Platform(
            "native rounded corners are available only on Windows".into(),
        ))
    }

    pub(super) struct Backend;

    impl Backend {
        pub(super) fn attach(
            _window: &impl HasWindowHandle,
            _config: GlassConfig,
        ) -> Result<Self, GlassError> {
            Err(GlassError::Platform(
                "Aero Glass is available only on Windows".into(),
            ))
        }

        pub(super) fn update(
            &mut self,
            _config: GlassConfig,
            _active: bool,
        ) -> Result<(), GlassError> {
            Ok(())
        }

        pub(super) fn status(&self) -> GlassStatus {
            GlassStatus::Unsupported
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configuration_is_clamped_at_the_module_interface() {
        let config = GlassConfig {
            translucency: 250,
            blur_radius: 900.0,
            ..GlassConfig::default()
        }
        .normalized();
        assert_eq!(config.translucency, 100);
        assert_eq!(config.blur_radius, 64.0);
    }

    #[test]
    fn inactive_solid_is_opt_in() {
        let glass = GlassConfig::default();
        assert_eq!(glass.inactive_behavior, InactiveBehavior::KeepGlass);
        assert_eq!(glass.tint_opacity(false), glass.tint_opacity(true));

        let solid = GlassConfig {
            inactive_behavior: InactiveBehavior::Solid,
            ..glass
        };
        assert_eq!(solid.tint_opacity(false), u8::MAX);
    }
}
