//! Dump everything the Windows Explorer bridge can see, for diagnosing
//! "wrong tab / wrong selection" reports.
//!
//!     cargo run -p cdout-core --example explorer_probe          # human-readable dump
//!     cargo run -p cdout-core --example explorer_probe -- --json # resolver diagnostics as JSON
//!
//! Prints the foreground window, every Explorer top-level window in Z-order
//! (with minimised/cloaked flags), the per-tab child windows of each one
//! with the signals that might distinguish the active tab, every
//! `IShellWindows` entry with the handles and selection it reports, and what
//! `get_active_explorer_info` resolves to.

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("explorer_probe only runs on Windows");
}

#[cfg(target_os = "windows")]
fn main() {
    // `--json`: print only the resolver's diagnostics payload, for scripts.
    if std::env::args().any(|a| a == "--json") {
        match cdout_core::features::explorer::get_explorer_debug_info() {
            Ok(info) => println!("{}", serde_json::to_string_pretty(&info).unwrap()),
            Err(e) => {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        return;
    }
    use windows::core::ComInterface;
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM, RECT};
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, IServiceProvider, CLSCTX_LOCAL_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::System::Variant::{VARIANT, VT_I4};
    use windows::Win32::UI::Shell::{
        IShellBrowser, IShellFolderViewDual, IShellWindows, IWebBrowserApp, SID_STopLevelBrowser,
        ShellWindows,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumChildWindows, EnumWindows, GetAncestor, GetClassNameW, GetDesktopWindow,
        GetForegroundWindow, GetGUIThreadInfo, GetParent, GetWindow, GetWindowRect, GetWindowTextW,
        GetWindowThreadProcessId, IsIconic, IsWindowVisible, GA_ROOT, GUITHREADINFO, GW_CHILD,
        GW_HWNDNEXT, GW_HWNDPREV,
    };

    fn class(h: HWND) -> String {
        let mut b = [0u16; 256];
        let n = unsafe { GetClassNameW(h, &mut b) } as usize;
        String::from_utf16_lossy(&b[..n])
    }
    fn title(h: HWND) -> String {
        let mut b = [0u16; 512];
        let n = unsafe { GetWindowTextW(h, &mut b) } as usize;
        String::from_utf16_lossy(&b[..n])
    }
    fn cloaked(h: HWND) -> u32 {
        let mut v: u32 = 0;
        let _ = unsafe {
            DwmGetWindowAttribute(
                h,
                DWMWA_CLOAKED,
                &mut v as *mut u32 as *mut _,
                std::mem::size_of::<u32>() as u32,
            )
        };
        v
    }
    fn rect(h: HWND) -> String {
        let mut r = RECT::default();
        let _ = unsafe { GetWindowRect(h, &mut r) };
        format!(
            "{},{} {}x{}",
            r.left,
            r.top,
            r.right - r.left,
            r.bottom - r.top
        )
    }
    unsafe extern "system" fn collect(h: HWND, l: LPARAM) -> BOOL {
        let v = &mut *(l.0 as *mut Vec<HWND>);
        v.push(h);
        BOOL(1)
    }
    fn descendants(h: HWND) -> Vec<HWND> {
        let mut v: Vec<HWND> = Vec::new();
        let _ = unsafe { EnumChildWindows(h, Some(collect), LPARAM(&mut v as *mut _ as isize)) };
        v
    }
    /// Walk up from `h` to the nearest ShellTabWindowClass ancestor.
    fn tab_ancestor(mut h: HWND) -> Option<HWND> {
        for _ in 0..32 {
            if h.0 == 0 {
                return None;
            }
            if class(h) == "ShellTabWindowClass" {
                return Some(h);
            }
            h = unsafe { GetParent(h) };
        }
        None
    }

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        let fg = GetForegroundWindow();
        println!(
            "foreground: {:?} class={:?} title={:?}",
            fg.0,
            class(fg),
            title(fg)
        );

        println!(
            "\n== top-level windows via GW_CHILD/GW_HWNDNEXT walk (what the current code does)"
        );
        let mut cur = GetWindow(GetDesktopWindow(), GW_CHILD);
        let mut walked = 0;
        let mut first_cabinet_at = None;
        while cur.0 != 0 && walked < 5000 {
            if first_cabinet_at.is_none() && class(cur) == "CabinetWClass" {
                first_cabinet_at = Some(walked);
            }
            walked += 1;
            cur = GetWindow(cur, GW_HWNDNEXT);
        }
        println!(
            "   walked {walked} windows; first CabinetWClass at index {first_cabinet_at:?} (current code stops after 200)"
        );

        println!("\n== CabinetWClass windows via EnumWindows, top of Z-order first");
        let mut all: Vec<HWND> = Vec::new();
        let _ = EnumWindows(Some(collect), LPARAM(&mut all as *mut _ as isize));
        println!("   EnumWindows returned {} top-level windows", all.len());
        let cabinets: Vec<HWND> = all
            .iter()
            .copied()
            .filter(|h| class(*h) == "CabinetWClass")
            .collect();
        for cur in &cabinets {
            println!(
                "  {:?} visible={} iconic={} cloaked={} title={:?}",
                cur.0,
                IsWindowVisible(*cur).as_bool(),
                IsIconic(*cur).as_bool(),
                cloaked(*cur),
                title(*cur)
            );
        }

        for cab in &cabinets {
            let tid = GetWindowThreadProcessId(*cab, None);
            let mut gti = GUITHREADINFO {
                cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
                ..Default::default()
            };
            let got = GetGUIThreadInfo(tid, &mut gti).is_ok();
            println!(
                "\n== {:?} {:?}\n   thread={tid} gui_info={got} active={:?} focus={:?} ({}) focus_tab={:?}",
                cab.0,
                title(*cab),
                gti.hwndActive.0,
                gti.hwndFocus.0,
                class(gti.hwndFocus),
                tab_ancestor(gti.hwndFocus).map(|h| h.0)
            );
            for k in descendants(*cab) {
                if class(k) != "ShellTabWindowClass" {
                    continue;
                }
                let top_sibling = GetWindow(k, GW_HWNDPREV).0 == 0;
                // Position among ShellTabWindowClass siblings in Z-order (0 = topmost).
                let parent = GetParent(k);
                let mut z_index = 0;
                let mut sib = GetWindow(k, GW_HWNDPREV);
                while sib.0 != 0 {
                    if class(sib) == "ShellTabWindowClass" {
                        z_index += 1;
                    }
                    sib = GetWindow(sib, GW_HWNDPREV);
                }
                let kids = descendants(k);
                let hidden: Vec<String> = kids
                    .iter()
                    .filter(|d| !IsWindowVisible(**d).as_bool())
                    .map(|d| class(*d))
                    .collect();
                let defview_visible = kids
                    .iter()
                    .find(|d| class(**d) == "SHELLDLL_DefView")
                    .map(|d| IsWindowVisible(*d).as_bool());
                println!(
                    "   tab {:?} {:?} parent={} z_index={z_index} rect={} top_sibling={top_sibling} descendants={} hidden={} defview_visible={:?}",
                    k.0,
                    title(k),
                    class(parent),
                    rect(k),
                    kids.len(),
                    hidden.len(),
                    defview_visible
                );
            }
        }

        println!("\n== IShellWindows entries");
        let sw: IShellWindows =
            CoCreateInstance(&ShellWindows, None, CLSCTX_LOCAL_SERVER).expect("ShellWindows");
        let count = sw.Count().unwrap_or(0);
        for i in 0..count {
            let mut v = VARIANT::default();
            (*v.Anonymous.Anonymous).vt = VT_I4;
            (*v.Anonymous.Anonymous).Anonymous.lVal = i;
            let Ok(disp) = sw.Item(v) else { continue };
            let Ok(browser) = disp.cast::<IWebBrowserApp>() else {
                continue;
            };
            let full = browser
                .FullName()
                .map(|s| s.to_string())
                .unwrap_or_default();
            if !full.to_lowercase().ends_with("explorer.exe") {
                println!("  [{i}] skipped: {full}");
                continue;
            }
            let frame = browser.HWND().map(|h| h.0).unwrap_or(-1);
            let frame_h = HWND(frame);
            let visible_prop = browser.Visible().map(|b| b.as_bool()).ok();
            let (tab_hwnd, tab_class, root) = match browser.cast::<IServiceProvider>() {
                Ok(sp) => match sp.QueryService::<IShellBrowser>(&SID_STopLevelBrowser) {
                    Ok(sb) => match sb.GetWindow() {
                        Ok(h) => (h.0, class(h), GetAncestor(h, GA_ROOT).0),
                        Err(e) => (-1, format!("GetWindow err {e}"), 0),
                    },
                    Err(e) => (-1, format!("QueryService err {e}"), 0),
                },
                Err(e) => (-1, format!("IServiceProvider err {e}"), 0),
            };
            let name = browser
                .LocationName()
                .map(|s| s.to_string())
                .unwrap_or_default();
            let url = browser
                .LocationURL()
                .map(|s| s.to_string())
                .unwrap_or_default();
            let mut sel = Vec::new();
            if let Ok(doc) = browser.Document() {
                if let Ok(fv) = doc.cast::<IShellFolderViewDual>() {
                    if let Ok(items) = fv.SelectedItems() {
                        let n = items.Count().unwrap_or(0);
                        for j in 0..n.min(3) {
                            let mut v = VARIANT::default();
                            (*v.Anonymous.Anonymous).vt = VT_I4;
                            (*v.Anonymous.Anonymous).Anonymous.lVal = j;
                            if let Ok(item) = items.Item(v) {
                                if let Ok(p) = item.Path() {
                                    sel.push(p.to_string());
                                }
                            }
                        }
                        if n > 3 {
                            sel.push(format!("... {} total", n));
                        }
                    }
                }
            }
            println!(
                "  [{i}] frame={frame:?} ({} visible={} iconic={} cloaked={} title={:?})\n       tab={tab_hwnd:?} ({tab_class}) root={root:?} browser.Visible={visible_prop:?}\n       name={name:?} url={url:?}\n       selected={sel:?}",
                class(frame_h),
                IsWindowVisible(frame_h).as_bool(),
                IsIconic(frame_h).as_bool(),
                cloaked(frame_h),
                title(frame_h),
            );
        }

        println!("\n== get_active_explorer_info()");
        match cdout_core::features::explorer::get_active_explorer_info() {
            Ok(s) => println!("  path={:?}\n  files={:?}", s.path, s.selected_files),
            Err(e) => println!("  error: {e}"),
        }
    }
}
