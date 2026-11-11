// Hide the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    klavye_studio::migrate::move_old_data();
    klavye_studio::run();
}
