//! Windows Explorer: which folder is open and which files are selected.
//!
//! Explorer exposes every open tab as an entry of the `ShellWindows` COM
//! collection, but says nothing about which window the user was just in or
//! which tab of it is showing. Both are recovered from the window manager:
//!
//! 1. **Window.** Top-level windows are enumerated with `EnumWindows`, which
//!    returns them in Z-order. The first `CabinetWClass` window that is
//!    visible, not minimised and not cloaked (another virtual desktop) is the
//!    Explorer window the user most recently used. An earlier version walked
//!    the Z-order by hand with a cap of 200 windows; a busy desktop has
//!    thousands of top-level windows (most invisible), so it regularly gave
//!    up before reaching Explorer at all.
//! 2. **Tab.** Each tab owns a `ShellTabWindowClass` child window; all of
//!    them stay visible and share one rectangle, but Explorer raises the
//!    active tab's window to the top of the sibling Z-order. The tab whose
//!    child is topmost is the active one (verified against UI Automation's
//!    selected tab item). The window *title* is only a fallback: Explorer
//!    leaves the titles of background windows stale, so it names the tab
//!    that was active the last time the window had focus, not the current
//!    one, and substring matching against it picked wrong tabs.
//!
//! The selection reported is exactly the active tab's, even when empty. A
//! selection in another tab is never substituted.

use std::borrow::Cow;

use serde::Serialize;
use windows::core::{ComInterface, Result};
use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
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
    EnumWindows, GetAncestor, GetClassNameW, GetForegroundWindow, GetWindow, GetWindowTextW,
    IsIconic, IsWindowVisible, GA_ROOT, GW_CHILD, GW_HWNDNEXT,
};

use super::ExplorerState;

const FRAME_CLASS: &str = "CabinetWClass";
const TAB_CLASS: &str = "ShellTabWindowClass";

// --- Diagnostics payload ---

/// One top-level Explorer window, in Z-order position.
#[derive(Serialize, Debug, Clone)]
pub struct FrameInfo {
    pub hwnd: String,
    pub title: String,
    pub visible: bool,
    pub minimized: bool,
    /// Hidden by DWM, typically because it lives on another virtual desktop.
    pub cloaked: bool,
    pub is_foreground: bool,
    /// Visible, not minimised, not cloaked: eligible to be the target.
    pub usable: bool,
}

/// One `ShellWindows` entry belonging to Explorer.
#[derive(Serialize, Debug, Clone)]
pub struct TabInfo {
    pub index: i32,
    pub frame_hwnd: String,
    pub tab_hwnd: String,
    /// Position of the tab's child window among its siblings, 0 = topmost =
    /// active. `None` when the child window could not be found.
    pub z_index: Option<u32>,
    pub location_name: String,
    pub location_url: String,
    /// Filesystem path of the location, when it has one.
    pub path: Option<String>,
    pub selected_count: usize,
    pub in_target_frame: bool,
    pub is_active: bool,
}

#[derive(Serialize, Debug)]
pub struct ExplorerDebugInfo {
    pub os: String,
    pub foreground_hwnd: String,
    pub foreground_class: String,
    pub foreground_title: String,
    pub is_explorer_window: bool,
    /// Every Explorer window, top of the Z-order first.
    pub frames: Vec<FrameInfo>,
    pub target_explorer_hwnd: String,
    pub target_explorer_title: String,
    pub tabs: Vec<TabInfo>,
    /// How the active tab was chosen: `topmost-tab`, `title-prefix`,
    /// `first-tab`, `no-tabs` or `no-explorer-window`.
    pub resolution: String,
    pub resolved_path: String,
    pub selected_count: usize,
    pub selection: Vec<String>,
}

// --- Pure decision logic (unit-tested) ---

/// What the resolver knows about a tab without any handles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TabCandidate {
    pub z_index: Option<u32>,
    pub location_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Resolution {
    TopmostTab,
    TitlePrefix,
    FirstTab,
    NoTabs,
    NoExplorerWindow,
}

impl Resolution {
    fn as_str(self) -> &'static str {
        match self {
            Resolution::TopmostTab => "topmost-tab",
            Resolution::TitlePrefix => "title-prefix",
            Resolution::FirstTab => "first-tab",
            Resolution::NoTabs => "no-tabs",
            Resolution::NoExplorerWindow => "no-explorer-window",
        }
    }
}

/// Does the window title name this tab? Titles look like `Downloads`,
/// `Downloads - File Explorer` or `Downloads and 7 more tabs - File Explorer`
/// (the connecting words are localised, so only the leading name is used):
/// the tab name must be the title's prefix and end at a word boundary.
pub(crate) fn title_names_tab(title: &str, location_name: &str) -> bool {
    if location_name.is_empty() {
        return false;
    }
    let title = title.to_lowercase();
    let name = location_name.to_lowercase();
    match title.strip_prefix(&name) {
        Some(rest) => rest.chars().next().is_none_or(|c| !c.is_alphanumeric()),
        None => false,
    }
}

/// Pick the active tab among the target window's tabs.
pub(crate) fn choose_active_tab(
    tabs: &[TabCandidate],
    frame_title: &str,
) -> (Option<usize>, Resolution) {
    if tabs.is_empty() {
        return (None, Resolution::NoTabs);
    }
    // 1. The tab whose child window is topmost among its siblings.
    let topmost = tabs
        .iter()
        .enumerate()
        .filter_map(|(i, t)| t.z_index.map(|z| (z, i)))
        .min_by_key(|(z, i)| (*z, *i));
    if let Some((_, i)) = topmost {
        return (Some(i), Resolution::TopmostTab);
    }
    // 2. The longest tab name the (possibly stale) title starts with.
    let by_title = tabs
        .iter()
        .enumerate()
        .filter(|(_, t)| title_names_tab(frame_title, &t.location_name))
        .max_by_key(|(i, t)| (t.location_name.chars().count(), usize::MAX - *i));
    if let Some((i, _)) = by_title {
        return (Some(i), Resolution::TitlePrefix);
    }
    // 3. Anything rather than nothing, but never another tab's selection
    //    presented as this one's.
    (Some(0), Resolution::FirstTab)
}

/// `file:///C:/a/b%20c` → `C:\a\b c`; `file://server/share/x` →
/// `\\server\share\x`. Anything that is not a `file:` URL has no path
/// (search results, Home, This PC, control panel pages).
pub(crate) fn file_url_to_path(url: &str) -> Option<String> {
    let rest = url.strip_prefix("file:")?;
    let rest = rest.strip_prefix("//")?;
    let raw = if let Some(local) = rest.strip_prefix('/') {
        local.replace('/', "\\")
    } else {
        // Host-qualified: a UNC path.
        format!("\\\\{}", rest.replace('/', "\\"))
    };
    let decoded = urlencoding::decode(&raw)
        .unwrap_or(Cow::Borrowed(&raw))
        .to_string();
    if decoded.is_empty() {
        None
    } else {
        Some(decoded)
    }
}

/// Working directory for a selection made in a view that has no folder of
/// its own (search results): the parent of the first selected item.
pub(crate) fn parent_of_first(selection: &[String]) -> Option<String> {
    let first = selection.first()?;
    std::path::Path::new(first)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .filter(|p| !p.is_empty())
}

// --- Window manager helpers ---

fn window_class(hwnd: HWND) -> String {
    // SAFETY: read-only query into a fixed-size stack buffer.
    unsafe {
        let mut buf = [0u16; 256];
        let len = GetClassNameW(hwnd, &mut buf) as usize;
        String::from_utf16_lossy(&buf[..len])
    }
}

fn window_title(hwnd: HWND) -> String {
    // SAFETY: read-only query into a fixed-size stack buffer.
    unsafe {
        let mut buf = [0u16; 512];
        let len = GetWindowTextW(hwnd, &mut buf) as usize;
        String::from_utf16_lossy(&buf[..len]).trim().to_string()
    }
}

fn is_cloaked(hwnd: HWND) -> bool {
    let mut cloaked: u32 = 0;
    // SAFETY: DWM writes a u32 into the buffer we size for it.
    let ok = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut u32 as *mut _,
            std::mem::size_of::<u32>() as u32,
        )
    };
    ok.is_ok() && cloaked != 0
}

unsafe extern "system" fn push_hwnd(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: lparam is the `*mut Vec<HWND>` passed by `top_level_windows`,
    // alive for the duration of the EnumWindows call.
    let list = &mut *(lparam.0 as *mut Vec<HWND>);
    list.push(hwnd);
    BOOL(1)
}

/// All top-level windows, top of the Z-order first.
fn top_level_windows() -> Vec<HWND> {
    let mut list: Vec<HWND> = Vec::new();
    // SAFETY: the callback only pushes into the Vec whose address it is given.
    unsafe {
        let _ = EnumWindows(Some(push_hwnd), LPARAM(&mut list as *mut _ as isize));
    }
    list
}

/// Explorer frame windows in Z-order, with the flags that decide eligibility.
fn explorer_frames(foreground: HWND) -> Vec<(HWND, FrameInfo)> {
    top_level_windows()
        .into_iter()
        .filter(|h| window_class(*h) == FRAME_CLASS)
        .map(|h| {
            // SAFETY: read-only window state queries on a live HWND.
            let (visible, minimized) =
                unsafe { (IsWindowVisible(h).as_bool(), IsIconic(h).as_bool()) };
            let cloaked = is_cloaked(h);
            let info = FrameInfo {
                hwnd: format!("{}", h.0),
                title: window_title(h),
                visible,
                minimized,
                cloaked,
                is_foreground: h == foreground,
                usable: visible && !minimized && !cloaked,
            };
            (h, info)
        })
        .collect()
}

/// The frame's tab child windows, topmost first.
fn tab_windows_in_z_order(frame: HWND) -> Vec<HWND> {
    let mut tabs = Vec::new();
    // SAFETY: walking direct children of a live window with read-only calls.
    unsafe {
        let mut child = GetWindow(frame, GW_CHILD);
        let mut guard = 0;
        while child.0 != 0 && guard < 4096 {
            if window_class(child) == TAB_CLASS {
                tabs.push(child);
            }
            child = GetWindow(child, GW_HWNDNEXT);
            guard += 1;
        }
    }
    tabs
}

// --- COM helpers ---

fn index_variant(i: i32) -> VARIANT {
    let mut v = VARIANT::default();
    // SAFETY: setting the discriminant before the matching union field.
    unsafe {
        (*v.Anonymous.Anonymous).vt = VT_I4;
        (*v.Anonymous.Anonymous).Anonymous.lVal = i;
    }
    v
}

/// The tab's own `ShellTabWindowClass` window, via the browser's top-level
/// `IShellBrowser`.
fn tab_window(browser: &IWebBrowserApp) -> Option<HWND> {
    // SAFETY: COM calls on interfaces obtained through QueryInterface.
    unsafe {
        let sp: IServiceProvider = browser.cast().ok()?;
        let sb: IShellBrowser = sp.QueryService(&SID_STopLevelBrowser).ok()?;
        sb.GetWindow().ok().filter(|h| h.0 != 0)
    }
}

fn selection(browser: &IWebBrowserApp) -> Vec<String> {
    let mut out = Vec::new();
    // SAFETY: COM calls; VARIANT indices are built with the right vt.
    unsafe {
        let Ok(doc) = browser.Document() else {
            return out;
        };
        let Ok(view) = doc.cast::<IShellFolderViewDual>() else {
            return out;
        };
        let Ok(items) = view.SelectedItems() else {
            return out;
        };
        let count = items.Count().unwrap_or(0);
        for j in 0..count {
            if let Ok(item) = items.Item(index_variant(j)) {
                if let Ok(p) = item.Path() {
                    let p = p.to_string();
                    if !p.is_empty() {
                        out.push(p);
                    }
                }
            }
        }
    }
    out
}

struct TabRecord {
    info: TabInfo,
    selection: Vec<String>,
}

/// Every Explorer tab, with its frame and Z-order position.
fn explorer_tabs() -> Result<Vec<TabRecord>> {
    // SAFETY: COM object creation and enumeration on an initialised thread.
    unsafe {
        let shell_windows: IShellWindows =
            CoCreateInstance(&ShellWindows, None, CLSCTX_LOCAL_SERVER)?;
        let count = shell_windows.Count()?;
        let mut z_orders: std::collections::HashMap<isize, Vec<HWND>> = Default::default();
        let mut tabs = Vec::new();

        for i in 0..count {
            let Ok(disp) = shell_windows.Item(index_variant(i)) else {
                continue;
            };
            let Ok(browser) = disp.cast::<IWebBrowserApp>() else {
                continue;
            };
            let full_name = browser
                .FullName()
                .map(|s| s.to_string().to_lowercase())
                .unwrap_or_default();
            if !full_name.ends_with("explorer.exe") {
                continue;
            }

            let tab = tab_window(&browser);
            let frame = match tab {
                Some(t) => GetAncestor(t, GA_ROOT),
                None => HWND(browser.HWND().map(|h| h.0).unwrap_or(0)),
            };
            let z_index = tab.and_then(|t| {
                let order = z_orders
                    .entry(frame.0)
                    .or_insert_with(|| tab_windows_in_z_order(frame));
                order.iter().position(|h| *h == t).map(|p| p as u32)
            });
            let location_name = browser
                .LocationName()
                .map(|s| s.to_string())
                .unwrap_or_default();
            let location_url = browser
                .LocationURL()
                .map(|s| s.to_string())
                .unwrap_or_default();
            let selection = selection(&browser);

            tabs.push(TabRecord {
                info: TabInfo {
                    index: i,
                    frame_hwnd: format!("{}", frame.0),
                    tab_hwnd: tab.map(|h| format!("{}", h.0)).unwrap_or_default(),
                    z_index,
                    path: file_url_to_path(&location_url),
                    location_name,
                    location_url,
                    selected_count: selection.len(),
                    in_target_frame: false,
                    is_active: false,
                },
                selection,
            });
        }
        Ok(tabs)
    }
}

// --- Resolution ---

struct Scan {
    foreground: HWND,
    frames: Vec<(HWND, FrameInfo)>,
    target: Option<HWND>,
    tabs: Vec<TabRecord>,
    resolution: Resolution,
    state: ExplorerState,
}

fn scan() -> Result<Scan> {
    // SAFETY: per-thread COM initialisation; a repeat call on an already
    // initialised thread is harmless.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    // SAFETY: read-only query.
    let foreground = unsafe { GetForegroundWindow() };
    let frames = explorer_frames(foreground);

    // The foreground Explorer window if there is one, else the topmost
    // usable one (the most recently active Explorer window).
    let target = frames
        .iter()
        .find(|(h, f)| f.usable && *h == foreground)
        .or_else(|| frames.iter().find(|(_, f)| f.usable))
        .map(|(h, _)| *h);

    let Some(target) = target else {
        return Ok(Scan {
            foreground,
            frames,
            target: None,
            tabs: Vec::new(),
            resolution: Resolution::NoExplorerWindow,
            state: ExplorerState::default(),
        });
    };

    let mut tabs = explorer_tabs()?;
    let target_key = format!("{}", target.0);
    for t in &mut tabs {
        t.info.in_target_frame = t.info.frame_hwnd == target_key;
    }
    let in_target: Vec<usize> = (0..tabs.len())
        .filter(|i| tabs[*i].info.in_target_frame)
        .collect();
    let candidates: Vec<TabCandidate> = in_target
        .iter()
        .map(|i| TabCandidate {
            z_index: tabs[*i].info.z_index,
            location_name: tabs[*i].info.location_name.clone(),
        })
        .collect();
    let title = window_title(target);
    let (chosen, resolution) = choose_active_tab(&candidates, &title);
    let active = chosen.map(|c| in_target[c]);

    let state = match active {
        Some(i) => {
            tabs[i].info.is_active = true;
            let mut selected_files = tabs[i].selection.clone();
            // Case-insensitive natural order, as Explorer displays a folder,
            // so the numbered list handed to the model matches what the user
            // is looking at.
            selected_files.sort_by(|a, b| natord::compare_ignore_case(a, b));
            let path = tabs[i]
                .info
                .path
                .clone()
                .or_else(|| parent_of_first(&selected_files))
                .unwrap_or_default();
            ExplorerState {
                path,
                selected_files,
            }
        }
        None => ExplorerState::default(),
    };

    Ok(Scan {
        foreground,
        frames,
        target: Some(target),
        tabs,
        resolution,
        state,
    })
}

pub fn get_active_explorer_info() -> Result<ExplorerState> {
    Ok(scan()?.state)
}

pub fn get_explorer_debug_info() -> Result<ExplorerDebugInfo> {
    let s = scan()?;
    let foreground_class = window_class(s.foreground);
    let target_title = s
        .target
        .and_then(|t| s.frames.iter().find(|(h, _)| *h == t))
        .map(|(_, f)| f.title.clone())
        .unwrap_or_default();
    Ok(ExplorerDebugInfo {
        os: "windows".to_string(),
        foreground_hwnd: format!("{}", s.foreground.0),
        is_explorer_window: foreground_class == FRAME_CLASS,
        foreground_class,
        foreground_title: window_title(s.foreground),
        frames: s.frames.iter().map(|(_, f)| f.clone()).collect(),
        target_explorer_hwnd: s.target.map(|t| format!("{}", t.0)).unwrap_or_default(),
        target_explorer_title: target_title,
        tabs: s.tabs.iter().map(|t| t.info.clone()).collect(),
        resolution: s.resolution.as_str().to_string(),
        resolved_path: s.state.path.clone(),
        selected_count: s.state.selected_files.len(),
        selection: s.state.selected_files.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(name: &str, z: Option<u32>) -> TabCandidate {
        TabCandidate {
            z_index: z,
            location_name: name.into(),
        }
    }

    #[test]
    fn title_prefix_needs_a_word_boundary() {
        let title = "Screenshots and 7 more tabs - File Explorer";
        assert!(title_names_tab(title, "Screenshots"));
        assert!(title_names_tab(title, "screenshots"));
        assert!(!title_names_tab(title, "Screen"), "prefix of a longer word");
        assert!(!title_names_tab(title, "and"), "substring, not prefix");
        assert!(!title_names_tab(title, "File Explorer"));
        assert!(!title_names_tab(title, ""));
        assert!(title_names_tab("Downloads", "Downloads"));
        assert!(title_names_tab("Downloads - File Explorer", "Downloads"));
        assert!(title_names_tab(
            "hires_ridge et 2 autres onglets",
            "hires_ridge"
        ));
    }

    #[test]
    fn topmost_tab_wins_even_when_the_title_is_stale() {
        // The user's report: Downloads is showing, the background window's
        // title still says Screenshots.
        let tabs = [
            tab("Screenshots", Some(7)),
            tab("memory", Some(1)),
            tab("Downloads", Some(0)),
        ];
        let (i, how) = choose_active_tab(&tabs, "Screenshots and 7 more tabs - File Explorer");
        assert_eq!((i, how), (Some(2), Resolution::TopmostTab));
    }

    #[test]
    fn title_fallback_prefers_the_longest_name() {
        let tabs = [
            tab("hires", None),
            tab("hires_ridge_crops", None),
            tab("zips", None),
        ];
        let (i, how) =
            choose_active_tab(&tabs, "hires_ridge_crops and 2 more tabs - File Explorer");
        assert_eq!((i, how), (Some(1), Resolution::TitlePrefix));
    }

    #[test]
    fn duplicate_names_resolve_to_the_first_without_z_order() {
        let tabs = [tab("a", None), tab("hires", None), tab("hires", None)];
        let (i, how) = choose_active_tab(&tabs, "hires - File Explorer");
        assert_eq!((i, how), (Some(1), Resolution::TitlePrefix));
    }

    #[test]
    fn unmatched_title_falls_back_to_the_first_tab() {
        let tabs = [tab("a", None), tab("b", None)];
        assert_eq!(
            choose_active_tab(&tabs, "File Explorer"),
            (Some(0), Resolution::FirstTab)
        );
        assert_eq!(choose_active_tab(&[], "x"), (None, Resolution::NoTabs));
    }

    #[test]
    fn mixed_z_knowledge_still_prefers_z_order() {
        let tabs = [tab("a", None), tab("b", Some(3)), tab("c", Some(1))];
        assert_eq!(
            choose_active_tab(&tabs, "a - File Explorer"),
            (Some(2), Resolution::TopmostTab)
        );
    }

    #[test]
    fn file_urls_become_paths() {
        assert_eq!(
            file_url_to_path("file:///C:/Users/home/Downloads").as_deref(),
            Some("C:\\Users\\home\\Downloads")
        );
        assert_eq!(
            file_url_to_path("file:///D:/a%20b/caf%C3%A9").as_deref(),
            Some("D:\\a b\\café")
        );
        assert_eq!(
            file_url_to_path("file://nas/share/clips").as_deref(),
            Some("\\\\nas\\share\\clips")
        );
        assert_eq!(file_url_to_path(""), None);
        assert_eq!(file_url_to_path("search-ms:query=pdf"), None);
        assert_eq!(
            file_url_to_path("::{20D04FE0-3AEA-1069-A2D8-08002B30309D}"),
            None
        );
    }

    #[test]
    fn search_results_use_the_first_items_folder() {
        let sel = vec!["C:\\Users\\home\\Downloads\\a.pdf".to_string()];
        assert_eq!(
            parent_of_first(&sel).as_deref(),
            Some("C:\\Users\\home\\Downloads")
        );
        assert_eq!(parent_of_first(&[]), None);
    }
}
