//! Embed the app icon and version info into the Windows executable, so
//! Explorer, the taskbar and Alt+Tab show the cdout icon.

fn main() {
    println!("cargo:rerun-if-changed=../../icons/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("../../icons/icon.ico")
        .set("ProductName", "cdout")
        .set("FileDescription", "cdout")
        .set("CompanyName", "cdout")
        .set("LegalCopyright", "MIT");
    if let Err(e) = res.compile() {
        panic!("embedding the Windows icon failed: {e}");
    }
}
