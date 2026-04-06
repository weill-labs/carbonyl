use std::any::Any;
use std::ffi::{CStr, CString};
use std::io::Write;
use std::panic::{self, AssertUnwindSafe};
use std::process::{Command, Stdio};
use std::sync::{mpsc, Mutex, MutexGuard};
use std::{env, io, thread};

use libc::{c_char, c_float, c_int, c_uchar, c_uint, c_void, size_t};

use crate::cli::{CommandLine, CommandLineProgram, EnvVar};
use crate::gfx::{Cast, Color, Point, Rect, Size};
use crate::output::{RenderThread, Window};
use crate::ui::navigation::NavigationAction;
use crate::{input, utils::log};

#[repr(C)]
#[derive(Copy, Clone)]
pub struct CSize {
    width: c_uint,
    height: c_uint,
}
#[repr(C)]
#[derive(Copy, Clone)]
pub struct CPoint {
    x: c_uint,
    y: c_uint,
}
#[repr(C)]
#[derive(Copy, Clone)]
pub struct CRect {
    origin: CPoint,
    size: CSize,
}
#[repr(C)]
#[derive(Copy, Clone)]
pub struct CColor {
    r: u8,
    g: u8,
    b: u8,
}
#[repr(C)]
#[derive(Copy, Clone)]
pub struct CText {
    text: *const c_char,
    rect: CRect,
    color: CColor,
}

#[repr(C)]
pub struct RendererBridge {
    window: Window,
    renderer: RenderThread,
}

unsafe impl Send for RendererBridge {}
unsafe impl Sync for RendererBridge {}

pub type RendererPtr = *const Mutex<RendererBridge>;

fn panic_message(payload: Box<dyn Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "unknown panic".to_owned()
    }
}

fn ffi_boundary<R, F>(name: &str, default: R, run: F) -> R
where
    F: FnOnce() -> R,
{
    match panic::catch_unwind(AssertUnwindSafe(run)) {
        Ok(value) => value,
        Err(payload) => {
            log::error!("{name} panicked: {}", panic_message(payload));
            default
        }
    }
}

fn get_bridge<'a>(bridge: RendererPtr, name: &str) -> Option<&'a Mutex<RendererBridge>> {
    if bridge.is_null() {
        log::error!("{name} called with null renderer bridge");
        return None;
    }

    unsafe { bridge.as_ref() }
}

fn lock_bridge<'a>(
    bridge: &'a Mutex<RendererBridge>,
    name: &str,
) -> MutexGuard<'a, RendererBridge> {
    match bridge.lock() {
        Ok(bridge) => bridge,
        Err(error) => {
            log::error!("{name} recovered a poisoned renderer mutex");
            error.into_inner()
        }
    }
}

fn read_c_string(ptr: *const c_char, context: &str) -> Option<String> {
    if ptr.is_null() {
        log::error!("{context} called with null string");
        return None;
    }

    let value = unsafe { CStr::from_ptr(ptr) };

    match value.to_str() {
        Ok(value) => Some(value.to_owned()),
        Err(error) => {
            log::error!("{context} received invalid UTF-8: {error}");
            Some(value.to_string_lossy().into_owned())
        }
    }
}

impl<T: Copy> From<CPoint> for Point<T>
where
    c_uint: Cast<T>,
{
    fn from(value: CPoint) -> Self {
        Point::new(value.x, value.y).cast()
    }
}
impl From<Size<c_uint>> for CSize {
    fn from(value: Size<c_uint>) -> Self {
        Self {
            width: value.width,
            height: value.height,
        }
    }
}
impl<T: Copy> From<CSize> for Size<T>
where
    c_uint: Cast<T>,
{
    fn from(value: CSize) -> Self {
        Size::new(value.width, value.height).cast()
    }
}
impl From<CColor> for Color {
    fn from(value: CColor) -> Self {
        Color::new(value.r, value.g, value.b)
    }
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct BrowserDelegate {
    shutdown: extern "C" fn(),
    refresh: extern "C" fn(),
    go_to: extern "C" fn(*const c_char),
    go_back: extern "C" fn(),
    go_forward: extern "C" fn(),
    scroll: extern "C" fn(c_int),
    key_press: extern "C" fn(c_char),
    mouse_up: extern "C" fn(c_uint, c_uint),
    mouse_down: extern "C" fn(c_uint, c_uint),
    mouse_move: extern "C" fn(c_uint, c_uint),
    post_task: extern "C" fn(extern "C" fn(*mut c_void), *mut c_void),
}

fn main() -> io::Result<Option<i32>> {
    let cmd = match CommandLineProgram::parse_or_run() {
        None => return Ok(Some(0)),
        Some(cmd) => cmd,
    };

    if cmd.shell_mode {
        return Ok(None);
    }

    let mut terminal = input::Terminal::setup();
    let mut command = Command::new(env::current_exe()?);

    if !cmd.bitmap {
        command
            .arg("--disable-threaded-scrolling")
            .arg("--disable-threaded-animation");
    }

    let output = command
        .args(cmd.args)
        .env(EnvVar::ShellMode, "1")
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::piped())
        .output()?;

    terminal.teardown();

    let code = output.status.code().unwrap_or(127);

    if code != 0 || cmd.debug {
        io::stderr().write_all(&output.stderr)?;
    }

    Ok(Some(code))
}

#[no_mangle]
pub extern "C" fn carbonyl_bridge_main() {
    ffi_boundary("carbonyl_bridge_main", (), || match main() {
        Ok(Some(code)) => std::process::exit(code),
        Ok(None) => (),
        Err(error) => {
            let _ = writeln!(io::stderr(), "carbonyl: fatal error: {error}");
            std::process::exit(1);
        }
    });
}

#[no_mangle]
pub extern "C" fn carbonyl_bridge_bitmap_mode() -> bool {
    ffi_boundary("carbonyl_bridge_bitmap_mode", false, || {
        CommandLine::parse().bitmap
    })
}

#[no_mangle]
pub extern "C" fn carbonyl_bridge_get_dpi() -> c_float {
    ffi_boundary("carbonyl_bridge_get_dpi", 1.0, || Window::read().dpi)
}

#[no_mangle]
pub extern "C" fn carbonyl_renderer_create() -> RendererPtr {
    ffi_boundary("carbonyl_renderer_create", std::ptr::null(), || {
        let bridge = RendererBridge {
            window: Window::read(),
            renderer: RenderThread::new(),
        };

        Box::into_raw(Box::new(Mutex::new(bridge)))
    })
}

#[no_mangle]
pub extern "C" fn carbonyl_renderer_start(bridge: RendererPtr) {
    ffi_boundary("carbonyl_renderer_start", (), || {
        {
            let Some(bridge) = get_bridge(bridge, "carbonyl_renderer_start") else {
                return;
            };
            let mut bridge = lock_bridge(bridge, "carbonyl_renderer_start");

            bridge.renderer.enable()
        }

        carbonyl_renderer_resize(bridge);
    });
}

#[no_mangle]
pub extern "C" fn carbonyl_renderer_resize(bridge: RendererPtr) {
    ffi_boundary("carbonyl_renderer_resize", (), || {
        let Some(bridge) = get_bridge(bridge, "carbonyl_renderer_resize") else {
            return;
        };
        let mut bridge = lock_bridge(bridge, "carbonyl_renderer_resize");
        let window = bridge.window.update();
        let cells = window.cells.clone();

        log::debug!("resizing renderer, terminal window: {:?}", window);

        bridge
            .renderer
            .render(move |renderer| renderer.set_size(cells));
    });
}

#[no_mangle]
pub extern "C" fn carbonyl_renderer_push_nav(
    bridge: RendererPtr,
    url: *const c_char,
    can_go_back: bool,
    can_go_forward: bool,
) {
    ffi_boundary("carbonyl_renderer_push_nav", (), || {
        let Some(bridge) = get_bridge(bridge, "carbonyl_renderer_push_nav") else {
            return;
        };
        let Some(url) = read_c_string(url, "carbonyl_renderer_push_nav") else {
            return;
        };
        let mut bridge = lock_bridge(bridge, "carbonyl_renderer_push_nav");

        bridge.renderer.render(move |renderer| {
            renderer.push_nav(&url, can_go_back, can_go_forward)
        });
    });
}

#[no_mangle]
pub extern "C" fn carbonyl_renderer_set_title(bridge: RendererPtr, title: *const c_char) {
    ffi_boundary("carbonyl_renderer_set_title", (), || {
        let Some(bridge) = get_bridge(bridge, "carbonyl_renderer_set_title") else {
            return;
        };
        let Some(title) = read_c_string(title, "carbonyl_renderer_set_title") else {
            return;
        };
        let mut bridge = lock_bridge(bridge, "carbonyl_renderer_set_title");

        bridge.renderer.render(move |renderer| {
            if let Err(error) = renderer.set_title(&title) {
                log::error!("failed to set title: {error}");
            }
        });
    });
}

#[no_mangle]
pub extern "C" fn carbonyl_renderer_draw_text(
    bridge: RendererPtr,
    text: *const CText,
    text_size: size_t,
) {
    ffi_boundary("carbonyl_renderer_draw_text", (), || {
        let Some(bridge) = get_bridge(bridge, "carbonyl_renderer_draw_text") else {
            return;
        };
        if text.is_null() && text_size != 0 {
            log::error!("carbonyl_renderer_draw_text called with null text buffer");
            return;
        }

        let text = if text_size == 0 {
            &[][..]
        } else {
            unsafe { std::slice::from_raw_parts(text, text_size) }
        };
        let mut bridge = lock_bridge(bridge, "carbonyl_renderer_draw_text");
        let mut vec = text
            .iter()
            .filter_map(|text| {
                let Some(value) = read_c_string(text.text, "carbonyl_renderer_draw_text") else {
                    return None;
                };

                Some((
                    value,
                    text.rect.origin.into(),
                    text.rect.size.into(),
                    text.color.into(),
                ))
            })
            .collect::<Vec<(String, Point, Size, Color)>>();

        bridge.renderer.render(move |renderer| {
            renderer.clear_text();
            let viewport = renderer.get_size();
            let viewport_width = viewport.width.saturating_mul(2);
            let viewport_height = viewport.height.saturating_sub(1).saturating_mul(4);

            for (text, origin, size, color) in std::mem::take(&mut vec) {
                if text.is_empty() {
                    let full_viewport_fill = origin.x == 0
                        && origin.y == 0
                        && size.width.saturating_add(2) >= viewport_width
                        && size.height.saturating_add(4) >= viewport_height;

                    if full_viewport_fill {
                        renderer.fill_rect(Rect { origin, size }, color);
                    }
                } else {
                    renderer.draw_text(&text, origin, size, color);
                }
            }
        });
    });
}

#[derive(Clone, Copy)]
struct CallbackData(*const c_void);

impl CallbackData {
    pub fn as_ptr(&self) -> *const c_void {
        self.0
    }
}

unsafe impl Send for CallbackData {}
unsafe impl Sync for CallbackData {}

#[no_mangle]
pub extern "C" fn carbonyl_renderer_draw_bitmap(
    bridge: RendererPtr,
    pixels: *const c_uchar,
    pixels_size: CSize,
    rect: CRect,
    callback: extern "C" fn(*const c_void),
    callback_data: *const c_void,
) {
    ffi_boundary("carbonyl_renderer_draw_bitmap", (), || {
        let callback_data = CallbackData(callback_data);
        let Some(bridge) = get_bridge(bridge, "carbonyl_renderer_draw_bitmap") else {
            callback(callback_data.as_ptr());
            return;
        };
        let Some(length) = (pixels_size.width as usize)
            .checked_mul(pixels_size.height as usize)
            .and_then(|length| length.checked_mul(4))
        else {
            log::error!("carbonyl_renderer_draw_bitmap received oversized pixel buffer");
            callback(callback_data.as_ptr());
            return;
        };
        let pixels = if length == 0 {
            &[][..]
        } else {
            if pixels.is_null() {
                log::error!("carbonyl_renderer_draw_bitmap called with null pixel buffer");
                callback(callback_data.as_ptr());
                return;
            }

            unsafe { std::slice::from_raw_parts(pixels, length) }
        };
        let mut bridge = lock_bridge(bridge, "carbonyl_renderer_draw_bitmap");

        bridge.renderer.render(move |renderer| {
            renderer.draw_background(
                pixels,
                pixels_size.into(),
                Rect {
                    size: rect.size.into(),
                    origin: rect.origin.into(),
                },
            );

            callback(callback_data.as_ptr());
        });
    });
}

#[no_mangle]
pub extern "C" fn carbonyl_renderer_get_size(bridge: RendererPtr) -> CSize {
    ffi_boundary(
        "carbonyl_renderer_get_size",
        CSize {
            width: 0,
            height: 0,
        },
        || {
            let Some(bridge) = get_bridge(bridge, "carbonyl_renderer_get_size") else {
                return CSize {
                    width: 0,
                    height: 0,
                };
            };
            let bridge = lock_bridge(bridge, "carbonyl_renderer_get_size");

            log::debug!("terminal size: {:?}", bridge.window.browser);

            bridge.window.browser.into()
        },
    )
}

extern "C" fn post_task_handler(callback: *mut c_void) {
    let mut closure = unsafe { Box::from_raw(callback as *mut Box<dyn FnMut()>) };

    closure()
}

unsafe fn post_task<F>(handle: extern "C" fn(extern "C" fn(*mut c_void), *mut c_void), run: F)
where
    F: FnMut() + Send + 'static,
{
    let closure: *mut Box<dyn FnMut()> = Box::into_raw(Box::new(Box::new(run)));

    handle(post_task_handler, closure as *mut c_void);
}

/// Function called by the C++ code to start listening for input events.
///
/// This spawns a dedicated Rust input thread and returns immediately.
#[no_mangle]
pub extern "C" fn carbonyl_renderer_listen(bridge: RendererPtr, delegate: *mut BrowserDelegate) {
    ffi_boundary("carbonyl_renderer_listen", (), || {
        if bridge.is_null() {
            log::error!("carbonyl_renderer_listen called with null renderer bridge");
            return;
        }
        let bridge_ptr = bridge as usize;
        let Some(delegate) = (unsafe { delegate.as_ref() }).copied() else {
            log::error!("carbonyl_renderer_listen called with null browser delegate");
            return;
        };

        use input::*;

        thread::spawn(move || {
            let bridge = bridge_ptr as RendererPtr;
            macro_rules! emit {
                ($event:ident($($args:expr),*) => $closure:expr) => {{
                    let run = move || {
                        (delegate.$event)($($args),*);

                        $closure
                    };

                    unsafe { post_task(delegate.post_task, run) }
                }};
                ($event:ident($($args:expr),*)) => {{
                    emit!($event($($args),*) => {})
                }};
            }

            if let Err(error) = listen(|mut events| {
                let Some(bridge) = get_bridge(bridge, "carbonyl_renderer_listen") else {
                    return;
                };

                lock_bridge(bridge, "carbonyl_renderer_listen")
                    .renderer
                    .render(move |renderer| {
                        let get_scale = || {
                            get_bridge(bridge, "carbonyl_renderer_listen")
                                .map(|bridge| lock_bridge(bridge, "carbonyl_renderer_listen").window.scale)
                        };
                        let scale = |col, row| {
                            let Some(scale) = get_scale() else {
                                return (0, 0);
                            };

                            scale
                                .mul(((col as f32 + 0.5), (row as f32 - 0.5)))
                                .floor()
                                .cast()
                                .into()
                        };
                        let dispatch = |action| match action {
                            NavigationAction::Ignore => false,
                            NavigationAction::Forward => true,
                            NavigationAction::GoBack() => {
                                emit!(go_back());
                                false
                            }
                            NavigationAction::GoForward() => {
                                emit!(go_forward());
                                false
                            }
                            NavigationAction::Refresh() => {
                                emit!(refresh());
                                false
                            }
                            NavigationAction::GoTo(url) => {
                                match CString::new(url) {
                                    Ok(c_str) => emit!(go_to(c_str.as_ptr())),
                                    Err(error) => {
                                        log::error!("failed to encode navigation URL: {error}");
                                    }
                                }
                                false
                            }
                        };

                        for event in std::mem::take(&mut events) {
                            use Event::*;

                            match event {
                                Exit => (),
                                Scroll { delta } => {
                                    let Some(scale) = get_scale() else {
                                        continue;
                                    };

                                    emit!(scroll((delta as f32 * scale.height) as c_int))
                                }
                                KeyPress { key } => match renderer.keypress(&key) {
                                    Ok(action) => {
                                        if dispatch(action) {
                                            emit!(key_press(key.char as c_char))
                                        }
                                    }
                                    Err(error) => log::error!("keypress handling failed: {error}"),
                                },
                                MouseUp { col, row } => {
                                    match renderer.mouse_up((col as _, row as _).into()) {
                                        Ok(action) => {
                                            if dispatch(action) {
                                                let (width, height) = scale(col, row);

                                                emit!(mouse_up(width, height))
                                            }
                                        }
                                        Err(error) => {
                                            log::error!("mouse up handling failed: {error}")
                                        }
                                    }
                                }
                                MouseDown { col, row } => {
                                    match renderer.mouse_down((col as _, row as _).into()) {
                                        Ok(action) => {
                                            if dispatch(action) {
                                                let (width, height) = scale(col, row);

                                                emit!(mouse_down(width, height))
                                            }
                                        }
                                        Err(error) => {
                                            log::error!("mouse down handling failed: {error}")
                                        }
                                    }
                                }
                                MouseMove { col, row } => {
                                    match renderer.mouse_move((col as _, row as _).into()) {
                                        Ok(action) => {
                                            if dispatch(action) {
                                                let (width, height) = scale(col, row);

                                                emit!(mouse_move(width, height))
                                            }
                                        }
                                        Err(error) => {
                                            log::error!("mouse move handling failed: {error}")
                                        }
                                    }
                                }
                                Terminal(terminal) => match terminal {
                                    TerminalEvent::Name(name) => {
                                        log::debug!("terminal name: {name}")
                                    }
                                    TerminalEvent::TrueColorSupported => renderer.enable_true_color(),
                                },
                            }
                        }
                    })
            }) {
                log::error!("input listener failed: {error}");
            }

            let (tx, rx) = mpsc::channel();

            emit!(shutdown() => {
                if tx.send(()).is_err() {
                    log::error!("failed to signal renderer shutdown");
                }
            });
            let _ = rx.recv();
        });
    });
}
