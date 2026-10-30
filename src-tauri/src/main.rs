#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod gui;

fn main() {
    if let Err(error) = gui::run() {
        // Deliberately do not print backend output, launch URLs or arbitrary OS errors.
        eprintln!("Hermes Desktop Rust: {error}");
        std::process::exit(1);
    }
}
