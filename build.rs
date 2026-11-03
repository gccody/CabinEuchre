//! Two jobs at build time:
//!
//! - Refuses a release build whose online play couldn't work for players (see
//!   [`check_release_server`]).
//! - Puts the icon and the game's name into the Windows `.exe`, which is what
//!   Explorer, the taskbar and Task Manager show. Phones get their icon from
//!   `[package.metadata.love2drs]` in `Cargo.toml` instead.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=EUCHRE_SERVER");
    println!("cargo:rerun-if-env-changed=EUCHRE_ALLOW_LOCAL_SERVER");
    check_release_server();
    #[cfg(windows)]
    windows_resources();
}

/// A release build with no `EUCHRE_SERVER` uses the live server (`DEFAULT_SERVER`
/// in `src/net/online.rs`), so forgetting it is fine. Setting it to something a
/// player can't reach isn't: nothing would look wrong until online play failed for
/// everyone. To try a release build locally, set `EUCHRE_SERVER` when running it
/// (that isn't checked), or build with `EUCHRE_ALLOW_LOCAL_SERVER=1`.
fn check_release_server() {
    if std::env::var("PROFILE").as_deref() != Ok("release") || std::env::var_os("EUCHRE_ALLOW_LOCAL_SERVER").is_some() {
        return;
    }
    let Ok(server) = std::env::var("EUCHRE_SERVER") else { return };
    let local = ["localhost", "127.0.0.1", "0.0.0.0", "[::1]"].iter().any(|host| server.contains(host));
    if !server.starts_with("wss://") || local {
        panic!(
            "\n\nThis release build would ship with EUCHRE_SERVER={server}, which players can't use safely.\n\
             A release needs a public wss:// address. Unset EUCHRE_SERVER to use the live server, or set\n\
             EUCHRE_ALLOW_LOCAL_SERVER=1 if this build will never leave your machine.\n\n"
        );
    }
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
