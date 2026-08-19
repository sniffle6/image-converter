#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(windows))]
fn main() {
    eprintln!("The Aero Glass probe runs only on Windows.");
}

#[cfg(windows)]
mod windows_probe {
    use aero_glass::{AeroGlass, GlassConfig, InactiveBehavior, RgbColor};
    use raw_window_handle::{
        HandleError, HasWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle,
    };
    use std::{
        num::NonZeroIsize,
        time::{Duration, Instant},
    };
    use windows::{
        Win32::{
            Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
            Graphics::Gdi::{
                BeginPaint, CreateSolidBrush, DeleteObject, EndPaint, FillRect, PAINTSTRUCT,
            },
            System::LibraryLoader::GetModuleHandleW,
            UI::WindowsAndMessaging::{
                CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow,
                DispatchMessageW, GetForegroundWindow, GetMessageW, LWA_COLORKEY, MSG,
                PostQuitMessage, RegisterClassW, SW_SHOW, SetLayeredWindowAttributes, SetTimer,
                ShowWindow, TranslateMessage, WINDOW_EX_STYLE, WM_CLOSE, WM_DESTROY, WM_PAINT,
                WNDCLASSW, WS_EX_LAYERED, WS_OVERLAPPEDWINDOW, WS_POPUP, WS_VISIBLE,
            },
        },
        core::{Error, Result, w},
    };

    const WIDTH: i32 = 900;
    const HEIGHT: i32 = 620;
    const KEY_COLOR: COLORREF = COLORREF(0x0001_0203);

    struct ProbeArguments {
        blur: f32,
        translucency: u8,
        seconds: u64,
        x: i32,
        y: i32,
        force_inactive: bool,
        solid_inactive: bool,
    }

    #[derive(Clone, Copy)]
    struct ProbeWindow(HWND);

    impl HasWindowHandle for ProbeWindow {
        fn window_handle(&self) -> std::result::Result<WindowHandle<'_>, HandleError> {
            let raw = NonZeroIsize::new(self.0.0 as isize).ok_or(HandleError::Unavailable)?;
            let handle = Win32WindowHandle::new(raw);
            // SAFETY: this borrowed handle cannot outlive ProbeWindow, whose HWND remains live
            // for the complete AeroGlass lifetime below.
            Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::Win32(handle)) })
        }
    }

    struct NativeWindow(HWND);

    impl Drop for NativeWindow {
        fn drop(&mut self) {
            if !self.0.is_invalid() {
                // SAFETY: this value owns the HWND returned by CreateWindowExW.
                let _ = unsafe { DestroyWindow(self.0) };
            }
        }
    }

    pub fn run() -> Result<()> {
        let arguments = arguments();
        // SAFETY: None asks for the current executable module.
        let module = unsafe { GetModuleHandleW(None)? };
        let instance = HINSTANCE(module.0);
        register_classes(instance)?;

        let checker = NativeWindow(create_window(
            WINDOW_EX_STYLE::default(),
            w!("AeroGlassProbeChecker"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            arguments.x,
            arguments.y,
            WIDTH,
            HEIGHT,
            instance,
        )?);
        let host = NativeWindow(create_window(
            WS_EX_LAYERED,
            w!("AeroGlassProbeHost"),
            WS_POPUP | WS_VISIBLE,
            arguments.x,
            arguments.y,
            WIDTH,
            HEIGHT,
            instance,
        )?);
        // SAFETY: host is a live layered window. Every painted pixel uses KEY_COLOR, making the
        // probe host transparent while still accepting focus and messages.
        unsafe { SetLayeredWindowAttributes(host.0, KEY_COLOR, 255, LWA_COLORKEY)? };
        // SAFETY: both windows are valid and should be visible for the probe.
        unsafe {
            let _ = ShowWindow(checker.0, SW_SHOW);
            let _ = ShowWindow(host.0, SW_SHOW);
            SetTimer(Some(host.0), 1, 16, None);
        }

        let config = GlassConfig {
            tint: RgbColor::new(25, 27, 31),
            translucency: arguments.translucency,
            blur_radius: arguments.blur,
            inactive_behavior: if arguments.solid_inactive {
                InactiveBehavior::Solid
            } else {
                InactiveBehavior::KeepGlass
            },
        };
        let mut glass = AeroGlass::attach(&ProbeWindow(host.0), config).map_err(|error| {
            Error::new(
                windows::core::HRESULT(0x8000_4005_u32 as i32),
                error.to_string(),
            )
        })?;
        glass.update(config).map_err(|error| {
            Error::new(
                windows::core::HRESULT(0x8000_4005_u32 as i32),
                error.to_string(),
            )
        })?;
        println!(
            "AERO_PROBE host={:?} checker={:?} blur={} translucency={} active={} inactive={} duration={}s",
            host.0,
            checker.0,
            arguments.blur,
            arguments.translucency,
            !arguments.force_inactive,
            if arguments.solid_inactive {
                "solid"
            } else {
                "glass"
            },
            arguments.seconds,
        );

        let started = Instant::now();
        let mut message = MSG::default();
        loop {
            // SAFETY: standard UI-thread message loop with a valid MSG buffer.
            let result = unsafe { GetMessageW(&mut message, None, 0, 0) };
            if result.0 <= 0 {
                break;
            }
            // SAFETY: message was populated by GetMessageW.
            unsafe {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            let active = !arguments.force_inactive && unsafe { GetForegroundWindow() } == host.0;
            glass
                .set_active(active)
                .and_then(|_| glass.update(config))
                .map_err(|error| {
                    Error::new(
                        windows::core::HRESULT(0x8000_4005_u32 as i32),
                        error.to_string(),
                    )
                })?;
            if started.elapsed() >= Duration::from_secs(arguments.seconds) {
                break;
            }
        }
        Ok(())
    }

    fn arguments() -> ProbeArguments {
        let mut blur = 18.0;
        let mut translucency = 72;
        let mut seconds = 30;
        let mut x = 160;
        let mut y = 110;
        let mut force_inactive = false;
        let mut solid_inactive = false;
        let mut args = std::env::args().skip(1);
        while let Some(argument) = args.next() {
            match argument.as_str() {
                "--blur" => blur = args.next().and_then(|v| v.parse().ok()).unwrap_or(blur),
                "--translucency" => {
                    translucency = args
                        .next()
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(translucency)
                }
                "--seconds" => {
                    seconds = args.next().and_then(|v| v.parse().ok()).unwrap_or(seconds)
                }
                "--x" => x = args.next().and_then(|v| v.parse().ok()).unwrap_or(x),
                "--y" => y = args.next().and_then(|v| v.parse().ok()).unwrap_or(y),
                "--inactive" => force_inactive = true,
                "--solid-inactive" => solid_inactive = true,
                _ => {}
            }
        }
        ProbeArguments {
            blur,
            translucency,
            seconds,
            x,
            y,
            force_inactive,
            solid_inactive,
        }
    }

    fn register_classes(instance: HINSTANCE) -> Result<()> {
        for (name, procedure) in [
            (w!("AeroGlassProbeChecker"), checker_window_proc as _),
            (w!("AeroGlassProbeHost"), host_window_proc as _),
        ] {
            let class = WNDCLASSW {
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(procedure),
                hInstance: instance,
                lpszClassName: name,
                ..Default::default()
            };
            // SAFETY: class names are static and procedures live for the process lifetime.
            if unsafe { RegisterClassW(&class) } == 0 {
                return Err(Error::from_thread());
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn create_window(
        ex_style: WINDOW_EX_STYLE,
        class: windows::core::PCWSTR,
        style: windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        instance: HINSTANCE,
    ) -> Result<HWND> {
        // SAFETY: the class was registered above and all handles/parameters are valid.
        unsafe {
            CreateWindowExW(
                ex_style,
                class,
                w!("Aero Glass package probe"),
                style,
                x,
                y,
                width,
                height,
                None,
                None,
                Some(instance),
                None,
            )
        }
    }

    unsafe extern "system" fn checker_window_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match message {
            WM_PAINT => {
                let mut paint = PAINTSTRUCT::default();
                // SAFETY: called for WM_PAINT with a live HWND and valid PAINTSTRUCT.
                let dc = unsafe { BeginPaint(hwnd, &mut paint) };
                let colors = [
                    COLORREF(0x0038_38e8),
                    COLORREF(0x0038_d8e8),
                    COLORREF(0x0038_d858),
                    COLORREF(0x00e8_c838),
                    COLORREF(0x00e8_4838),
                    COLORREF(0x00d8_38d8),
                ];
                for (index, color) in colors.into_iter().enumerate() {
                    // SAFETY: brush is deleted after FillRect and dc is valid until EndPaint.
                    let brush = unsafe { CreateSolidBrush(color) };
                    let left = index as i32 * WIDTH / colors.len() as i32;
                    let right = (index as i32 + 1) * WIDTH / colors.len() as i32;
                    let rect = RECT {
                        left,
                        top: 0,
                        right,
                        bottom: HEIGHT,
                    };
                    unsafe {
                        let _ = FillRect(dc, &rect, brush);
                        let _ = DeleteObject(brush.into());
                    }
                }
                let _ = unsafe { EndPaint(hwnd, &paint) };
                LRESULT(0)
            }
            WM_CLOSE => {
                unsafe { DestroyWindow(hwnd) }.ok();
                LRESULT(0)
            }
            WM_DESTROY => {
                unsafe { PostQuitMessage(0) };
                LRESULT(0)
            }
            _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
        }
    }

    unsafe extern "system" fn host_window_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match message {
            WM_PAINT => {
                let mut paint = PAINTSTRUCT::default();
                let dc = unsafe { BeginPaint(hwnd, &mut paint) };
                let brush = unsafe { CreateSolidBrush(KEY_COLOR) };
                let rect = RECT {
                    left: 0,
                    top: 0,
                    right: WIDTH,
                    bottom: HEIGHT,
                };
                unsafe {
                    let _ = FillRect(dc, &rect, brush);
                    let _ = DeleteObject(brush.into());
                    let _ = EndPaint(hwnd, &paint);
                }
                LRESULT(0)
            }
            WM_CLOSE => {
                unsafe { DestroyWindow(hwnd) }.ok();
                LRESULT(0)
            }
            WM_DESTROY => {
                unsafe { PostQuitMessage(0) };
                LRESULT(0)
            }
            _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
        }
    }
}

#[cfg(windows)]
fn main() -> windows::core::Result<()> {
    windows_probe::run()
}
