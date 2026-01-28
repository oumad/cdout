use serde::Serialize;
use windows::core::{ComInterface, Result};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_LOCAL_SERVER, COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::Variant::{VARIANT, VT_I4};
use windows::Win32::UI::Shell::{
    IShellFolderViewDual, IShellWindows, IWebBrowserApp, ShellWindows,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetForegroundWindow, GetWindow, GetWindowTextW, IsWindowVisible, GW_HWNDNEXT,
};

#[derive(Serialize)]
pub struct ExplorerState {
    pub path: String,
    pub selected_files: Vec<String>,
}

#[derive(Serialize, Debug)]
pub struct ExplorerDebugInfo {
    pub foreground_hwnd: String,
    pub foreground_class: String,
    pub foreground_title: String,
    pub target_explorer_hwnd: String,
    pub target_explorer_title: String,
    pub is_explorer_window: bool,
    pub shell_windows: Vec<ShellWindowInfo>,
}

#[derive(Serialize, Debug)]
pub struct ShellWindowInfo {
    pub index: i32,
    pub hwnd: String,
    pub location_name: String,
    pub location_url: String,
    pub selected_count: i32,
    pub hwnd_matches: bool,
    pub title_matches: bool,
}

/// Get the class name of a window
fn get_window_class(hwnd: HWND) -> String {
    unsafe {
        let mut class_buf = [0u16; 256];
        let class_len = GetClassNameW(hwnd, &mut class_buf);
        if class_len > 0 {
            String::from_utf16_lossy(&class_buf[..class_len as usize])
        } else {
            String::new()
        }
    }
}

/// Get the title of a window
fn get_window_title(hwnd: HWND) -> String {
    unsafe {
        let mut buf = [0u16; 512];
        let len = GetWindowTextW(hwnd, &mut buf);
        if len > 0 {
            String::from_utf16_lossy(&buf[..len as usize])
                .trim()
                .to_string()
        } else {
            String::new()
        }
    }
}

/// Find the first Explorer window in Z-order starting from (and including) the given hwnd
/// If start_hwnd is an Explorer window, returns it. Otherwise walks down Z-order.
fn find_explorer_window_from(start_hwnd: HWND) -> Option<(HWND, String)> {
    unsafe {
        let mut current = start_hwnd;

        // Check up to 20 windows in Z-order
        for _ in 0..20 {
            if current.0 == 0 {
                break;
            }

            // Only consider visible windows
            if IsWindowVisible(current).as_bool() {
                let class_name = get_window_class(current);
                if class_name == "CabinetWClass" {
                    let title = get_window_title(current);
                    return Some((current, title));
                }
            }

            // Move to next window in Z-order
            current = GetWindow(current, GW_HWNDNEXT);
        }

        None
    }
}

/// Improved title matching that handles various Windows title formats
fn title_matches_location(window_title: &str, location_name: &str) -> bool {
    if location_name.is_empty() {
        return false;
    }

    let title_lower = window_title.to_lowercase();
    let loc_lower = location_name.to_lowercase();

    // Exact match
    if title_lower == loc_lower {
        return true;
    }

    // Title starts with location name (e.g., "scenes" in "scenes - File Explorer")
    if title_lower.starts_with(&loc_lower) {
        return true;
    }

    // Title contains location name
    if title_lower.contains(&loc_lower) {
        return true;
    }

    false
}

/// Get debug info about all Explorer windows/tabs
pub fn get_explorer_debug_info() -> Result<ExplorerDebugInfo> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        let foreground_hwnd = GetForegroundWindow();
        let foreground_class = get_window_class(foreground_hwnd);
        let foreground_title = get_window_title(foreground_hwnd);
        let is_explorer_window = foreground_class == "CabinetWClass";

        // Find the target Explorer window (foreground if it's Explorer, otherwise next in Z-order)
        let (target_hwnd, target_title) = if is_explorer_window {
            (foreground_hwnd, foreground_title.clone())
        } else {
            // Walk Z-order to find first Explorer window below the current app
            find_explorer_window_from(foreground_hwnd).unwrap_or((HWND(0), String::new()))
        };

        let shell_windows: IShellWindows =
            CoCreateInstance(&ShellWindows, None, CLSCTX_LOCAL_SERVER)?;
        let count = shell_windows.Count()?;

        let mut windows_info = Vec::new();

        for i in 0..count {
            let mut variant = VARIANT::default();
            (*variant.Anonymous.Anonymous).vt = VT_I4;
            (*variant.Anonymous.Anonymous).Anonymous.lVal = i;

            if let Ok(disp) = shell_windows.Item(variant) {
                if let Ok(browser) = disp.cast::<IWebBrowserApp>() {
                    let name = browser.FullName()?;
                    let name_str = name.to_string().to_lowercase();
                    if name_str.ends_with("explorer.exe") {
                        let tab_hwnd_long = browser.HWND()?;
                        let tab_hwnd = HWND(tab_hwnd_long.0 as _);
                        let loc_name = browser.LocationName()?.to_string();
                        let loc_url = browser.LocationURL()?.to_string();

                        let mut selected_count = 0;
                        if let Ok(doc) = browser.Document() {
                            if let Ok(fv) = doc.cast::<IShellFolderViewDual>() {
                                if let Ok(items) = fv.SelectedItems() {
                                    if let Ok(cnt) = items.Count() {
                                        selected_count = cnt;
                                    }
                                }
                            }
                        }

                        let hwnd_matches = tab_hwnd == target_hwnd;
                        let title_matches = title_matches_location(&target_title, &loc_name);

                        windows_info.push(ShellWindowInfo {
                            index: i,
                            hwnd: format!("{:?}", tab_hwnd.0),
                            location_name: loc_name,
                            location_url: loc_url,
                            selected_count,
                            hwnd_matches,
                            title_matches,
                        });
                    }
                }
            }
        }

        Ok(ExplorerDebugInfo {
            foreground_hwnd: format!("{:?}", foreground_hwnd.0),
            foreground_class,
            foreground_title,
            target_explorer_hwnd: format!("{:?}", target_hwnd.0),
            target_explorer_title: target_title,
            is_explorer_window,
            shell_windows: windows_info,
        })
    }
}

pub fn get_active_explorer_info() -> Result<ExplorerState> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        // 1. Get the foreground window
        let foreground_hwnd = GetForegroundWindow();
        let foreground_class = get_window_class(foreground_hwnd);
        let is_explorer_foreground = foreground_class == "CabinetWClass";

        // 2. Find the target Explorer window
        // If foreground is Explorer, use it. Otherwise, find the first Explorer in Z-order.
        let (target_hwnd, target_title) = if is_explorer_foreground {
            (foreground_hwnd, get_window_title(foreground_hwnd))
        } else {
            // Walk Z-order starting from foreground to find first Explorer window
            find_explorer_window_from(foreground_hwnd).unwrap_or((HWND(0), String::new()))
        };

        // If no Explorer window found, return empty
        if target_hwnd.0 == 0 {
            return Ok(ExplorerState {
                path: String::new(),
                selected_files: Vec::new(),
            });
        }

        let shell_windows: IShellWindows =
            CoCreateInstance(&ShellWindows, None, CLSCTX_LOCAL_SERVER)?;
        let count = shell_windows.Count()?;

        let mut path = String::new();
        let mut selected_files: Vec<String> = Vec::new();

        // Simple logic: iterate through all tabs from the target window,
        // and find the one that matches the window title.

        let mut fallback_path = String::new();
        let mut fallback_selected: Vec<String> = Vec::new();
        let mut title_match_found = false;

        for i in 0..count {
            let mut variant = VARIANT::default();
            (*variant.Anonymous.Anonymous).vt = VT_I4;
            (*variant.Anonymous.Anonymous).Anonymous.lVal = i;

            if let Ok(disp) = shell_windows.Item(variant) {
                if let Ok(browser) = disp.cast::<IWebBrowserApp>() {
                    let name = browser.FullName()?;
                    let name_str = name.to_string().to_lowercase();
                    if name_str.ends_with("explorer.exe") {
                        let tab_hwnd_long = browser.HWND()?;
                        let tab_hwnd = HWND(tab_hwnd_long.0 as _);
                        let loc_name = browser.LocationName()?.to_string();
                        let loc_url = browser.LocationURL()?.to_string();

                        // Only consider tabs from the target Explorer window
                        if tab_hwnd != target_hwnd {
                            continue;
                        }

                        let current_path = if loc_url.starts_with("file:///") {
                            let p = loc_url.trim_start_matches("file:///").replace("/", "\\");
                            urlencoding::decode(&p)
                                .unwrap_or(std::borrow::Cow::Borrowed(&p))
                                .to_string()
                        } else {
                            continue; // Skip non-file locations
                        };

                        // Collect selection for this tab
                        let mut current_selected = Vec::new();
                        if let Ok(doc) = browser.Document() {
                            if let Ok(fv) = doc.cast::<IShellFolderViewDual>() {
                                if let Ok(items) = fv.SelectedItems() {
                                    if let Ok(cnt) = items.Count() {
                                        for j in 0..cnt {
                                            let mut v = VARIANT::default();
                                            (*v.Anonymous.Anonymous).vt = VT_I4;
                                            (*v.Anonymous.Anonymous).Anonymous.lVal = j;
                                            if let Ok(item) = items.Item(v) {
                                                if let Ok(p) = item.Path() {
                                                    current_selected.push(p.to_string());
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Check for title match
                        let matches_title = title_matches_location(&target_title, &loc_name);

                        if matches_title {
                            path = current_path;
                            selected_files = current_selected;
                            title_match_found = true;
                            break; // Stop immediately if we find the active tab (matched by title)
                        }

                        // Keep track of any tab with selections as a last resort
                        if !current_selected.is_empty() && fallback_path.is_empty() {
                            fallback_path = current_path;
                            fallback_selected = current_selected;
                        }
                    }
                }
            }
        }

        // If we didn't find the active tab by title, but we found another tab with selections, use that one.
        // This handles cases where the title might be lagging or generic (e.g. "File Explorer").
        if !title_match_found && !fallback_selected.is_empty() {
            path = fallback_path;
            selected_files = fallback_selected;
        }

        // Sort files using natural sort
        selected_files.sort_by(|a, b| natord::compare(a, b));

        Ok(ExplorerState {
            path,
            selected_files,
        })
    }
}
