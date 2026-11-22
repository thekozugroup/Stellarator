//! macOS: the main window's empty unified toolbar, and the probe's AppKit readings. Everything
//! here runs on the main thread (`run_on_main_thread`), where AppKit must be called, through the
//! objc2 crates `nsh_platform_macos` re-exports.
//!
//! The window has a hidden title bar (`titleBar: hidden`, height 52: ui/shell.rs keeps that band
//! free). An empty `NSToolbar` in the unified style is what macOS 26 apps with a sidebar have, and
//! it changes two things (measured for the Tauri app on macOS 26.6, desktop/src-tauri/src/
//! titlebar.rs): the window's corners are 26 pt instead of 16 pt, and AppKit centers the traffic
//! lights in the 52 pt band. nsh itself attaches no toolbar (it keeps a plain title bar's corners)
//! and gives the traffic lights back to AppKit once a window has one.

use std::sync::Mutex;

use nsh_platform_macos::objc2::rc::Retained;
use nsh_platform_macos::objc2::runtime::{AnyObject, NSObjectProtocol};
use nsh_platform_macos::objc2::{MainThreadMarker, MainThreadOnly, msg_send, sel};
use nsh_platform_macos::objc2_app_kit::{
    NSApplication, NSEvent, NSEventModifierFlags, NSEventType, NSToolbar, NSView, NSWindow, NSWindowButton,
    NSWindowToolbarStyle,
};
use nsh_platform_macos::objc2_foundation::{NSPoint, NSProcessInfo, NSRect, NSString};

/// What [`use_toolbar_window`] did.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Chrome {
    /// The window has the empty unified toolbar.
    pub toolbar: bool,
    /// `_cornerRadius` before the toolbar, when the window answers it.
    pub radius_before: Option<f64>,
    /// The macOS major version.
    pub major: isize,
}

/// The probe's copy of [`Chrome`].
static CHROME: Mutex<Option<Chrome>> = Mutex::new(None);

pub fn remember(chrome: Chrome) {
    *CHROME.lock().unwrap_or_else(|e| e.into_inner()) = Some(chrome);
}

/// The macOS major version.
fn major_version() -> isize {
    NSProcessInfo::processInfo().operatingSystemVersion().majorVersion
}

/// The app's main window: AppKit's main or key window, else its first visible one.
fn main_window(mtm: MainThreadMarker) -> Option<Retained<NSWindow>> {
    let app = NSApplication::sharedApplication(mtm);
    app.mainWindow()
        .or_else(|| app.keyWindow())
        .or_else(|| app.windows().iter().find(|w| w.isVisible()))
}

/// The window's `_cornerRadius` (a private AppKit method), only when the window answers it.
fn corner_radius(window: &NSWindow) -> Option<f64> {
    if !window.respondsToSelector(sel!(_cornerRadius)) {
        return None;
    }
    // SAFETY: the window answers `_cornerRadius`, which returns a CGFloat (f64 on 64-bit macOS).
    let radius: f64 = unsafe { msg_send![window, _cornerRadius] };
    Some(radius)
}

/// Gives the main window an empty unified toolbar when `attach` (macOS 26's toolbar-window corners
/// and traffic-light placement), and reports what it did.
pub fn use_toolbar_window(attach: bool) -> Chrome {
    let major = major_version();
    let Some(mtm) = MainThreadMarker::new() else {
        return Chrome { toolbar: false, radius_before: None, major };
    };
    let Some(window) = main_window(mtm) else {
        log::warn!("no main window to give a toolbar");
        return Chrome { toolbar: false, radius_before: None, major };
    };
    let radius_before = corner_radius(&window);
    if attach {
        let toolbar = NSToolbar::initWithIdentifier(NSToolbar::alloc(mtm), &NSString::from_str("stellarator.main-window"));
        window.setToolbar(Some(&toolbar));
        window.setToolbarStyle(NSWindowToolbarStyle::Unified);
    }
    Chrome { toolbar: window.toolbar().is_some(), radius_before, major }
}

/// The main window's `_cornerRadius` now.
pub fn corner_radius_of_main_window() -> Option<f64> {
    let mtm = MainThreadMarker::new()?;
    corner_radius(&main_window(mtm)?)
}

/// The radius the layout uses: what AppKit reports when it answers, else DESIGN.md's measured
/// values (26 with the toolbar on macOS 26, 16 without it, 10 before macOS 26).
pub fn ui_radius(chrome: &Chrome, measured: Option<f64>) -> f32 {
    match measured {
        Some(r) if r > 0.0 && r < 64.0 => r.round() as f32,
        _ if chrome.major >= 26 && chrome.toolbar => 26.0,
        _ if chrome.major >= 26 => 16.0,
        _ => 10.0,
    }
}

/// The class name of `object`.
fn class_name(object: &AnyObject) -> String {
    object.class().name().to_string_lossy().into_owned()
}

/// `rect` (window coordinates, origin at the bottom left) as x, y from the top left, w, h.
fn from_top(rect: NSRect, height: f64) -> String {
    format!(
        "x {:.1} y {:.1} w {:.1} h {:.1}",
        rect.origin.x,
        height - (rect.origin.y + rect.size.height),
        rect.size.width,
        rect.size.height
    )
}

/// The probe's AppKit readings, one line each.
pub fn report() -> Vec<String> {
    let mut lines = Vec::new();
    let version = NSProcessInfo::processInfo().operatingSystemVersion();
    lines.push(format!(
        "macOS {}.{}.{}",
        version.majorVersion, version.minorVersion, version.patchVersion
    ));
    let Some(mtm) = MainThreadMarker::new() else {
        lines.push("not on the main thread".into());
        return lines;
    };
    let Some(window) = main_window(mtm) else {
        lines.push("no main window".into());
        return lines;
    };
    let chrome = *CHROME.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(chrome) = chrome {
        lines.push(format!("toolbar attached {} ", chrome.toolbar));
        lines.push(format!("_cornerRadius before toolbar {:?}", chrome.radius_before));
    }
    lines.push(format!(
        "_cornerRadius now {:?} (responds {})",
        corner_radius(&window),
        window.respondsToSelector(sel!(_cornerRadius))
    ));
    let frame = window.frame();
    let height = frame.size.height;
    lines.push(format!("window frame w {:.1} h {:.1}", frame.size.width, height));
    lines.push(format!(
        "toolbar {} style {:?} titlebarAppearsTransparent {} styleMask {:#x}",
        window.toolbar().is_some(),
        window.toolbarStyle(),
        window.titlebarAppearsTransparent(),
        window.styleMask().0
    ));
    lines.push(format!("contentLayoutRect {}", from_top(window.contentLayoutRect(), height)));
    for (name, kind) in [
        ("close", NSWindowButton::CloseButton),
        ("minimize", NSWindowButton::MiniaturizeButton),
        ("zoom", NSWindowButton::ZoomButton),
    ] {
        match window.standardWindowButton(kind) {
            Some(button) => {
                let rect = button.convertRect_toView(button.bounds(), None);
                let center_y = height - (rect.origin.y + rect.size.height / 2.0);
                lines.push(format!(
                    "traffic light {name}: {} center ({:.1}, {:.1})",
                    from_top(rect, height),
                    rect.origin.x + rect.size.width / 2.0,
                    center_y
                ));
            }
            None => lines.push(format!("traffic light {name}: none")),
        }
    }
    let Some(content) = window.contentView() else {
        lines.push("no content view".into());
        return lines;
    };
    lines.push(format!("contentView {}", class_name(&content)));
    // SAFETY: the content view's superview (the window's frame view), read on the main thread.
    let Some(frame_view) = (unsafe { content.superview() }) else {
        lines.push("no frame view".into());
        return lines;
    };
    lines.push(format!("frame view {}", class_name(&frame_view)));
    for (x, y) in [(140.0, 4.0), (140.0, 26.0), (140.0, 40.0), (300.0, 26.0)] {
        // The frame view has no superview: hitTest takes window coordinates.
        let point = NSPoint::new(x, height - y);
        match frame_view.hitTest(point) {
            Some(hit) => lines.push(format!(
                "hitTest ({x}, {y}): {} in content {} chain {}",
                class_name(&hit),
                hit.isDescendantOf(&content),
                chain(&hit)
            )),
            None => lines.push(format!("hitTest ({x}, {y}): none")),
        }
    }
    lines
}

/// The classes from `view` up to the window's frame view.
fn chain(view: &NSView) -> String {
    let mut names = vec![class_name(view)];
    // SAFETY: superviews, read on the main thread.
    let mut next = unsafe { view.superview() };
    while let Some(parent) = next {
        names.push(class_name(&parent));
        next = unsafe { parent.superview() };
        if names.len() > 12 {
            break;
        }
    }
    names.join(" < ")
}

/// Posts a left click (down, then up) at each probe point of the top band into the app's event
/// queue, as the mouse would deliver it. The app's views print what reaches them.
pub fn post_band_clicks() -> Vec<String> {
    let mut lines = Vec::new();
    let Some(mtm) = MainThreadMarker::new() else {
        return vec!["not on the main thread".into()];
    };
    let Some(window) = main_window(mtm) else {
        return vec!["no main window".into()];
    };
    let app = NSApplication::sharedApplication(mtm);
    let frame = window.frame();
    let height = frame.size.height;
    let points = [
        ("sidebar band", 140.0, 26.0),
        ("toolbar band", 300.0, 26.0),
        ("New run button", frame.size.width - 80.0, 26.0),
    ];
    let uptime = NSProcessInfo::processInfo().systemUptime();
    for (i, (name, x, y)) in points.into_iter().enumerate() {
        let location = NSPoint::new(x, height - y);
        let at = uptime + i as f64 * 0.3;
        let make = |kind: NSEventType, time: f64| {
            // SAFETY: a mouse event for this window, made on the main thread.
            unsafe {
                NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
                    kind,
                    location,
                    NSEventModifierFlags::empty(),
                    time,
                    window.windowNumber(),
                    None,
                    0,
                    1,
                    1.0,
                )
            }
        };
        match (make(NSEventType::LeftMouseDown, at), make(NSEventType::LeftMouseUp, at + 0.05)) {
            (Some(down), Some(up)) => {
                app.postEvent_atStart(&down, false);
                app.postEvent_atStart(&up, false);
                lines.push(format!("posted a click at ({x:.0}, {y:.0}) for the {name}"));
            }
            _ => lines.push(format!("could not make a click for the {name}")),
        }
    }
    lines
}
