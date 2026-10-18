// No console window for release builds on Windows. Debug builds keep it so
// panics and logs reach the terminal (and the smoke script's stderr capture).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    cdout::run();
}
