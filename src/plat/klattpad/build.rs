//! Embeds the manifest, which gives KlattPad the current look of Windows controls
//! and sharp text on high-resolution screens.
//!
//! With Microsoft's linker the linker embeds it itself. With the GNU tools (MinGW,
//! as when building on a Mac or Linux) the manifest goes in as a resource,
//! compiled by the target's windres: <target>-windres, or WINDRES if set.

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=klattpad.manifest");
    println!("cargo:rerun-if-changed=klattpad.rc");
    println!("cargo:rerun-if-env-changed=WINDRES");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let folder = env::var("CARGO_MANIFEST_DIR").expect("cargo sets the manifest folder");
    if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-bins=/MANIFESTINPUT:{folder}\\klattpad.manifest");
        return;
    }
    let arch = env::var("CARGO_CFG_TARGET_ARCH").expect("cargo sets the architecture");
    let triple = match arch.as_str() {
        "x86" => "i686-w64-mingw32".to_string(),
        other => format!("{other}-w64-mingw32"),
    };
    let windres = env::var("WINDRES").unwrap_or_else(|_| format!("{triple}-windres"));
    let out = PathBuf::from(env::var("OUT_DIR").expect("cargo sets OUT_DIR")).join("klattpad-manifest.o");
    let status = Command::new(&windres)
        .current_dir(&folder)
        .args(["-O", "coff", "-i", "klattpad.rc", "-o"])
        .arg(&out)
        .status();
    match status {
        Ok(s) if s.success() => println!("cargo:rustc-link-arg-bins={}", out.display()),
        _ => println!("cargo:warning={windres} not found or failed: KlattPad builds without its manifest"),
    }
}
