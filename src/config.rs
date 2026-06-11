use std::{fs, path::PathBuf, time::Duration};

use serde::{Deserialize, Serialize};

use crate::app::{SortKey, Tab};

const CONFIG_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preferences {
    version: u32,
    pub tab: String,
    pub sort_key: String,
    pub sort_desc: bool,
    pub show_details: bool,
    pub overview_visible: bool,
    pub interval_ms: u64,
    pub filter: String,
    pub compact_mode: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            tab: "Cpu".to_string(),
            sort_key: "Cpu".to_string(),
            sort_desc: true,
            show_details: true,
            overview_visible: true,
            interval_ms: 1_000,
            filter: String::new(),
            compact_mode: false,
        }
    }
}

impl Preferences {
    pub fn load() -> Self {
        let path = match config_path() {
            Some(p) => p,
            None => return Self::default(),
        };
        if !path.exists() {
            return Self::default();
        }
        match fs::read_to_string(&path) {
            Ok(data) => match serde_json::from_str::<Preferences>(&data) {
                Ok(prefs) => prefs.migrate(),
                Err(_) => Self::default(),
            },
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) {
        let Some(path) = config_path() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(&path, serde_json::to_string_pretty(self).unwrap_or_default());
    }

    fn migrate(mut self) -> Self {
        if self.version < CONFIG_VERSION {
            self.version = CONFIG_VERSION;
        }
        self
    }

    pub fn apply_tab(&self) -> Tab {
        match self.tab.as_str() {
            "Cpu" => Tab::Cpu,
            "Memory" => Tab::Memory,
            "Energy" => Tab::Energy,
            "Disk" => Tab::Disk,
            "Network" => Tab::Network,
            "Movers" => Tab::Movers,
            _ => Tab::Cpu,
        }
    }

    pub fn apply_sort_key(&self) -> SortKey {
        match self.sort_key.as_str() {
            "Cpu" => SortKey::Cpu,
            "Memory" => SortKey::Memory,
            "Energy" => SortKey::Energy,
            "DiskRead" => SortKey::DiskRead,
            "DiskWrite" => SortKey::DiskWrite,
            "NetworkIn" => SortKey::NetworkIn,
            "NetworkOut" => SortKey::NetworkOut,
            "Trend" => SortKey::Trend,
            "Name" => SortKey::Name,
            "Pid" => SortKey::Pid,
            "User" => SortKey::User,
            "Runtime" => SortKey::Runtime,
            _ => SortKey::Cpu,
        }
    }

    pub fn from_app(app: &PreferencesSource) -> Self {
        Self {
            version: CONFIG_VERSION,
            tab: app.tab.title().to_string(),
            sort_key: app
                .sort_key
                .title()
                .replace("% CPU", "Cpu")
                .replace("Impact", "Energy")
                .trim()
                .to_string(),
            sort_desc: app.sort_desc,
            show_details: app.show_details,
            overview_visible: app.overview_visible,
            interval_ms: app.interval.as_millis() as u64,
            filter: app.filter.clone(),
            compact_mode: app.compact_mode,
        }
    }
}

pub struct PreferencesSource {
    pub tab: Tab,
    pub sort_key: SortKey,
    pub sort_desc: bool,
    pub show_details: bool,
    pub overview_visible: bool,
    pub interval: Duration,
    pub filter: String,
    pub compact_mode: bool,
}

fn config_path() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(
        PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("monitr")
            .join("config.json"),
    )
}
