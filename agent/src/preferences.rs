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
        let path = settings_path();
        let mut prefs = std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str::<DesktopPreferences>(&raw).ok())
            .unwrap_or_default();
        prefs.normalize();
        Self { path, inner: RwLock::new(prefs) }
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
