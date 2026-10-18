//! Classic tray menu construction and Linux GTK localization refresh.
use super::*;
use tauri::menu::{Menu, PredefinedMenuItem};

#[derive(Clone)]
pub(super) struct TrayMenuItems {
    pub(super) panel_menu: Submenu<tauri::Wry>,
    pub(super) panel_status: MenuItem<tauri::Wry>,
    pub(super) panel_open: MenuItem<tauri::Wry>,
    pub(super) panel_copy: MenuItem<tauri::Wry>,
    pub(super) panel_stop: MenuItem<tauri::Wry>,
    pub(super) open: MenuItem<tauri::Wry>,
    pub(super) hide: MenuItem<tauri::Wry>,
    pub(super) refresh: MenuItem<tauri::Wry>,
    pub(super) quit: MenuItem<tauri::Wry>,
    pub(super) proxy_menu: Submenu<tauri::Wry>,
    pub(super) status: MenuItem<tauri::Wry>,
    pub(super) start_agent: MenuItem<tauri::Wry>,
    pub(super) lock: MenuItem<tauri::Wry>,
    pub(super) install_login_agent: MenuItem<tauri::Wry>,
    pub(super) proxy_status: MenuItem<tauri::Wry>,
    pub(super) proxy_open: MenuItem<tauri::Wry>,
    pub(super) proxy_start: MenuItem<tauri::Wry>,
    pub(super) proxy_stop: MenuItem<tauri::Wry>,
    pub(super) proxy_refresh: MenuItem<tauri::Wry>,
    pub(super) group_menu: Submenu<tauri::Wry>,
    #[cfg(target_os = "linux")]
    rendered_locale: std::sync::Arc<Mutex<Option<aipass_agent_protocol::UiLocale>>>,
}

pub(super) fn build(app: &App) -> tauri::Result<(Menu<tauri::Wry>, TrayMenuItems)> {
    let status = MenuItem::with_id(app, MENU_STATUS, tr("tray.checking"), false, None::<&str>)?;
    let open = MenuItem::with_id(app, MENU_OPEN, tr("ext.openApp"), true, None::<&str>)?;
    let hide = MenuItem::with_id(app, MENU_HIDE, tr("tray.hideWindow"), true, None::<&str>)?;
    let refresh = MenuItem::with_id(app, MENU_REFRESH, tr("ext.refresh"), true, None::<&str>)?;
    let start_agent = MenuItem::with_id(
        app,
        MENU_START_AGENT,
        tr("tray.startAgent"),
        false,
        None::<&str>,
    )?;
    let lock = MenuItem::with_id(app, MENU_LOCK, tr("titlebar.lock"), false, None::<&str>)?;
    let proxy_status = MenuItem::with_id(
        app,
        MENU_PROXY_STATUS,
        tr("tray.checking"),
        false,
        None::<&str>,
    )?;
    let proxy_open = MenuItem::with_id(
        app,
        MENU_PROXY_OPEN,
        tr("tray.openServer"),
        true,
        None::<&str>,
    )?;
    let proxy_start = MenuItem::with_id(
        app,
        MENU_PROXY_START,
        tr("tray.startProxy"),
        false,
        None::<&str>,
    )?;
    let proxy_stop = MenuItem::with_id(
        app,
        MENU_PROXY_STOP,
        tr("tray.stopProxy"),
        false,
        None::<&str>,
    )?;
    let proxy_refresh = MenuItem::with_id(
        app,
        MENU_PROXY_REFRESH,
        tr("ext.refresh"),
        true,
        None::<&str>,
    )?;
    let group_menu = Submenu::with_items(app, tr("tray.switchGroup"), true, &[])?;
    let proxy_menu = Submenu::with_items(
        app,
        tr("tray.proxy"),
        true,
        &[
            &proxy_status,
            &proxy_open,
            &PredefinedMenuItem::separator(app)?,
            &proxy_start,
            &proxy_stop,
            &proxy_refresh,
            &PredefinedMenuItem::separator(app)?,
            &group_menu,
        ],
    )?;

    let install_login_agent = MenuItem::with_id(
        app,
        MENU_INSTALL_LOGIN_AGENT,
        tr("tray.repairAutostart"),
        true,
        None::<&str>,
    )?;

    let quit = MenuItem::with_id(app, MENU_QUIT, tr("tray.quit"), true, None::<&str>)?;
    let panel_status = MenuItem::with_id(
        app,
        "panel-status",
        tr("tray.panelOff"),
        false,
        None::<&str>,
    )?;
    let panel_open = MenuItem::with_id(
        app,
        action::PANEL_OPEN_URL,
        tr("tray.panelOpen"),
        false,
        None::<&str>,
    )?;
    let panel_copy = MenuItem::with_id(
        app,
        action::PANEL_COPY,
        tr("tray.panelCopy"),
        false,
        None::<&str>,
    )?;
    let panel_stop = MenuItem::with_id(
        app,
        action::PANEL_STOP,
        tr("tray.panelStop"),
        false,
        None::<&str>,
    )?;
    let panel_menu = Submenu::with_items(
        app,
        tr("tray.panel"),
        true,
        &[&panel_status, &panel_open, &panel_copy, &panel_stop],
    )?;

    let items = TrayMenuItems {
        panel_menu,
        panel_status,
        panel_open,
        panel_copy,
        panel_stop,
        open,
        hide,
        refresh,
        quit,
        proxy_menu,
        status,
        start_agent,
        lock,
        install_login_agent,
        proxy_status,
        proxy_open,
        proxy_start,
        proxy_stop,
        proxy_refresh,
        group_menu,
        #[cfg(target_os = "linux")]
        rendered_locale: std::sync::Arc::new(Mutex::new(None)),
    };

    let menu = items.menu(app.handle())?;
    Ok((menu, items))
}

impl TrayMenuItems {
    fn menu(&self, app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
        Menu::with_items(
            app,
            &[
                &self.status,
                &PredefinedMenuItem::separator(app)?,
                &self.open,
                &self.hide,
                &PredefinedMenuItem::separator(app)?,
                &self.refresh,
                &self.start_agent,
                &self.lock,
                &self.install_login_agent,
                &self.proxy_menu,
                &self.panel_menu,
                &PredefinedMenuItem::separator(app)?,
                &self.quit,
            ],
        )
    }

    #[cfg(target_os = "linux")]
    pub(super) fn refresh_locale(&self, app: &AppHandle) {
        let locale = crate::tray_i18n::locale();
        let Ok(mut rendered) = self.rendered_locale.lock() else {
            return;
        };
        if *rendered == Some(locale) {
            return;
        }
        // muda's GTK submenu uses a custom AccelLabel child. set_text updates
        // MenuItem.label but leaves that visible child unchanged. Recreate the
        // root only when locale changes, retaining action IDs and live states.
        if let (Some(tray), Ok(menu)) = (app.tray_by_id(TRAY_ID), self.menu(app)) {
            if tray.set_menu(Some(menu)).is_ok() {
                *rendered = Some(locale);
            }
        }
    }
}
