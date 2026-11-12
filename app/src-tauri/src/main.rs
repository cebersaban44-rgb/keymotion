// Hide the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    keymotion_lib::migrate::move_old_data();
    keymotion_lib::run();
}
