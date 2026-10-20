//! Platform-owned window decoration, applied before native window creation.
use std::sync::atomic::{AtomicBool, Ordering};

static COMPOSITED: AtomicBool = AtomicBool::new(true);

#[tauri::command]
pub(crate) fn window_compositing() -> bool {
    COMPOSITED.load(Ordering::Relaxed)
}

pub(super) fn configure_context(context: &mut tauri::Context<tauri::Wry>) {
    if cfg!(target_os = "linux") {
        // Use the themed in-app header consistently in dev, packaged and tray-first
        // windows. GTK chooses an RGBA visual before mapping the window.
        for window in &mut context.config_mut().app.windows {
            window.decorations = false;
            window.transparent = true;
        }
    }
}

#[cfg(target_os = "macos")]
pub(super) fn configure_window_chrome(window: &tauri::WebviewWindow) {
    const WINDOW_CORNER_RADIUS: f64 = 12.0;

    let _ = window.set_background_color(Some(tauri::webview::Color(0, 0, 0, 0)));
    let _ = window.with_webview(|webview| unsafe {
        use objc2_app_kit::{NSColor, NSView, NSWindow};

        let ns_window: &NSWindow = &*webview.ns_window().cast();
        let clear = NSColor::clearColor();
        ns_window.setOpaque(false);
        ns_window.setBackgroundColor(Some(&clear));
        ns_window.setHasShadow(true);

        if let Some(content_view) = ns_window.contentView() {
            round_macos_view(&content_view, WINDOW_CORNER_RADIUS);
            if let Some(frame_view) = content_view.superview() {
                round_macos_view(&frame_view, WINDOW_CORNER_RADIUS);
            }
        }

        let webview_view: &NSView = &*webview.inner().cast();
        round_macos_view(webview_view, WINDOW_CORNER_RADIUS);
        ns_window.invalidateShadow();
    });
}

#[cfg(target_os = "linux")]
pub(super) fn configure_window_chrome(window: &tauri::WebviewWindow) {
    use tauri::Emitter;
    let _ = window.set_background_color(Some(tauri::webview::Color(0, 0, 0, 0)));
    if let Some(screen) = window
        .gtk_window()
        .ok()
        .and_then(|w| gtk::prelude::WidgetExt::screen(&w))
    {
        COMPOSITED.store(screen.is_composited(), Ordering::Relaxed);
        let window = window.clone();
        screen.connect_composited_changed(move |screen| {
            let value = screen.is_composited();
            COMPOSITED.store(value, Ordering::Relaxed);
            let _ = window.emit("window-compositing", value);
        });
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(super) fn configure_window_chrome(_: &tauri::WebviewWindow) {}

#[cfg(target_os = "macos")]
fn round_macos_view(view: &objc2_app_kit::NSView, radius: f64) {
    use objc2_quartz_core::kCACornerCurveContinuous;

    view.setWantsLayer(true);
    if let Some(layer) = view.layer() {
        layer.setCornerRadius(radius);
        let continuous_curve = unsafe { kCACornerCurveContinuous };
        layer.setCornerCurve(continuous_curve);
        layer.setMasksToBounds(true);
        layer.setOpaque(false);
    }
}
