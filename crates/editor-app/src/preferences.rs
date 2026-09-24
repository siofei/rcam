//! Local, bounded UI preferences. No manufacturing data enters this file.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

const MAX_BYTES: u64 = 64 * 1024;
const MAX_RECENT: usize = 12;

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct AppPreferences {
    pub recent_projects: Vec<PathBuf>,
    pub logging_level: rcam_diagnostics::Level,
    pub shortcut_overrides: std::collections::BTreeMap<String, String>,
    pub panel_width: Option<f32>,
    pub recent_colors: Vec<String>,
}

impl AppPreferences {
    pub fn path() -> Option<PathBuf> {
        Some(
            PathBuf::from(std::env::var_os("HOME")?)
                .join("Library/Application Support/RCam/preferences.json"),
        )
    }
    pub fn load(path: &Path) -> Self {
        let Ok(metadata) = fs::metadata(path) else {
            return Self::default();
        };
        if metadata.len() > MAX_BYTES {
            return Self::default();
        }
        let Ok(bytes) = fs::read(path) else {
            return Self::default();
        };
        let Ok(mut prefs) = serde_json::from_slice::<Self>(&bytes) else {
            return Self::default();
        };
        prefs.recent_projects.retain(|path| {
            path.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("rcam"))
                && path.as_os_str().len() <= 4096
        });
        prefs.recent_projects.truncate(MAX_RECENT);
        prefs.recent_colors.truncate(8);
        prefs.panel_width = prefs
            .panel_width
            .filter(|width| width.is_finite() && (240.0..=480.0).contains(width));
        if prefs.logging_level == rcam_diagnostics::Level::Trace {
            prefs.logging_level = rcam_diagnostics::Level::Info;
        }
        prefs
    }
    pub fn remember(&mut self, path: PathBuf) {
        self.recent_projects.retain(|old| old != &path);
        self.recent_projects.insert(0, path);
        self.recent_projects.truncate(MAX_RECENT);
    }
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let Some(parent) = path.parent() else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "no preferences directory",
            ));
        };
        fs::create_dir_all(parent)?;
        let bytes = serde_json::to_vec(self)?;
        if bytes.len() > MAX_BYTES as usize {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "preferences too large",
            ));
        }
        let temp = path.with_extension("json.tmp");
        fs::write(&temp, bytes)?;
        fs::rename(temp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recent_is_bounded_and_deduplicated() {
        let mut p = AppPreferences::default();
        for n in 0..20 {
            p.remember(PathBuf::from(format!("{n}.rcam")));
        }
        assert_eq!(p.recent_projects.len(), 12);
        p.remember(PathBuf::from("8.rcam"));
        assert_eq!(p.recent_projects[0], PathBuf::from("8.rcam"));
        assert_eq!(
            p.recent_projects
                .iter()
                .filter(|path| *path == &PathBuf::from("8.rcam"))
                .count(),
            1
        );
    }

    #[test]
    fn corrupt_and_oversized_preferences_fall_back_to_defaults() {
        let dir = std::env::temp_dir().join(format!(
            "rcam-preferences-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("preferences.json");
        fs::write(&path, b"{broken").unwrap();
        assert!(AppPreferences::load(&path).recent_projects.is_empty());
        fs::write(&path, vec![b'x'; MAX_BYTES as usize + 1]).unwrap();
        assert!(AppPreferences::load(&path).recent_projects.is_empty());
        fs::remove_dir_all(dir).unwrap();
    }
}
