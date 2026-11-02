//! Modular settings: one TOML file, one table per preference page / module id. Each
//! module (de)serializes its own typed struct; unknown or invalid entries fall back to
//! defaults, so settings survive version changes.

use std::path::PathBuf;

use serde::Serialize;
use serde::de::DeserializeOwned;

pub struct SettingsStore {
    path: Option<PathBuf>,
    table: toml::Table,
    revision: u64,
    dirty: bool,
}

impl SettingsStore {
    pub fn load(path: Option<PathBuf>) -> Self {
        let table = path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| match s.parse::<toml::Table>() {
                Ok(t) => Some(t),
                Err(e) => {
                    log::warn!("ignoring unreadable settings: {e}");
                    None
                }
            })
            .unwrap_or_default();
        Self {
            path,
            table,
            revision: 0,
            dirty: false,
        }
    }

    pub fn in_memory() -> Self {
        Self::load(None)
    }

    /// Increases on every change; use it to refresh cached typed copies.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn get<T: DeserializeOwned + Default>(&self, id: &str) -> T {
        self.table
            .get(id)
            .and_then(|v| match v.clone().try_into() {
                Ok(t) => Some(t),
                Err(e) => {
                    log::warn!("settings [{id}] invalid, using defaults: {e}");
                    None
                }
            })
            .unwrap_or_default()
    }

    pub fn set<T: Serialize>(&mut self, id: &str, value: &T) {
        match toml::Value::try_from(value) {
            Ok(v) => {
                if self.table.get(id) != Some(&v) {
                    self.table.insert(id.to_owned(), v);
                    self.revision += 1;
                    self.dirty = true;
                }
            }
            Err(e) => log::error!("can't store settings [{id}]: {e}"),
        }
    }

    /// Writes the file if anything changed.
    pub fn flush(&mut self) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        let Some(path) = &self.path else { return };
        let result = (|| -> anyhow::Result<()> {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let tmp = path.with_extension("toml.tmp");
            std::fs::write(&tmp, toml::to_string_pretty(&self.table)?)?;
            std::fs::rename(&tmp, path)?;
            Ok(())
        })();
        if let Err(e) = result {
            log::error!("saving settings failed: {e:#}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
    #[serde(default)]
    struct A {
        x: f32,
        name: Option<String>,
    }

    #[test]
    fn typed_roundtrip_and_defaults() {
        let mut s = SettingsStore::in_memory();
        assert_eq!(s.get::<A>("a"), A::default());
        s.set("a", &A { x: 2.5, name: None });
        assert_eq!(s.revision(), 1);
        s.set("a", &A { x: 2.5, name: None });
        assert_eq!(s.revision(), 1);
        assert_eq!(s.get::<A>("a").x, 2.5);
        s.table
            .insert("b".into(), toml::Value::String("garbage".into()));
        assert_eq!(s.get::<A>("b"), A::default());
    }
}
