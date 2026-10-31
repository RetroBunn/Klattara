//! KlattPad: type text and hear Klattara speak it.
//!
//! It is a plain Windows program built from standard controls, so screen readers
//! can read all of it. What you type goes straight to the engine, commands and all,
//! so phoneme input and singing work here.

#![cfg_attr(windows, windows_subsystem = "windows")]

mod document;
mod speech;

#[cfg(windows)]
mod player;
#[cfg(windows)]
mod window;

#[cfg(windows)]
fn main() {
    window::run();
}

#[cfg(not(windows))]
fn main() {
    eprintln!("KlattPad runs on Windows.");
}
