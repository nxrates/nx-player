use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

/// State managed by Tauri
pub struct ExtensionHostState(pub Mutex<ExtensionHost>);

pub struct ExtensionHost {
    extensions_dir: PathBuf,
    manifests: HashMap<String, ExtensionManifest>,
    processes: HashMap<String, Child>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionManifest {
    pub id: String,
    pub name: String,
    pub binary: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub settings: Vec<ExtensionSettingDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionSettingDef {
    pub key: String,
    #[serde(rename = "type")]
    pub setting_type: String,
    pub label: String,
    #[serde(default)]
    pub default: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub running: bool,
    pub settings: Vec<ExtensionSettingDef>,
}

/// JSON-lines protocol messages
#[derive(Debug, Serialize, Deserialize)]
pub struct ExtMessage {
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

impl ExtensionHost {
    pub fn new(extensions_dir: PathBuf) -> Self {
        std::fs::create_dir_all(&extensions_dir).ok();
        let mut host = Self {
            extensions_dir,
            manifests: HashMap::new(),
            processes: HashMap::new(),
        };
        host.discover();
        host
    }

    /// Scan extensions directory for manifest.json files
    pub fn discover(&mut self) {
        self.manifests.clear();
        let Ok(entries) = std::fs::read_dir(&self.extensions_dir) else {
            return;
        };
        for entry in entries.flatten() {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            let manifest_path = dir.join("manifest.json");
            if !manifest_path.exists() {
                continue;
            }
            if let Ok(data) = std::fs::read_to_string(&manifest_path) {
                if let Ok(manifest) = serde_json::from_str::<ExtensionManifest>(&data) {
                    self.manifests.insert(manifest.id.clone(), manifest);
                }
            }
        }
    }

    pub fn list(&self) -> Vec<ExtensionInfo> {
        self.manifests
            .values()
            .map(|m| ExtensionInfo {
                id: m.id.clone(),
                name: m.name.clone(),
                description: m.description.clone(),
                version: m.version.clone(),
                running: self.processes.contains_key(&m.id),
                settings: m.settings.clone(),
            })
            .collect()
    }

    pub fn start(&mut self, id: &str) -> Result<(), String> {
        if self.processes.contains_key(id) {
            return Ok(());
        }
        let manifest = self.manifests.get(id).ok_or("Extension not found")?;
        let bin_path = self
            .extensions_dir
            .join(&manifest.id)
            .join(&manifest.binary);
        if !bin_path.exists() {
            return Err(format!("Binary not found: {}", bin_path.display()));
        }
        let child = Command::new(&bin_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to start extension: {}", e))?;
        self.processes.insert(id.to_string(), child);
        Ok(())
    }

    pub fn stop(&mut self, id: &str) {
        if let Some(mut child) = self.processes.remove(id) {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// Send a message to an extension and read the response (blocking)
    pub fn send_recv(&mut self, id: &str, msg: &ExtMessage) -> Result<ExtMessage, String> {
        let child = self.processes.get_mut(id).ok_or("Extension not running")?;
        let stdin = child.stdin.as_mut().ok_or("No stdin")?;
        let stdout = child.stdout.as_mut().ok_or("No stdout")?;

        let json = serde_json::to_string(msg).map_err(|e| e.to_string())?;
        writeln!(stdin, "{}", json).map_err(|e| e.to_string())?;
        stdin.flush().map_err(|e| e.to_string())?;

        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        reader.read_line(&mut line).map_err(|e| e.to_string())?;

        serde_json::from_str(line.trim()).map_err(|e| e.to_string())
    }

    pub fn install(&mut self, archive_path: &Path) -> Result<String, String> {
        let manifest_path = archive_path.join("manifest.json");
        if !manifest_path.exists() {
            return Err("No manifest.json found".to_string());
        }
        let data = std::fs::read_to_string(&manifest_path).map_err(|e| e.to_string())?;
        let manifest: ExtensionManifest =
            serde_json::from_str(&data).map_err(|e| e.to_string())?;
        let dest = self.extensions_dir.join(&manifest.id);
        if dest.exists() {
            return Err("Extension already installed".to_string());
        }
        copy_dir_recursive(archive_path, &dest).map_err(|e| e.to_string())?;
        self.manifests.insert(manifest.id.clone(), manifest.clone());
        Ok(manifest.id)
    }

    pub fn uninstall(&mut self, id: &str) -> Result<(), String> {
        self.stop(id);
        self.manifests.remove(id);
        let dir = self.extensions_dir.join(id);
        if dir.exists() {
            std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn stop_all(&mut self) {
        let ids: Vec<String> = self.processes.keys().cloned().collect();
        for id in ids {
            self.stop(&id);
        }
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let dest_path = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &dest_path)?;
        } else {
            std::fs::copy(entry.path(), dest_path)?;
        }
    }
    Ok(())
}
