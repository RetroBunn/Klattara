//! Embeds the manifest, which gives KlattPad the current look of Windows controls
//! and sharp text on high-resolution screens.

fn main() {
    println!("cargo:rerun-if-changed=klattpad.manifest");
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        let folder = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets the manifest folder");
        println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-bins=/MANIFESTINPUT:{folder}\\klattpad.manifest");
    }
}
