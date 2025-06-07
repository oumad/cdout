use std::ptr::NonNull;

use windows::{
    core::*,
    Win32::Foundation::*,
    Win32::System::Com::*,
    Win32::System::Ole::*,
    Win32::System::WindowsProgramming::MAX_PATH,
    Win32::UI::Shell::{
        Common::ITEMIDLIST,
        *,
    },
    Win32::UI::WindowsAndMessaging::*,
};

// SID_STopLevelBrowser
const SID_STopLevelBrowser: GUID = GUID::from_u128(0x4C96BE40_915C_11CF_99D3_00AA004AE837);

#[derive(Debug)]
struct ExplorerInfo {
    current_dir: String,
    selected_items: Vec<String>,
}

struct ComInitializer;

impl ComInitializer {
    fn new() -> Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED)?;
        }
        Ok(ComInitializer)
    }
}

impl Drop for ComInitializer {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

fn get_explorer_info() -> Result<Option<ExplorerInfo>> {
    let fg_hwnd = unsafe { GetForegroundWindow() };
    if fg_hwnd == HWND::default() {
        return Ok(None);
    }

    let mut class_name_buffer: [u16; 128] = [0; 128];
    let length = unsafe { GetClassNameW(fg_hwnd, &mut class_name_buffer) };

    if length == 0 {
        return Ok(None);
    }

    let class_name = String::from_utf16_lossy(&class_name_buffer[..length as usize]);

    if !(class_name == "CabinetWClass" || class_name == "ExploreWClass") {
        return Ok(None);
    }

    let shell_windows: IShellWindows = unsafe { CoCreateInstance(&ShellWindows, None, CLSCTX_LOCAL_SERVER)? };
    let count = unsafe { shell_windows.Count()? };

    for i in 0..count {
        let variant = VARIANT::from(i);
        // Use ? to propagate error, ensuring window_dispatch is valid if Ok.
        let window_dispatch = unsafe { shell_windows.Item(&variant)? };
        // According to windows-rs, if Item returns Ok(IDispatch), the IDispatch should be valid.
        // A null IDispatch with an Ok result would be an API contract violation.

        if let Ok(app) = window_dispatch.cast::<IWebBrowserApp>() {
            let hwnd_browser = HWND(unsafe { app.HWND()? });
            if hwnd_browser == fg_hwnd {
                let service_provider: IServiceProvider = app.cast()?;
                let shell_browser: IShellBrowser = unsafe { service_provider.QueryService(&SID_STopLevelBrowser, &IShellBrowser::IID)? };

                match unsafe { shell_browser.QueryActiveShellView() } {
                    Ok(shell_view) => {
                        if let Ok(folder_view) = shell_view.cast::<IFolderView>() {
                            let mut current_path_str: String;
                            let mut selected_files: Vec<String> = Vec::new();

                            // Get Current Path
                            let persist_folder: IPersistFolder2 = unsafe { folder_view.GetFolder(&IPersistFolder2::IID)? };
                            let mut pidl_current_folder_raw: *mut ITEMIDLIST = std::ptr::null_mut();

                            unsafe { persist_folder.GetCurFolder(&mut pidl_current_folder_raw)? };
                            // If GetCurFolder fails, `?` propagates the error.
                            // If it succeeds, pidl_current_folder_raw is populated.
                            // It's highly unlikely to be null if GetCurFolder succeeds, as it usually allocates.
                            // If it can be null on success, the API is tricky. Assume non-null on success.
                            if pidl_current_folder_raw.is_null() {
                                // This indicates an unexpected situation: GetCurFolder succeeded but gave a null PIDL.
                                return Err(Error::new(E_UNEXPECTED, "GetCurFolder returned a null PIDL despite success.".into()));
                            }

                            let mut path_buffer: [u16; MAX_PATH as usize] = [0; MAX_PATH as usize];
                            if unsafe { SHGetPathFromIDListW(pidl_current_folder_raw, &mut path_buffer).as_bool() } {
                                let path_len = path_buffer.iter().position(|&c| c == 0).unwrap_or(path_buffer.len());
                                current_path_str = String::from_utf16_lossy(&path_buffer[..path_len]);
                            } else {
                                // Path conversion failed, free the PIDL before returning error
                                unsafe { CoTaskMemFree(Some(pidl_current_folder_raw as *const _)) };
                                return Err(Error::new(E_FAIL, "Failed to convert current folder PIDL to path.".into()));
                            }
                            // Path conversion succeeded, free the PIDL
                            unsafe { CoTaskMemFree(Some(pidl_current_folder_raw as *const _)) };

                            // Get Selected Items
                            let selected_count = unsafe { folder_view.ItemCount(SVGIO_SELECTION)? };
                            if selected_count > 0 {
                                let mut ppenum: Option<IEnumIDList> = None;
                                unsafe {
                                    folder_view.Items(SVGIO_SELECTION, &IEnumIDList::IID, &mut ppenum as *mut _ as *mut *mut core::ffi::c_void)?;
                                }

                                if let Some(enum_pidl) = ppenum {
                                    let mut rgelt: [*mut ITEMIDLIST; 1] = [std::ptr::null_mut()];
                                    let mut celtfetched: u32 = 0;
                                    while unsafe { enum_pidl.Next(&mut rgelt, &mut celtfetched).is_ok() } && celtfetched > 0 {
                                        let pidl_selected_raw = rgelt[0];
                                        if pidl_selected_raw.is_null() {
                                            // Should not happen if Next reports celtfetched > 0
                                            continue;
                                        }

                                        let mut selected_path_buffer: [u16; MAX_PATH as usize] = [0; MAX_PATH as usize];
                                        if unsafe { SHGetPathFromIDListW(pidl_selected_raw, &mut selected_path_buffer).as_bool() } {
                                            let sel_path_len = selected_path_buffer.iter().position(|&c| c == 0).unwrap_or(selected_path_buffer.len());
                                            let path = String::from_utf16_lossy(&selected_path_buffer[..sel_path_len]);
                                            selected_files.push(path);
                                        } else {
                                            // Log or handle minor error for individual item if needed
                                            // e.g. println!("Failed to convert a selected PIDL to path.");
                                        }
                                        // Free the PIDL obtained from Next()
                                        unsafe { CoTaskMemFree(Some(pidl_selected_raw as *const _)) };
                                        rgelt[0] = std::ptr::null_mut(); // Clear the slot for safety
                                    }
                                }
                            }

                            let info = ExplorerInfo { current_dir: current_path_str, selected_items: selected_files };
                            return Ok(Some(info));

                        } else {
                             return Err(Error::new(E_NOINTERFACE, "Could not cast IShellView to IFolderView.".into()));
                        }
                    }
                    Err(e) => {
                        return Err(e);
                    }
                }
            }
        }
    }
    Ok(None)
}

fn main() -> Result<()> {
    let _com_initializer = ComInitializer::new()?;

    match get_explorer_info() {
        Ok(Some(info)) => {
            println!("{:#?}", info);
        }
        Ok(None) => {
            println!("No active Explorer window processed, or foreground window is not an Explorer window, or no foreground window found.");
        }
        Err(e) => {
            eprintln!("Error: {:?}", e);
        }
    }
    Ok(())
}
