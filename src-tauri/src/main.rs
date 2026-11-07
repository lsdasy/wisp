// В релизе на Windows не открывать консольное окно.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    lumen_app::run();
}
