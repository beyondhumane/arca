// Embeds the icon into the executable, so the taskbar and the Explorer show the
// brand instead of the generic Windows one, and the files the installer carries.
//
// The files come from ARCA_PAYLOAD, a zip that windows/build-setup.ps1 makes out
// of the release binaries. Without it the installer is built empty on purpose:
// a plain `cargo build` has nothing to install and says so when asked to.
use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=ARCA_PAYLOAD");
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR")).join("payload.zip");
    match env::var_os("ARCA_PAYLOAD") {
        Some(path) => {
            println!("cargo:rerun-if-changed={}", PathBuf::from(&path).display());
            fs::copy(&path, &out).expect("copy the installer payload");
        }
        None => fs::write(&out, b"").expect("write an empty payload"),
    }

    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=../brand/arca-monolito.ico");
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../brand/arca-monolito.ico");
        res.set("FileDescription", "Arca setup");
        res.set("ProductName", "Arca");
        if let Err(e) = res.compile() {
            println!("cargo:warning=icon not embedded: {e}");
        }
    }
}
