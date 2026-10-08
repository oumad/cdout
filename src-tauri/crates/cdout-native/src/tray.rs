//! System tray / menu-bar icon with "Show cdout" and "Quit cdout".

use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use crate::dispatch::MsgSender;
use crate::state::Msg;

const SHOW_ID: &str = "show";
const QUIT_ID: &str = "quit";

static TRAY_PNG: &[u8] = include_bytes!("../../../icons/32x32.png");

fn icon() -> Result<tray_icon::Icon, String> {
    let img = image::load_from_memory(TRAY_PNG)
        .map_err(|e| e.to_string())?
        .into_rgba8();
    let (w, h) = img.dimensions();
    tray_icon::Icon::from_rgba(img.into_raw(), w, h).map_err(|e| e.to_string())
}

/// Build the tray icon. Must be called on the UI thread after the event loop
/// has started; the returned handle must be kept alive.
pub fn create(out: MsgSender) -> Result<TrayIcon, String> {
    let menu = Menu::new();
    menu.append_items(&[
        &MenuItem::with_id(SHOW_ID, "Show cdout", true, None),
        &PredefinedMenuItem::separator(),
        &MenuItem::with_id(QUIT_ID, "Quit cdout", true, None),
    ])
    .map_err(|e| e.to_string())?;

    let builder = TrayIconBuilder::new()
        .with_tooltip("cdout")
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false);
    // Menu-bar icons must be template images on macOS, or the fixed-colour
    // PNG stays dark on a dark menu bar.
    #[cfg(target_os = "macos")]
    let builder = builder.with_icon_templated(icon()?);
    #[cfg(not(target_os = "macos"))]
    let builder = builder.with_icon(icon()?);
    let tray = builder.build().map_err(|e| e.to_string())?;

    let menu_out = out.clone();
    MenuEvent::set_event_handler(Some(move |e: MenuEvent| match e.id.0.as_str() {
        SHOW_ID => menu_out.send(Msg::TrayShowMain),
        QUIT_ID => menu_out.send(Msg::TrayQuit),
        _ => {}
    }));
    TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = e
        {
            out.send(Msg::TrayShowMain);
        }
    }));
    tracing::info!(target: "cdout", "tray.created");
    Ok(tray)
}
