use super::{GlassConfig, GlassError, GlassStatus};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
    ffi::c_void,
    mem::{size_of, size_of_val},
    sync::{Mutex, OnceLock},
};
use windows::{
    Foundation::{IPropertyValue, PropertyValue},
    Graphics::Effects::{
        IGraphicsEffect, IGraphicsEffect_Impl, IGraphicsEffectSource, IGraphicsEffectSource_Impl,
    },
    System::DispatcherQueueController,
    UI::{
        Color,
        Composition::{
            CompositionBackdropBrush, CompositionEffectBrush, CompositionEffectSourceParameter,
            Compositor, ContainerVisual, SpriteVisual, Visual,
        },
    },
    Win32::{
        Foundation::{E_INVALIDARG, HWND, LPARAM, LRESULT, WPARAM},
        Graphics::{
            Direct2D::CLSID_D2D1GaussianBlur,
            Dwm::{
                DWMWA_USE_HOSTBACKDROPBRUSH, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
                DwmSetWindowAttribute,
            },
            Gdi::{CreateRoundRectRgn, DeleteObject, HGDIOBJ, SetWindowRgn},
        },
        System::{
            LibraryLoader::GetModuleHandleW,
            WinRT::{
                Composition::ICompositorDesktopInterop,
                CreateDispatcherQueueController, DQTAT_COM_STA, DQTYPE_THREAD_CURRENT,
                DispatcherQueueOptions,
                Graphics::Direct2D::{
                    GRAPHICS_EFFECT_PROPERTY_MAPPING, GRAPHICS_EFFECT_PROPERTY_MAPPING_DIRECT,
                    GRAPHICS_EFFECT_PROPERTY_MAPPING_UNKNOWN, IGraphicsEffectD2D1Interop,
                    IGraphicsEffectD2D1Interop_Impl,
                },
                RO_INIT_SINGLETHREADED, RoInitialize, RoUninitialize,
            },
        },
        UI::{
            Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent},
            HiDpi::GetDpiForWindow,
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DestroyWindow, EVENT_SYSTEM_MINIMIZESTART,
                GetWindowRect, IsIconic, IsWindowVisible, IsZoomed, RegisterClassW, SW_HIDE,
                SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_SHOWWINDOW, SetWindowPos, ShowWindow,
                WINDOW_EX_STYLE, WINEVENT_OUTOFCONTEXT, WM_DESTROY, WNDCLASSW, WS_EX_NOACTIVATE,
                WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_POPUP,
            },
        },
    },
    core::{Error, HSTRING, Interface, PCWSTR, Result as WindowsResult, implement, w},
};
use windows_numerics::Vector2;

pub(super) fn set_rounded_corners(window: &impl HasWindowHandle) -> Result<(), GlassError> {
    let handle = window
        .window_handle()
        .map_err(|error| GlassError::Platform(error.to_string()))?;
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return Err(GlassError::UnsupportedWindowHandle);
    };
    let hwnd = HWND(handle.hwnd.get() as *mut c_void);
    set_rounded_corners_for_hwnd(hwnd)
}

fn set_rounded_corners_for_hwnd(hwnd: HWND) -> Result<(), GlassError> {
    let preference = DWMWCP_ROUND;
    // SAFETY: hwnd is a live top-level window and DwmSetWindowAttribute copies the fixed-size
    // corner-preference value during this call.
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &preference as *const _ as *const c_void,
            size_of_val(&preference) as u32,
        )
        .map_err(|error| platform_error_at("set rounded window corners", error))
    }?;
    sync_rounded_window_region(hwnd)
}

fn sync_rounded_window_region(hwnd: HWND) -> Result<(), GlassError> {
    let mut rect = windows::Win32::Foundation::RECT::default();
    // SAFETY: hwnd is a live top-level window and rect is valid for the synchronous call.
    unsafe { GetWindowRect(hwnd, &mut rect) }.map_err(platform_error)?;
    let maximized = unsafe { IsZoomed(hwnd).as_bool() };
    apply_window_region(
        hwnd,
        rect.right - rect.left,
        rect.bottom - rect.top,
        maximized,
    )
}

fn apply_window_region(
    hwnd: HWND,
    width: i32,
    height: i32,
    maximized: bool,
) -> Result<(), GlassError> {
    if maximized {
        // SAFETY: a null region restores the complete rectangular window while maximized.
        if unsafe { SetWindowRgn(hwnd, None, true) } == 0 {
            return Err(platform_error(Error::from_thread()));
        }
        return Ok(());
    }

    // Windows 11's standard top-level corner radius is 8 device-independent pixels.
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
    let radius = (8_i32 * dpi as i32 + 48) / 96;
    // CreateRoundRectRgn excludes the lower and right edges, hence the inclusive +1 bounds.
    let region = unsafe { CreateRoundRectRgn(0, 0, width + 1, height + 1, radius * 2, radius * 2) };
    if region.0.is_null() {
        return Err(platform_error(Error::from_thread()));
    }
    // On success Windows owns the region. On failure it remains ours and must be deleted.
    if unsafe { SetWindowRgn(hwnd, Some(region), true) } == 0 {
        let _ = unsafe { DeleteObject(HGDIOBJ(region.0)) };
        return Err(platform_error(Error::from_thread()));
    }
    Ok(())
}

pub(super) struct Backend {
    host_hwnd: HWND,
    backdrop: BackdropWindow,
    minimize_hook: MinimizeHook,
    _queue: DispatcherQueueController,
    compositor: Compositor,
    _target: windows::UI::Composition::Desktop::DesktopWindowTarget,
    _root: ContainerVisual,
    blur_visual: SpriteVisual,
    tint_visual: SpriteVisual,
    _host_backdrop: CompositionBackdropBrush,
    _blur_brush: CompositionEffectBrush,
    blur_radius: f32,
    backdrop_region_state: Option<(i32, i32, bool)>,
    initialized_winrt: bool,
}

impl Backend {
    pub(super) fn attach(
        window: &impl HasWindowHandle,
        config: GlassConfig,
    ) -> Result<Self, GlassError> {
        let handle = window
            .window_handle()
            .map_err(|error| GlassError::Platform(error.to_string()))?;
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return Err(GlassError::UnsupportedWindowHandle);
        };
        let host_hwnd = HWND(handle.hwnd.get() as *mut c_void);

        // SAFETY: this is called on the owning UI thread and balanced in Drop.
        unsafe { RoInitialize(RO_INIT_SINGLETHREADED) }.map_err(platform_error)?;
        let initialized_winrt = true;

        let result = Self::attach_initialized(host_hwnd, config, initialized_winrt);
        if result.is_err() {
            // SAFETY: balances the successful RoInitialize call above.
            unsafe { RoUninitialize() };
        }
        result
    }

    fn attach_initialized(
        host_hwnd: HWND,
        config: GlassConfig,
        initialized_winrt: bool,
    ) -> Result<Self, GlassError> {
        let options = DispatcherQueueOptions {
            dwSize: size_of::<DispatcherQueueOptions>() as u32,
            threadType: DQTYPE_THREAD_CURRENT,
            apartmentType: DQTAT_COM_STA,
        };
        // SAFETY: the options request a queue on the current UI thread and the controller is
        // retained for the complete compositor lifetime.
        let queue = unsafe { CreateDispatcherQueueController(options) }
            .map_err(|error| platform_error_at("create dispatcher queue", error))?;

        let backdrop = BackdropWindow::create()?;
        let minimize_hook = MinimizeHook::install(host_hwnd, backdrop.hwnd)?;
        enable_host_backdrop(backdrop.hwnd, true)?;
        let compositor =
            Compositor::new().map_err(|error| platform_error_at("create compositor", error))?;
        let interop: ICompositorDesktopInterop = compositor
            .cast()
            .map_err(|error| platform_error_at("query compositor desktop interop", error))?;
        // SAFETY: the package owns backdrop_hwnd for the complete target lifetime. The target
        // is topmost inside the companion window, which contains no application controls.
        let target = unsafe { interop.CreateDesktopWindowTarget(backdrop.hwnd, true) }
            .map_err(|error| platform_error_at("create desktop window target", error))?;

        let root = compositor.CreateContainerVisual().map_err(platform_error)?;
        root.SetRelativeSizeAdjustment(Vector2 { X: 1.0, Y: 1.0 })
            .map_err(platform_error)?;

        let host_backdrop = compositor
            .CreateHostBackdropBrush()
            .map_err(|error| platform_error_at("create host backdrop brush", error))?;
        let blur_brush = create_blur_brush(&compositor, &host_backdrop, config.blur_radius)?;
        let blur_visual = compositor.CreateSpriteVisual().map_err(platform_error)?;
        blur_visual
            .SetRelativeSizeAdjustment(Vector2 { X: 1.0, Y: 1.0 })
            .map_err(platform_error)?;
        blur_visual.SetBrush(&blur_brush).map_err(platform_error)?;

        let tint_visual = compositor.CreateSpriteVisual().map_err(platform_error)?;
        tint_visual
            .SetRelativeSizeAdjustment(Vector2 { X: 1.0, Y: 1.0 })
            .map_err(platform_error)?;
        let tint = compositor
            .CreateColorBrushWithColor(tint_color(config, true))
            .map_err(platform_error)?;
        tint_visual.SetBrush(&tint).map_err(platform_error)?;

        let children = root.Children().map_err(platform_error)?;
        children.InsertAtTop(&blur_visual).map_err(platform_error)?;
        children.InsertAtTop(&tint_visual).map_err(platform_error)?;
        target.SetRoot(&root).map_err(platform_error)?;

        Ok(Self {
            host_hwnd,
            backdrop,
            minimize_hook,
            _queue: queue,
            compositor,
            _target: target,
            _root: root,
            blur_visual,
            tint_visual,
            _host_backdrop: host_backdrop,
            _blur_brush: blur_brush,
            blur_radius: config.blur_radius,
            backdrop_region_state: None,
            initialized_winrt,
        })
    }

    pub(super) fn update(&mut self, config: GlassConfig, active: bool) -> Result<(), GlassError> {
        self.sync_backdrop_window()?;
        if (config.blur_radius - self.blur_radius).abs() > f32::EPSILON {
            let brush =
                create_blur_brush(&self.compositor, &self._host_backdrop, config.blur_radius)?;
            self.blur_visual.SetBrush(&brush).map_err(platform_error)?;
            self._blur_brush = brush;
            self.blur_radius = config.blur_radius;
        }

        let tint = self
            .compositor
            .CreateColorBrushWithColor(tint_color(config, active))
            .map_err(platform_error)?;
        self.tint_visual.SetBrush(&tint).map_err(platform_error)?;
        Ok(())
    }

    pub(super) fn status(&self) -> GlassStatus {
        GlassStatus::Active
    }

    fn sync_backdrop_window(&mut self) -> Result<(), GlassError> {
        // SAFETY: host_hwnd is owned by the caller for Backend's lifetime.
        if unsafe {
            !IsWindowVisible(self.host_hwnd).as_bool() || IsIconic(self.host_hwnd).as_bool()
        } {
            // SAFETY: hiding our non-activating package-owned window has no external side effect.
            let _ = unsafe { ShowWindow(self.backdrop.hwnd, SW_HIDE) };
            return Ok(());
        }
        let mut rect = windows::Win32::Foundation::RECT::default();
        // SAFETY: both HWND values remain live for Backend's lifetime. Positioning the companion
        // immediately after the host keeps it directly behind the app without activation.
        unsafe {
            GetWindowRect(self.host_hwnd, &mut rect).map_err(platform_error)?;
            SetWindowPos(
                self.backdrop.hwnd,
                Some(self.host_hwnd),
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            )
            .map_err(platform_error)?;
        }
        let state = (rect.right - rect.left, rect.bottom - rect.top, unsafe {
            IsZoomed(self.host_hwnd).as_bool()
        });
        if self.backdrop_region_state != Some(state) {
            apply_window_region(self.backdrop.hwnd, state.0, state.1, state.2)?;
            self.backdrop_region_state = Some(state);
        }
        Ok(())
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        self.minimize_hook.uninstall();
        let _ = self._target.SetRoot(None::<&Visual>);
        let _ = enable_host_backdrop(self.backdrop.hwnd, false);
        if self.initialized_winrt {
            // SAFETY: Backend is !Send through AeroGlass and drops on the thread that called
            // RoInitialize successfully.
            unsafe { RoUninitialize() };
        }
    }
}

#[derive(Clone, Copy)]
struct MinimizeRegistration {
    host: usize,
    backdrop: usize,
}

static MINIMIZE_REGISTRATIONS: Mutex<Vec<MinimizeRegistration>> = Mutex::new(Vec::new());

struct MinimizeHook {
    handle: Option<HWINEVENTHOOK>,
    registration: MinimizeRegistration,
}

impl MinimizeHook {
    fn install(host: HWND, backdrop: HWND) -> Result<Self, GlassError> {
        let registration = MinimizeRegistration {
            host: host.0 as usize,
            backdrop: backdrop.0 as usize,
        };
        minimize_registrations().push(registration);

        // Restrict the hook to the host's UI thread. Out-of-context callbacks are delivered on
        // this thread's message loop, so the companion window remains thread-affine.
        let thread_id = unsafe {
            windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(host, None)
        };
        let handle = unsafe {
            SetWinEventHook(
                EVENT_SYSTEM_MINIMIZESTART,
                EVENT_SYSTEM_MINIMIZESTART,
                None,
                Some(minimize_event),
                0,
                thread_id,
                WINEVENT_OUTOFCONTEXT,
            )
        };
        if handle.is_invalid() {
            remove_minimize_registration(registration);
            return Err(platform_error_at(
                "listen for host window minimization",
                Error::from_thread(),
            ));
        }

        Ok(Self {
            handle: Some(handle),
            registration,
        })
    }

    fn uninstall(&mut self) {
        if let Some(handle) = self.handle.take() {
            // SAFETY: this hook was created by install and is unhooked exactly once.
            let _ = unsafe { UnhookWinEvent(handle) };
            remove_minimize_registration(self.registration);
        }
    }
}

impl Drop for MinimizeHook {
    fn drop(&mut self) {
        self.uninstall();
    }
}

fn minimize_registrations() -> std::sync::MutexGuard<'static, Vec<MinimizeRegistration>> {
    MINIMIZE_REGISTRATIONS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn remove_minimize_registration(registration: MinimizeRegistration) {
    minimize_registrations()
        .retain(|entry| entry.host != registration.host || entry.backdrop != registration.backdrop);
}

unsafe extern "system" fn minimize_event(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    _object_id: i32,
    _child_id: i32,
    _event_thread: u32,
    _event_time: u32,
) {
    if event != EVENT_SYSTEM_MINIMIZESTART {
        return;
    }
    let host = hwnd.0 as usize;
    let backdrop = minimize_registrations()
        .iter()
        .find(|entry| entry.host == host)
        .map(|entry| entry.backdrop);
    if let Some(backdrop) = backdrop {
        // SAFETY: the registered companion HWND remains live until its hook is uninstalled.
        let _ = unsafe { ShowWindow(HWND(backdrop as *mut c_void), SW_HIDE) };
    }
}

struct BackdropWindow {
    hwnd: HWND,
}

impl BackdropWindow {
    fn create() -> Result<Self, GlassError> {
        static REGISTERED: OnceLock<Result<(), String>> = OnceLock::new();
        // SAFETY: None requests the module handle for the current executable.
        let instance = unsafe { GetModuleHandleW(None) }.map_err(platform_error)?;
        let registration = REGISTERED.get_or_init(|| {
            let class = WNDCLASSW {
                lpfnWndProc: Some(backdrop_window_proc),
                hInstance: instance.into(),
                lpszClassName: w!("AeroGlassBackdropWindow"),
                ..Default::default()
            };
            // SAFETY: class points to static strings and a process-lifetime window procedure.
            if unsafe { RegisterClassW(&class) } == 0 {
                Err(Error::from_thread().to_string())
            } else {
                Ok(())
            }
        });
        registration
            .as_ref()
            .map_err(|message| GlassError::Platform(message.clone()))?;

        let ex_style =
            WINDOW_EX_STYLE(WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0 | WS_EX_TRANSPARENT.0);
        // SAFETY: the class is registered above. This must remain unowned: Windows always keeps an
        // owned popup above its owner, while the material window must sit behind the host.
        let hwnd = unsafe {
            CreateWindowExW(
                ex_style,
                w!("AeroGlassBackdropWindow"),
                w!(""),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                Some(instance.into()),
                None,
            )
        }
        .map_err(platform_error)?;
        // Match the companion material to the host's DWM-clipped outline. Corner preference is
        // cosmetic, so unsupported Windows versions must not disable the glass fallback.
        let _ = set_rounded_corners_for_hwnd(hwnd);
        // SAFETY: showing without activation cannot steal focus from the host.
        let _ = unsafe { ShowWindow(hwnd, SW_SHOWNOACTIVATE) };
        Ok(Self { hwnd })
    }
}

impl Drop for BackdropWindow {
    fn drop(&mut self) {
        // SAFETY: hwnd was created and is exclusively owned by this value.
        let _ = unsafe { DestroyWindow(self.hwnd) };
    }
}

unsafe extern "system" fn backdrop_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_DESTROY {
        return LRESULT(0);
    }
    // SAFETY: unhandled messages are delegated to the system default procedure with the exact
    // parameters received from Windows.
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

fn tint_color(config: GlassConfig, active: bool) -> Color {
    Color {
        A: config.tint_opacity(active),
        R: config.tint.red,
        G: config.tint.green,
        B: config.tint.blue,
    }
}

fn create_blur_brush(
    compositor: &Compositor,
    backdrop: &CompositionBackdropBrush,
    radius: f32,
) -> Result<CompositionEffectBrush, GlassError> {
    let source_name = HSTRING::from("backdrop");
    let parameter = CompositionEffectSourceParameter::Create(&source_name)
        .map_err(|error| platform_error_at("create backdrop effect parameter", error))?;
    let source: IGraphicsEffectSource = parameter
        .cast()
        .map_err(|error| platform_error_at("cast backdrop effect parameter", error))?;
    let effect: IGraphicsEffect = GaussianBlurEffect::new(radius, source).into();
    let factory = compositor
        .CreateEffectFactory(&effect)
        .map_err(|error| platform_error_at("create Gaussian blur effect factory", error))?;
    let brush = factory
        .CreateBrush()
        .map_err(|error| platform_error_at("create Gaussian blur effect brush", error))?;
    brush
        .SetSourceParameter(&source_name, backdrop)
        .map_err(|error| platform_error_at("bind host backdrop to blur effect", error))?;
    Ok(brush)
}

fn enable_host_backdrop(hwnd: HWND, enabled: bool) -> Result<(), GlassError> {
    let value = i32::from(enabled);
    // SAFETY: hwnd is a live top-level window. DwmSetWindowAttribute copies the fixed-size
    // BOOL value during the call.
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_HOSTBACKDROPBRUSH,
            (&raw const value).cast(),
            size_of::<i32>() as u32,
        )
    }
    .map_err(platform_error)
}

fn platform_error(error: Error) -> GlassError {
    GlassError::Platform(error.to_string())
}

fn platform_error_at(stage: &str, error: Error) -> GlassError {
    GlassError::Platform(format!("{stage}: {error}"))
}

#[implement(IGraphicsEffect, IGraphicsEffectSource, IGraphicsEffectD2D1Interop)]
struct GaussianBlurEffect {
    name: Mutex<HSTRING>,
    radius: f32,
    source: IGraphicsEffectSource,
}

impl GaussianBlurEffect {
    fn new(radius: f32, source: IGraphicsEffectSource) -> Self {
        Self {
            name: Mutex::new(HSTRING::from("GaussianBlurEffect")),
            radius,
            source,
        }
    }
}

impl IGraphicsEffectSource_Impl for GaussianBlurEffect_Impl {}

impl IGraphicsEffect_Impl for GaussianBlurEffect_Impl {
    fn Name(&self) -> WindowsResult<HSTRING> {
        Ok(self.name.lock().expect("effect name poisoned").clone())
    }

    fn SetName(&self, name: &HSTRING) -> WindowsResult<()> {
        *self.name.lock().expect("effect name poisoned") = name.clone();
        Ok(())
    }
}

impl IGraphicsEffectD2D1Interop_Impl for GaussianBlurEffect_Impl {
    fn GetEffectId(&self) -> WindowsResult<windows::core::GUID> {
        Ok(CLSID_D2D1GaussianBlur)
    }

    fn GetNamedPropertyMapping(
        &self,
        name: &PCWSTR,
        index: *mut u32,
        mapping: *mut GRAPHICS_EFFECT_PROPERTY_MAPPING,
    ) -> WindowsResult<()> {
        // SAFETY: the compositor supplies a valid null-terminated property name and valid
        // output pointers for the duration of this synchronous call.
        let property =
            unsafe { name.to_string() }.map_err(|_| Error::from_hresult(E_INVALIDARG))?;
        if index.is_null() || mapping.is_null() {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        // SAFETY: pointers were checked above and are owned by the compositor call.
        unsafe {
            let property_index = match property.as_str() {
                "BlurAmount" => Some(0),
                "Optimization" => Some(1),
                "BorderMode" => Some(2),
                _ => None,
            };
            if let Some(property_index) = property_index {
                index.write(property_index);
                mapping.write(GRAPHICS_EFFECT_PROPERTY_MAPPING_DIRECT);
            } else {
                // Windows Composition probes effect metadata beyond the public property list.
                // Unknown names are reported through the documented sentinel values; returning
                // E_INVALIDARG here causes CreateEffectFactory to reject the entire graph.
                index.write(u32::MAX);
                mapping.write(GRAPHICS_EFFECT_PROPERTY_MAPPING_UNKNOWN);
            }
        }
        Ok(())
    }

    fn GetPropertyCount(&self) -> WindowsResult<u32> {
        // Gaussian blur's native Direct2D effect has three properties. Composition validates
        // all three even though this module only exposes blur radius to callers.
        Ok(3)
    }

    fn GetProperty(&self, index: u32) -> WindowsResult<IPropertyValue> {
        match index {
            0 => PropertyValue::CreateSingle(self.radius)?.cast(),
            // D2D1_GAUSSIANBLUR_OPTIMIZATION_BALANCED
            1 => PropertyValue::CreateUInt32(1)?.cast(),
            // D2D1_BORDER_MODE_HARD avoids transparent edges around the material.
            2 => PropertyValue::CreateUInt32(1)?.cast(),
            _ => Err(Error::from_hresult(E_INVALIDARG)),
        }
    }

    fn GetSource(&self, index: u32) -> WindowsResult<IGraphicsEffectSource> {
        if index == 0 {
            Ok(self.source.clone())
        } else {
            Err(Error::from_hresult(E_INVALIDARG))
        }
    }

    fn GetSourceCount(&self) -> WindowsResult<u32> {
        Ok(1)
    }
}
