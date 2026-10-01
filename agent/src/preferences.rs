use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::RwLock;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct DesktopPreferences {
    #[serde(rename = "rs-engine")]
    pub engine: String,
    #[serde(rename = "rsWorkMode")]
    pub work_mode: String,
    #[serde(rename = "rsPermMode")]
    pub perm_mode: String,
    #[serde(rename = "rsSounds")]
    pub sounds: bool,
    #[serde(rename = "rsExtraThinking")]
    pub extra_thinking: bool,
    #[serde(rename = "rsPlanMode")]
    pub plan_mode: bool,
    #[serde(rename = "rsThinkingLevel")]
    pub thinking_level: String,
    #[serde(rename = "rsForgeMode")]
    pub forge_mode: bool,
    #[serde(rename = "rsAutoFix")]
    pub auto_fix: bool,
    #[serde(rename = "rsBgMode")]
    pub bg_mode: bool,
}

impl Default for DesktopPreferences {
    fn default() -> Self {
        Self {
            engine: "roblox".to_string(),
            work_mode: "balanced".to_string(),
            perm_mode: "sandbox".to_string(),
            sounds: true,
            extra_thinking: false,
            plan_mode: false,
            thinking_level: "default".to_string(),
            forge_mode: true,
            auto_fix: true,
            bg_mode: true,
        }
    }
}

impl DesktopPreferences {
    pub fn normalize(&mut self) {
        if !matches!(self.engine.as_str(), "roblox" | "local") {
            self.engine = "roblox".to_string();
        }
        if !matches!(self.work_mode.as_str(), "fast" | "balanced" | "thorough") {
            self.work_mode = "balanced".to_string();
        }
        if !matches!(self.perm_mode.as_str(), "sandbox" | "ask" | "full") {
            self.perm_mode = "sandbox".to_string();
        }
        if !matches!(self.thinking_level.as_str(), "default" | "low" | "mid" | "high" | "max") {
            self.thinking_level = "default".to_string();
        }
    }
}

pub struct PreferencesStore {
    path: PathBuf,
    inner: RwLock<DesktopPreferences>,
}

impl PreferencesStore {
    pub fn load() -> Self {
        Self::load_from(settings_path())
    }

    fn load_from(path: PathBuf) -> Self {
        let mut prefs = std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str::<DesktopPreferences>(&raw).ok())
            .unwrap_or_default();
        prefs.normalize();
        Self { path, inner: RwLock::new(prefs) }
    }

    pub fn persisted(&self) -> bool {
        self.path.exists()
    }

    pub fn snapshot(&self) -> DesktopPreferences {
        self.inner.read().map(|p| p.clone()).unwrap_or_default()
    }

    pub fn replace(&self, mut prefs: DesktopPreferences) -> anyhow::Result<DesktopPreferences> {
        prefs.normalize();
        {
            let mut guard = self.inner.write().map_err(|_| anyhow::anyhow!("preferences lock poisoned"))?;
            *guard = prefs.clone();
        }
        self.save(&prefs)?;
        Ok(prefs)
    }

    pub fn patch(&self, patch: serde_json::Value) -> anyhow::Result<DesktopPreferences> {
        let current = self.snapshot();
        let mut value = serde_json::to_value(current)?;
        let dst = value.as_object_mut().ok_or_else(|| anyhow::anyhow!("preferences are not an object"))?;
        let src = patch.as_object().ok_or_else(|| anyhow::anyhow!("preferences patch must be an object"))?;
        for (key, val) in src {
            if dst.contains_key(key) {
                dst.insert(key.clone(), val.clone());
            }
        }
        let next = serde_json::from_value::<DesktopPreferences>(value)?;
        self.replace(next)
    }

    pub fn update<F>(&self, mut edit: F) -> anyhow::Result<DesktopPreferences>
    where
        F: FnMut(&mut DesktopPreferences),
    {
        let mut next = self.snapshot();
        edit(&mut next);
        self.replace(next)
    }

    fn save(&self, prefs: &DesktopPreferences) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(prefs)?)?;
        if self.path.exists() {
            std::fs::remove_file(&self.path)?;
        }
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

fn settings_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("plazcode-settings.json")))
        .unwrap_or_else(|| PathBuf::from("plazcode-settings.json"))
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_settings_path() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("plazcode-settings-{}-{nonce}.json", std::process::id()))
    }

    #[test]
    fn preferences_persist_across_reopen() {
        let path = temp_settings_path();
        let store = PreferencesStore::load_from(path.clone());
        assert!(!store.persisted());

        let mut prefs = store.snapshot();
        prefs.engine = "local".to_string();
        prefs.work_mode = "thorough".to_string();
        prefs.perm_mode = "ask".to_string();
        prefs.sounds = false;
        prefs.extra_thinking = true;
        prefs.plan_mode = true;
        prefs.thinking_level = "max".to_string();
        prefs.forge_mode = false;
        prefs.auto_fix = false;
        prefs.bg_mode = false;
        store.replace(prefs).expect("save preferences");
        assert!(store.persisted());

        let reopened = PreferencesStore::load_from(path.clone()).snapshot();
        assert_eq!(reopened.engine, "local");
        assert_eq!(reopened.work_mode, "thorough");
        assert_eq!(reopened.perm_mode, "ask");
        assert!(!reopened.sounds);
        assert!(reopened.extra_thinking);
        assert!(reopened.plan_mode);
        assert_eq!(reopened.thinking_level, "max");
        assert!(!reopened.forge_mode);
        assert!(!reopened.auto_fix);
        assert!(!reopened.bg_mode);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn invalid_saved_choices_are_normalized() {
        let path = temp_settings_path();
        std::fs::write(&path, r#"{
            "rs-engine":"bad",
            "rsWorkMode":"bad",
            "rsPermMode":"bad",
            "rsThinkingLevel":"bad"
        }"#).expect("write invalid preferences");

        let reopened = PreferencesStore::load_from(path.clone()).snapshot();
        assert_eq!(reopened.engine, "roblox");
        assert_eq!(reopened.work_mode, "balanced");
        assert_eq!(reopened.perm_mode, "sandbox");
        assert_eq!(reopened.thinking_level, "default");

        let _ = std::fs::remove_file(path);
    }
}
