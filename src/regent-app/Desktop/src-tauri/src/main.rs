// Prevents an extra console window on Windows in release — the shell owns a
// hidden deacon and must never flash its own console either.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    regent_desktop_lib::run();
}
