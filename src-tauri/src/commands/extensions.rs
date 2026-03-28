use tauri::State;

use super::LockExt;
use crate::extensions::{ExtMessage, ExtensionHostState, ExtensionInfo};

#[tauri::command]
pub fn list_extensions(host: State<'_, ExtensionHostState>) -> Result<Vec<ExtensionInfo>, String> {
    let host = host.0.acquire()?;
    Ok(host.list())
}

#[tauri::command]
pub fn install_extension(
    path: String,
    host: State<'_, ExtensionHostState>,
) -> Result<String, String> {
    let mut host = host.0.acquire()?;
    host.install(std::path::Path::new(&path))
}

#[tauri::command]
pub fn uninstall_extension(
    id: String,
    host: State<'_, ExtensionHostState>,
) -> Result<(), String> {
    let mut host = host.0.acquire()?;
    host.uninstall(&id)
}

#[tauri::command]
pub fn start_extension(id: String, host: State<'_, ExtensionHostState>) -> Result<(), String> {
    let mut host = host.0.acquire()?;
    host.start(&id)
}

#[tauri::command]
pub fn stop_extension(id: String, host: State<'_, ExtensionHostState>) -> Result<(), String> {
    let mut host = host.0.acquire()?;
    host.stop(&id);
    Ok(())
}

#[tauri::command]
pub fn extension_search(
    id: String,
    query: String,
    host: State<'_, ExtensionHostState>,
) -> Result<serde_json::Value, String> {
    let mut host = host.0.acquire()?;
    // Start if not running
    if !host.list().iter().any(|e| e.id == id && e.running) {
        host.start(&id)?;
    }
    let msg = ExtMessage {
        method: "search".to_string(),
        params: serde_json::json!({ "query": query }),
    };
    let resp = host.send_recv(&id, &msg)?;
    Ok(resp.params)
}
