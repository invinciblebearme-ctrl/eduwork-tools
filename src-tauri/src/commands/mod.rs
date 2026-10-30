pub mod excel;
pub mod files;
use serde::Serialize;

#[derive(Serialize)]
pub struct AppInfo {
    name: &'static str,
    version: &'static str,
    phase: &'static str,
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo {
        name: "EduWork Tools",
        version: env!("CARGO_PKG_VERSION"),
        phase: "phase-5",
    }
}
