//! Small JSON settings file: definitions folder and recent files.
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct Settings {
    pub definitions_dir: Option<PathBuf>,
    pub recent: Vec<PathBuf>,
    /// the game's saved-vehicles folder (detected or chosen)
    pub vehicles_dir: Option<PathBuf>,
}

fn file() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("keel").join("settings.json"))
}

impl Settings {
    pub fn load() -> Self {
        let Some(f) = file() else { return Settings::default() };
        let Ok(v) = std::fs::read_to_string(f).map(|s| serde_json::from_str::<Value>(&s).unwrap_or(Value::Null)) else {
            return Settings::default();
        };
        Settings {
            definitions_dir: v["definitions_dir"].as_str().map(PathBuf::from),
            vehicles_dir: v["vehicles_dir"].as_str().map(PathBuf::from),
            recent: v["recent"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(PathBuf::from)).collect()).unwrap_or_default(),
        }
    }
    pub fn save(&self) {
        let Some(f) = file() else { return };
        let _ = std::fs::create_dir_all(f.parent().unwrap());
        let v = json!({
            "definitions_dir": self.definitions_dir.as_ref().map(|p| p.display().to_string()),
            "vehicles_dir": self.vehicles_dir.as_ref().map(|p| p.display().to_string()),
            "recent": self.recent.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
        });
        let _ = std::fs::write(f, serde_json::to_string_pretty(&v).unwrap());
    }
    pub fn push_recent(&mut self, p: &Path) {
        let p = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        self.recent.retain(|x| x != &p);
        self.recent.insert(0, p);
        self.recent.truncate(10);
        self.save();
    }
}
