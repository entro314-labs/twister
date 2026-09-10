//! Window material — the OS effect drawn behind a transparent window.
//!
//! The renderer stamps `data-material` from what this ACTUALLY applied, never
//! from the preference: macOS and Windows always accept, Linux compositors
//! routinely do not, and a see-through sidebar with nothing frosting behind it
//! shows the desktop wallpaper through the chrome.

use tauri::Window;

/// Applies a material and returns what the OS really did — `off` whenever the
/// platform or the compositor refused.
pub fn apply_material(window: &Window, requested: &str) -> String {
    #[cfg(target_os = "macos")]
    {
        use window_vibrancy::{NSVisualEffectMaterial, apply_vibrancy, clear_vibrancy};

        let material = match requested {
            "standard" => Some(NSVisualEffectMaterial::Sidebar),
            "strong" => Some(NSVisualEffectMaterial::UnderWindowBackground),
            _ => None,
        };
        if let Some(material) = material {
            if apply_vibrancy(window, material, None, None).is_ok() {
                requested.to_string()
            } else {
                "off".to_string()
            }
        } else {
            let _ = clear_vibrancy(window);
            "off".to_string()
        }
    }

    #[cfg(target_os = "windows")]
    {
        use window_vibrancy::{apply_mica, clear_mica};

        if requested == "off" {
            let _ = clear_mica(window);
            return "off".to_string();
        }
        // Mica has no strength control; both levels map to the one effect and the
        // renderer's own alpha is what differs.
        return if apply_mica(window, None).is_ok() {
            requested.to_string()
        } else {
            "off".to_string()
        };
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        // Linux has no portable equivalent that can be applied from here, so the
        // honest answer is that nothing was applied.
        let _ = (window, requested);
        "off".to_string()
    }
}

/// macOS: hands traffic-light placement back to `AppKit`.
///
/// Transparent title bar, hidden title, full-size content view — then an EMPTY
/// unified `NSToolbar`. The toolbar paints nothing (the bar is transparent, the
/// title is hidden, the separator is off); it is installed for its geometry
/// alone. A toolbar window gets the standard 52px title bar and `AppKit` centres
/// the lights in it — centre y = 26, buttons ending near x = 79 — the same
/// metric Finder has, on every macOS since Big Sur. `TITLEBAR_H` and
/// `TITLEBAR_INSET_LEFT` in `chrome.ts` are sized to that band; change them
/// together or the shell's header strips step against the lights.
///
/// Deliberately NOT `traffic_light_position`, which is what this used to do:
/// `tao` implements it by growing the title-bar container and re-setting only the
/// buttons' x on every `drawRect`. macOS 26+ no longer moves the buttons with
/// that container, so the y is inert there while still shifting them on 14/15 —
/// one placement per OS version. Setting the buttons' frames by hand is no
/// better: `AppKit` re-lays them out and undoes it.
#[cfg(target_os = "macos")]
pub fn apply_macos_chrome(window: &Window) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    use objc2_app_kit::{
        NSTitlebarSeparatorStyle, NSWindowStyleMask, NSWindowTitleVisibility, NSWindowToolbarStyle,
    };

    let Ok(ns_window) = window.ns_window() else {
        log::warn!("no NSWindow for the main window; the traffic lights keep AppKit's defaults");
        return;
    };
    let ns_window = ns_window.cast::<AnyObject>();

    // SAFETY: `ns_window` is the live NSWindow Tauri just returned for the main
    // window, and this runs on the main thread — it is called from setup. The
    // style mask is or-ed into rather than replaced, so the bits Tauri owns
    // survive. `-[NSToolbar new]` hands over a +1 reference the window takes
    // ownership of in `setToolbar:`.
    #[expect(unsafe_code)]
    unsafe {
        let _: () = msg_send![ns_window, setTitlebarAppearsTransparent: true];
        let _: () = msg_send![ns_window, setTitleVisibility: NSWindowTitleVisibility::Hidden];

        let current: NSWindowStyleMask = msg_send![ns_window, styleMask];
        let full_size = current | NSWindowStyleMask::FullSizeContentView;
        let _: () = msg_send![ns_window, setStyleMask: full_size];

        let toolbar: *mut AnyObject = msg_send![objc2::class!(NSToolbar), new];
        let _: () = msg_send![toolbar, setAllowsUserCustomization: false];
        let _: () = msg_send![ns_window, setToolbar: toolbar];
        let _: () = msg_send![ns_window, setToolbarStyle: NSWindowToolbarStyle::Unified];
        let _: () = msg_send![ns_window, setTitlebarSeparatorStyle: NSTitlebarSeparatorStyle::None];
    }
}
