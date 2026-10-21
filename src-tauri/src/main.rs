// Prevents an extra console window on Windows release builds. Harmless on Linux.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    pve_desk_lib::run()
}
