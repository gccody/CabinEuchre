//! Puts the icon and the game's name into the Windows `.exe`, which is what
//! Explorer, the taskbar and Task Manager show. Phones get their icon from
//! `[package.metadata.love2drs]` in `Cargo.toml` instead.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    #[cfg(windows)]
    windows_resources();
}

/// Only a Windows host can build these, since `rc.exe` comes with the Windows SDK.
/// A Windows host building for a phone skips them.
#[cfg(windows)]
fn windows_resources() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=Cargo.toml");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut resource = winresource::WindowsResource::new();
    resource
        .set_icon("assets/icon.ico")
        .set("FileDescription", "Cabin Euchre")
        .set("ProductName", "Cabin Euchre");
    // Fail loudly: a build that quietly loses its icon could ship that way.
    resource.compile().expect("can't embed the icon in the .exe");
}
