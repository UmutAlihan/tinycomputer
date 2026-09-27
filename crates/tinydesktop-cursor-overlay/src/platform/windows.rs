//! Windows: a layered, topmost, click-through, non-activating tool window
//! (no taskbar button, never focused) whose pixels are set with
//! `UpdateLayeredWindow` from premultiplied BGRA frames of the sprite.
//!
//! A `WM_TIMER` at display rate ticks the driver on the window's thread.

// Every call here is Win32 foreign-function work; each `unsafe` block says
// why it is sound.
#![allow(unsafe_code)]

use std::ffi::c_void;
use std::ptr::{null, null_mut};

use tinydesktop_cursor::sprite::Sprite;
use windows_sys::Win32::Foundation::{HWND, POINT, SIZE};
use windows_sys::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
    CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, GetDC, HBITMAP, HDC, SelectObject,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, MSG, RegisterClassW,
    SW_HIDE, SW_SHOWNOACTIVATE, SetTimer, ShowWindow, TranslateMessage, ULW_ALPHA,
    UpdateLayeredWindow, WM_TIMER, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

use crate::driver::{Driver, Picture, Tick};

/// Milliseconds between redraws: a display's refresh.
const FRAME_MS: u32 = 16;

/// Pixels per point the sprite is drawn at. The helper is not DPI-aware, so
/// Windows scales the window with the display, keeping it in the same
/// logical units the module's coordinates use.
const SCALE: u32 = 1;

/// One sprite frame, ready to hand to `UpdateLayeredWindow`.
struct Bitmap {
    dc: HDC,
}

struct Overlay {
    window: HWND,
    screen: HDC,
    frames: Vec<Bitmap>,
    side: i32,
    shown: Option<Picture>,
}

/// A wide, NUL-terminated string for Win32.
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

impl Overlay {
    fn new() -> Option<Self> {
        let class = wide("TinydesktopCursorOverlay");
        // SAFETY: a null module name asks for this executable's own handle,
        // which lives as long as the process.
        let instance = unsafe { GetModuleHandleW(null()) };
        let class_info = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(DefWindowProcW),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: null_mut(),
            hCursor: null_mut(),
            hbrBackground: null_mut(),
            lpszMenuName: null(),
            lpszClassName: class.as_ptr(),
        };
        // SAFETY: `class_info` is fully initialized and `class` outlives the
        // call (Windows copies the name).
        if unsafe { RegisterClassW(&class_info) } == 0 {
            return None;
        }
        let sprite = Sprite::render(SCALE);
        let side = i32::try_from(sprite.pixels()).ok()?;
        // SAFETY: the class was registered above; every pointer argument is
        // either valid for the call or null where Win32 allows it.
        let window = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW
                    | WS_EX_NOACTIVATE,
                class.as_ptr(),
                class.as_ptr(),
                WS_POPUP,
                0,
                0,
                side,
                side,
                null_mut(),
                null_mut(),
                instance,
                null(),
            )
        };
        if window.is_null() {
            return None;
        }
        // SAFETY: a null window asks for the screen's device context, which
        // this process keeps for its whole life.
        let screen = unsafe { GetDC(null_mut()) };
        let frames = (0..sprite.frames.len())
            .map(|index| bitmap(screen, side, &sprite.premultiplied_bgra(index)))
            .collect::<Option<Vec<_>>>()?;
        Some(Self {
            window,
            screen,
            frames,
            side,
            shown: None,
        })
    }

    fn show(&mut self, picture: Picture) {
        if self.shown == Some(picture) {
            return;
        }
        let Some(frame) = self.frames.get(picture.sprite_frame) else {
            return;
        };
        let origin = POINT {
            x: round(picture.origin.x),
            y: round(picture.origin.y),
        };
        let size = SIZE {
            cx: self.side,
            cy: self.side,
        };
        let source = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION {
            BlendOp: u8::try_from(AC_SRC_OVER).unwrap_or(0),
            BlendFlags: 0,
            SourceConstantAlpha: opacity(picture.opacity),
            AlphaFormat: u8::try_from(AC_SRC_ALPHA).unwrap_or(1),
        };
        // SAFETY: the window and device contexts were created by this
        // overlay and live as long as it; every pointer is to a local that
        // outlives the call.
        unsafe {
            UpdateLayeredWindow(
                self.window,
                self.screen,
                &origin,
                &size,
                frame.dc,
                &source,
                0,
                &blend,
                ULW_ALPHA,
            );
            if self.shown.is_none() {
                ShowWindow(self.window, SW_SHOWNOACTIVATE);
            }
        }
        self.shown = Some(picture);
    }

    fn hide(&mut self) {
        if self.shown.take().is_some() {
            // SAFETY: the window belongs to this overlay.
            unsafe { ShowWindow(self.window, SW_HIDE) };
        }
    }
}

/// A memory device context holding one frame as a 32-bit top-down DIB.
fn bitmap(screen: HDC, side: i32, bgra: &[u8]) -> Option<Bitmap> {
    let header = BITMAPINFOHEADER {
        biSize: u32::try_from(std::mem::size_of::<BITMAPINFOHEADER>()).ok()?,
        biWidth: side,
        biHeight: -side,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB,
        biSizeImage: 0,
        biXPelsPerMeter: 0,
        biYPelsPerMeter: 0,
        biClrUsed: 0,
        biClrImportant: 0,
    };
    let info = BITMAPINFO {
        bmiHeader: header,
        bmiColors: [windows_sys::Win32::Graphics::Gdi::RGBQUAD {
            rgbBlue: 0,
            rgbGreen: 0,
            rgbRed: 0,
            rgbReserved: 0,
        }],
    };
    let mut bits: *mut c_void = null_mut();
    // SAFETY: `info` describes a `side` × `side` 32-bit DIB; Windows
    // allocates it and writes its address to `bits`.
    let dib: HBITMAP =
        unsafe { CreateDIBSection(screen, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0) };
    if dib.is_null() || bits.is_null() {
        return None;
    }
    let length = usize::try_from(side).ok()?.pow(2) * 4;
    if bgra.len() != length {
        return None;
    }
    // SAFETY: `bits` points at the DIB's `length` bytes, which nothing else
    // references yet, and `bgra` is exactly that long.
    unsafe { std::ptr::copy_nonoverlapping(bgra.as_ptr(), bits.cast::<u8>(), length) };
    // SAFETY: a memory DC compatible with the screen, owning the DIB for the
    // life of the process.
    let dc = unsafe { CreateCompatibleDC(screen) };
    if dc.is_null() {
        return None;
    }
    // SAFETY: both handles were just created and are valid.
    unsafe { SelectObject(dc, dib) };
    Some(Bitmap { dc })
}

fn round(value: f64) -> i32 {
    let value = value.round().clamp(f64::from(i32::MIN), f64::from(i32::MAX));
    // A clamped, rounded float converts exactly; parse it rather than cast.
    format!("{value:.0}").parse().unwrap_or(0)
}

fn opacity(value: f64) -> u8 {
    let scaled = (value.clamp(0.0, 1.0) * 255.0).round();
    (0..=u8::MAX).find(|&alpha| f64::from(alpha) >= scaled).unwrap_or(u8::MAX)
}

pub(crate) fn run(mut driver: Driver) {
    let Some(mut overlay) = Overlay::new() else {
        return;
    };
    // SAFETY: the timer belongs to the overlay's window on this thread.
    unsafe { SetTimer(overlay.window, 1, FRAME_MS, None) };
    // SAFETY: `MSG` is plain data that `GetMessageW` fills in.
    let mut message: MSG = unsafe { std::mem::zeroed() };
    // SAFETY: the standard message loop over this thread's queue.
    while unsafe { GetMessageW(&mut message, null_mut(), 0, 0) } > 0 {
        if message.message == WM_TIMER {
            match driver.tick() {
                Tick::Show(picture) => overlay.show(picture),
                Tick::Hidden => overlay.hide(),
                Tick::Quit => return,
            }
            continue;
        }
        // SAFETY: `message` was just filled in by `GetMessageW`.
        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}
