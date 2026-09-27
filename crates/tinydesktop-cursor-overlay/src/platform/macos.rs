//! macOS: a borderless, transparent `NSWindow` above every other window,
//! ignoring the mouse, joining every Space, showing the sprite in an
//! `NSImageView`. The app is an accessory, so it has no Dock icon or menu
//! bar and never becomes active.
//!
//! A repeating `NSTimer` on the main run loop ticks the driver at display
//! rate and moves, reframes, or fades the window.

// Creating the window and scheduling the timer are the two AppKit calls the
// bindings mark `unsafe`; each is justified where it is made.
#![allow(unsafe_code)]

use std::cell::RefCell;
use std::ptr::NonNull;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::{AnyThread, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSColor, NSImage,
    NSImageScaling, NSImageView, NSScreen, NSScreenSaverWindowLevel, NSWindow,
    NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{NSData, NSPoint, NSRect, NSSize, NSTimer};
use tinydesktop_cursor::sprite::{SIZE, Sprite, png};

use crate::driver::{Driver, Picture, Tick};

/// How often the overlay redraws: a display's refresh.
const FRAME_SECONDS: f64 = 1.0 / 60.0;

/// Pixels per point the sprite is drawn at: sharp on Retina, and scaled down
/// cleanly elsewhere.
const SCALE: u32 = 2;

struct Overlay {
    window: Retained<NSWindow>,
    view: Retained<NSImageView>,
    images: Vec<Retained<NSImage>>,
    shown: Option<Picture>,
    mtm: MainThreadMarker,
}

impl Overlay {
    fn new(mtm: MainThreadMarker) -> Option<Self> {
        let side = f64::from(SIZE);
        let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(side, side));
        // SAFETY: called on the main thread (`mtm`) with a valid rectangle and
        // a borderless style. `releasedWhenClosed` is turned off straight
        // after, so the `Retained` handle stays the window's only owner and
        // the window is never closed while it is alive.
        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(mtm),
                frame,
                NSWindowStyleMask::Borderless,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        // SAFETY: see above; the window is owned by `Retained` alone.
        unsafe { window.setReleasedWhenClosed(false) };
        window.setOpaque(false);
        window.setBackgroundColor(Some(&NSColor::clearColor()));
        window.setHasShadow(false);
        window.setIgnoresMouseEvents(true);
        window.setLevel(NSScreenSaverWindowLevel);
        window.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::IgnoresCycle,
        );

        let sprite = Sprite::render(SCALE);
        let pixels = sprite.pixels();
        let images: Vec<Retained<NSImage>> = sprite
            .frames
            .iter()
            .filter_map(|frame| {
                let data = NSData::with_bytes(&png(pixels, pixels, frame));
                let image = NSImage::initWithData(NSImage::alloc(), &data)?;
                image.setSize(NSSize::new(side, side));
                Some(image)
            })
            .collect();
        let view = NSImageView::imageViewWithImage(images.first()?, mtm);
        view.setImageScaling(NSImageScaling::ScaleAxesIndependently);
        view.setFrame(frame);
        window.setContentView(Some(&view));
        Some(Self {
            window,
            view,
            images,
            shown: None,
            mtm,
        })
    }

    /// The height of the primary display, which `AppKit`'s bottom-left origin
    /// is measured from.
    fn primary_height(&self) -> f64 {
        NSScreen::screens(self.mtm)
            .firstObject()
            .map_or(0.0, |screen| screen.frame().size.height)
    }

    fn show(&mut self, picture: Picture) {
        if self.shown == Some(picture) {
            return;
        }
        let side = f64::from(SIZE);
        let origin = NSPoint::new(
            picture.origin.x,
            self.primary_height() - picture.origin.y - side,
        );
        self.window.setFrameOrigin(origin);
        if self.shown.map(|shown| shown.sprite_frame) != Some(picture.sprite_frame)
            && let Some(image) = self.images.get(picture.sprite_frame)
        {
            self.view.setImage(Some(image));
        }
        self.window.setAlphaValue(picture.opacity);
        if self.shown.is_none() {
            self.window.orderFrontRegardless();
        }
        self.shown = Some(picture);
    }

    fn hide(&mut self) {
        if self.shown.take().is_some() {
            self.window.orderOut(None);
        }
    }
}

pub(crate) fn run(driver: Driver) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    let Some(overlay) = Overlay::new(mtm) else {
        return;
    };
    let state = RefCell::new((driver, overlay));
    let block = RcBlock::new(move |_timer: NonNull<NSTimer>| {
        let Ok(mut state) = state.try_borrow_mut() else {
            return;
        };
        let (driver, overlay) = &mut *state;
        match driver.tick() {
            Tick::Show(picture) => overlay.show(picture),
            Tick::Hidden => overlay.hide(),
            Tick::Quit => std::process::exit(0),
        }
    });
    // SAFETY: the block is `'static` (it owns everything it touches), is
    // only ever invoked by the main run loop on the main thread — where it
    // was created and where the window it drives lives — and the timer
    // retains it for as long as the timer is scheduled, which is the life of
    // the process.
    let _timer = unsafe {
        NSTimer::scheduledTimerWithTimeInterval_repeats_block(FRAME_SECONDS, true, &block)
    };
    app.run();
}
