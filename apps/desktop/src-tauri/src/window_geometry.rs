//! Window sizing and persistence shared by desktop activation paths.
use crate::{runtime_check, write_json_atomic, AppState};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

const DESKTOP_WINDOW_SIZE_FILE: &str = "window-size.json";
const DESKTOP_WINDOW_SIZE_TOLERANCE: f64 = 0.25;
const MIN_DESKTOP_WINDOW_WIDTH: f64 = 960.0;
const MIN_DESKTOP_WINDOW_HEIGHT: f64 = 640.0;
const DEFAULT_DESKTOP_WINDOW_WIDTH: f64 = 1280.0;
const DEFAULT_DESKTOP_WINDOW_HEIGHT: f64 = 820.0;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub(super) struct DesktopWindowSize {
    pub(super) width: f64,
    pub(super) height: f64,
}

fn default_window_size(target: &str) -> DesktopWindowSize {
    match target {
        "unlock" => DesktopWindowSize {
            width: 420.0,
            height: 560.0,
        },
        "quick-access" => DesktopWindowSize {
            width: 520.0,
            height: 640.0,
        },
        _ => DesktopWindowSize {
            width: DEFAULT_DESKTOP_WINDOW_WIDTH,
            height: DEFAULT_DESKTOP_WINDOW_HEIGHT,
        },
    }
}

fn window_size_path(app: &AppHandle) -> Result<PathBuf, String> {
    if let Some(path) = runtime_check::path("window-size.json") {
        return Ok(path);
    }
    let dir = app.path().app_config_dir().map_err(|err| err.to_string())?;
    Ok(dir.join(DESKTOP_WINDOW_SIZE_FILE))
}

fn window_size_is_valid(size: &DesktopWindowSize) -> bool {
    size.width.is_finite() && size.height.is_finite() && size.width > 0.0 && size.height > 0.0
}

fn window_size_within_tolerance(saved: &DesktopWindowSize, default: &DesktopWindowSize) -> bool {
    window_size_is_valid(saved)
        && (saved.width - default.width).abs() / default.width <= DESKTOP_WINDOW_SIZE_TOLERANCE
        && (saved.height - default.height).abs() / default.height <= DESKTOP_WINDOW_SIZE_TOLERANCE
}

pub(super) fn target_uses_persisted_window_size(target: &str) -> bool {
    matches!(target, "main" | "server")
}

fn clamp_main_window_size(size: DesktopWindowSize) -> DesktopWindowSize {
    DesktopWindowSize {
        width: size.width.max(MIN_DESKTOP_WINDOW_WIDTH),
        height: size.height.max(MIN_DESKTOP_WINDOW_HEIGHT),
    }
}

fn load_saved_window_size(app: &AppHandle) -> Option<DesktopWindowSize> {
    let path = window_size_path(app).ok()?;
    let bytes = fs::read(path).ok()?;
    let size = serde_json::from_slice::<DesktopWindowSize>(&bytes).ok()?;
    window_size_is_valid(&size).then_some(size)
}

pub(super) fn window_size_for_target(app: &AppHandle, target: &str) -> DesktopWindowSize {
    let default = default_window_size(target);
    if !target_uses_persisted_window_size(target) {
        return default;
    }

    let size = load_saved_window_size(app)
        .filter(|saved| window_size_within_tolerance(saved, &default))
        .map(clamp_main_window_size)
        .unwrap_or(default);
    match app.get_webview_window("main") {
        Some(window) => fit_on_monitor(&window, size),
        None => size,
    }
}

fn fit_on_monitor(window: &tauri::WebviewWindow, size: DesktopWindowSize) -> DesktopWindowSize {
    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten())
        .or_else(|| window.available_monitors().ok()?.into_iter().next());
    let border = window
        .outer_size()
        .ok()
        .zip(window.inner_size().ok())
        .map(|(outer, inner)| {
            (
                outer.width.saturating_sub(inner.width),
                outer.height.saturating_sub(inner.height),
            )
        })
        .unwrap_or_default();
    match monitor {
        Some(monitor) => fit_to_work_area(
            size,
            monitor.work_area().size.width.saturating_sub(border.0),
            monitor.work_area().size.height.saturating_sub(border.1),
            monitor.scale_factor(),
        ),
        None => size,
    }
}

// Tray-open preserves the current user-resized window rather than loading its
// saved/default size again. It must still fit when it has never been shown, or
// when its former monitor/work area has disappeared.
pub(super) fn fit_existing_window(window: &tauri::WebviewWindow) -> Result<(), String> {
    let physical = window.inner_size().map_err(|err| err.to_string())?;
    let scale = window.scale_factor().map_err(|err| err.to_string())?;
    if !scale.is_finite() || scale <= 0.0 {
        return Ok(());
    }
    let width = f64::from(physical.width) / scale;
    let height = f64::from(physical.height) / scale;
    let fitted = fit_on_monitor(window, DesktopWindowSize { width, height });
    if (fitted.width - width).abs() >= 0.5 || (fitted.height - height).abs() >= 0.5 {
        window
            .set_size(tauri::Size::Logical(tauri::LogicalSize::new(
                fitted.width,
                fitted.height,
            )))
            .map_err(|err| err.to_string())?;
    }
    Ok(())
}

// GTK only knows the window manager frame extents after mapping/configuration.
// Refit restored windows once those extents exist; never resize a maximized
// window behind the window manager or replace the user's restored dimensions.
#[cfg(target_os = "linux")]
pub(super) fn fit_after_native_event(app: &AppHandle, label: &str, event: &tauri::WindowEvent) {
    if label != "main"
        || !matches!(
            event,
            tauri::WindowEvent::Resized(_) | tauri::WindowEvent::ScaleFactorChanged { .. }
        )
        || !target_uses_persisted_window_size(&app.state::<AppState>().window_target())
    {
        return;
    }
    if let Some(window) = app.get_webview_window(label) {
        if window.is_visible().unwrap_or(false) && !window.is_maximized().unwrap_or(false) {
            let _ = fit_existing_window(&window);
        }
    }
}

pub(super) fn persist_window_size_for_target(app: &AppHandle, target: &str) -> Result<(), String> {
    if !target_uses_persisted_window_size(target) {
        return Ok(());
    }

    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main desktop window is unavailable".to_string())?;
    let physical_size = window.inner_size().map_err(|err| err.to_string())?;
    let scale_factor = window.scale_factor().map_err(|err| err.to_string())?;
    if !scale_factor.is_finite() || scale_factor <= 0.0 {
        return Err("desktop window scale factor is invalid".to_string());
    }

    let size = DesktopWindowSize {
        width: f64::from(physical_size.width) / scale_factor,
        height: f64::from(physical_size.height) / scale_factor,
    };
    if !window_size_is_valid(&size) {
        return Err("desktop window size is invalid".to_string());
    }

    let path = window_size_path(app)?;
    write_json_atomic(&path, &size)
}

pub(crate) fn persist_window_size(app: &AppHandle) -> Result<(), String> {
    let target = app.state::<AppState>().window_target();
    persist_window_size_for_target(app, &target)
}

// Work area excludes desktop panels and docks. Convert physical monitor pixels
// to logical window pixels before applying the Tauri minimum.
fn fit_to_work_area(
    size: DesktopWindowSize,
    width: u32,
    height: u32,
    scale: f64,
) -> DesktopWindowSize {
    if width == 0 || height == 0 || !scale.is_finite() || scale <= 0.0 {
        return size;
    }
    DesktopWindowSize {
        width: size
            .width
            .min((f64::from(width) / scale).max(MIN_DESKTOP_WINDOW_WIDTH)),
        height: size
            .height
            .min((f64::from(height) / scale).max(MIN_DESKTOP_WINDOW_HEIGHT)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_window_size_restores_at_or_below_twenty_five_percent_deviation() {
        let default = default_window_size("main");
        let saved = DesktopWindowSize {
            width: default.width * 1.25,
            height: default.height * 0.75,
        };

        assert!(window_size_within_tolerance(&saved, &default));
    }

    #[test]
    fn desktop_window_size_falls_back_when_either_dimension_exceeds_tolerance() {
        let default = default_window_size("main");
        let saved = DesktopWindowSize {
            width: default.width * 1.251,
            height: default.height,
        };

        assert!(!window_size_within_tolerance(&saved, &default));
    }

    #[test]
    fn desktop_window_size_rejects_non_positive_and_non_finite_values() {
        let default = default_window_size("main");

        for saved in [
            DesktopWindowSize {
                width: 0.0,
                height: default.height,
            },
            DesktopWindowSize {
                width: f64::NAN,
                height: default.height,
            },
            DesktopWindowSize {
                width: default.width,
                height: f64::INFINITY,
            },
        ] {
            assert!(!window_size_within_tolerance(&saved, &default));
        }
    }

    #[test]
    fn desktop_main_window_size_never_drops_below_tauri_minimum() {
        let clamped = clamp_main_window_size(DesktopWindowSize {
            width: 960.0,
            height: 615.0,
        });

        assert_eq!(clamped.width, MIN_DESKTOP_WINDOW_WIDTH);
        assert_eq!(clamped.height, MIN_DESKTOP_WINDOW_HEIGHT);
    }

    #[test]
    fn desktop_special_window_targets_keep_their_fixed_sizes() {
        let main = default_window_size("main");

        assert!(target_uses_persisted_window_size("main"));
        assert!(target_uses_persisted_window_size("server"));
        assert!(!target_uses_persisted_window_size("unlock"));
        assert!(!target_uses_persisted_window_size("quick-access"));
        assert_eq!(default_window_size("unlock").width, 420.0);
        assert_eq!(default_window_size("quick-access").height, 640.0);
        assert_eq!(default_window_size("server").width, main.width);
    }

    #[test]
    fn first_launch_and_saved_sizes_fit_small_x11_work_areas() {
        let size = fit_to_work_area(default_window_size("main"), 1280, 772, 1.0);
        assert_eq!((size.width, size.height), (1280.0, 772.0));
        let size = fit_to_work_area(
            DesktopWindowSize {
                width: 1600.0,
                height: 1025.0,
            },
            1280,
            772,
            1.0,
        );
        assert_eq!((size.width, size.height), (1280.0, 772.0));
    }

    #[test]
    fn revealing_an_existing_window_keeps_user_dimensions_that_still_fit() {
        for (width, height) in [(960.0, 640.0), (1150.0, 700.0)] {
            let fitted = fit_to_work_area(DesktopWindowSize { width, height }, 1276, 769, 1.0);
            assert_eq!((fitted.width, fitted.height), (width, height));
        }
    }

    #[test]
    fn hidpi_work_areas_use_logical_pixels_and_preserve_the_minimum() {
        let size = fit_to_work_area(default_window_size("main"), 2400, 1440, 2.0);
        assert_eq!((size.width, size.height), (1200.0, 720.0));
        let size = fit_to_work_area(default_window_size("main"), 800, 600, 1.0);
        assert_eq!((size.width, size.height), (960.0, 640.0));
        for scale in [0.0, f64::NAN, f64::INFINITY] {
            let size = fit_to_work_area(default_window_size("main"), 1280, 772, scale);
            assert_eq!((size.width, size.height), (1280.0, 820.0));
        }
    }
}
