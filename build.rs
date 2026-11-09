//! Windows: the logo goes into the exe as its icon resource, so Explorer, a
//! shortcut and a pinned taskbar entry show it before the app has run. The
//! open window sets its own (`main.rs`); macOS takes the Dock icon at runtime.

fn main() {
    println!("cargo:rerun-if-changed=assets/icon/synapso.ico");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("assets/icon/synapso.ico")
            .compile()
            .expect("embed the Windows icon");
    }
}
