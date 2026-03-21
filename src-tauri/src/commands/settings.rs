use tauri::State;

use crate::db::{self, DbState};
use crate::models::Settings;

#[tauri::command]
pub fn get_settings(db_state: State<'_, DbState>) -> Result<Settings, String> {
    let conn = db_state.0.lock().map_err(|e| e.to_string())?;
    db::get_settings(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_settings(
    settings: Settings,
    db_state: State<'_, DbState>,
) -> Result<(), String> {
    let conn = db_state.0.lock().map_err(|e| e.to_string())?;
    db::update_settings(&conn, &settings).map_err(|e| e.to_string())
}
