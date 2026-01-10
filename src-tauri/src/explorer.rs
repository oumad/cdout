use serde::Serialize;
use windows::core::{Result, ComInterface, PCWSTR};
use windows::Win32::System::Com::{
    CoInitializeEx, CoCreateInstance, CLSCTX_LOCAL_SERVER,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::Variant::{VARIANT, VT_I4};
use windows::Win32::UI::Shell::{
    IShellWindows, IShellFolderViewDual, ShellWindows,
    IWebBrowserApp,
};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, GetWindowTextW,
};

#[derive(Serialize)]
pub struct ExplorerState {
    pub path: String,
    pub selected_files: Vec<String>,
}

fn to_wstring(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn get_active_explorer_info() -> Result<ExplorerState> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        // 1. Find the top-most Explorer window
        let class_name = to_wstring("CabinetWClass");
        let top_hwnd = FindWindowW(PCWSTR(class_name.as_ptr()), None);
        
        let mut top_title = String::new();
        if top_hwnd.0 != 0 {
            let mut buf = [0u16; 512];
            let len = GetWindowTextW(top_hwnd, &mut buf);
            if len > 0 {
                top_title = String::from_utf16_lossy(&buf[..len as usize]);
            }
        }
        top_title = top_title.trim().to_string();

        let shell_windows: IShellWindows = CoCreateInstance(&ShellWindows, None, CLSCTX_LOCAL_SERVER)?;
        let count = shell_windows.Count()?;
        
        let mut path = String::new();
        let mut selected_files = Vec::new();
        let mut active_tab_found = false;
        
        let mut fallback_first_tab_path = String::new();
        let mut fallback_first_tab_selected = Vec::new();

        for i in 0..count {
            let mut variant = VARIANT::default();
             (*variant.Anonymous.Anonymous).vt = VT_I4;
             (*variant.Anonymous.Anonymous).Anonymous.lVal = i;

            if let Ok(disp) = shell_windows.Item(variant) {
                if let Ok(browser) = disp.cast::<IWebBrowserApp>() {
                    // Check if it is Explorer
                    let name = browser.FullName()?;
                    let name_str = name.to_string().to_lowercase();
                    if name_str.ends_with("explorer.exe") {
                        
                        let tab_hwnd_long = browser.HWND()?;
                         // FIX: Access .0 on SHANDLE_PTR before casting
                        let tab_hwnd = windows::Win32::Foundation::HWND(tab_hwnd_long.0 as _);
                        let loc_name = browser.LocationName()?.to_string();
                        let loc_url = browser.LocationURL()?.to_string();

                        let current_path = if loc_url.starts_with("file:///") {
                            let p = loc_url.trim_start_matches("file:///").replace("/", "\\");
                            urlencoding::decode(&p).unwrap_or(std::borrow::Cow::Borrowed(&p)).to_string()
                        } else {
                            String::new()
                        };
                        
                        if !current_path.is_empty() {
                            // Collect selection
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

                            if fallback_first_tab_path.is_empty() {
                                fallback_first_tab_path = current_path.clone();
                                fallback_first_tab_selected = current_selected.clone();
                            }
                            
                            // LOGIC:
                            // 1. Must belong to the top-most window (HWND match).
                            // 2. Its Name must match the Window Title (Title typically == active tab Name).
                            
                            let is_same_window = tab_hwnd == top_hwnd;
                            // Relaxed check: Title contains LocationName (case insensitive?)
                            // Often Title is "Name", or "Name - File Explorer".
                            // LocationName is just "Name".
                            let title_match = top_title.eq_ignore_ascii_case(&loc_name) 
                                           || top_title.to_lowercase().starts_with(&loc_name.to_lowercase());

                            if is_same_window && title_match {
                                path = current_path;
                                selected_files = current_selected;
                                active_tab_found = true;
                                break;
                            }
                        }
                    }
                }
            }
        }
        
        if !active_tab_found {
            if !fallback_first_tab_path.is_empty() {
                path = fallback_first_tab_path;
                selected_files = fallback_first_tab_selected;
            }
        }
        
        // Sort files using natural sort (handles numbers correctly like Windows Explorer)
        selected_files.sort_by(|a, b| natord::compare(a, b));
        
        Ok(ExplorerState { path, selected_files })
    }
}
