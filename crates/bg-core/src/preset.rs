//! Versioned exercise presets.
//!
//! A preset file is an envelope around opaque exercise data. Exercise types own their
//! data schema and a version number; when a preset is loaded its data is migrated
//! step-wise to the current version. Presets that cannot be migrated (e.g. written by a
//! newer app) are kept untouched and exposed as incompatible instead of being overwritten.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub const ENVELOPE_FORMAT: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PresetEnvelope {
    pub format: u32,
    pub id: Uuid,
    pub name: String,
    pub exercise: String,
    pub version: u32,
    pub created: DateTime<Utc>,
    pub modified: DateTime<Utc>,
    pub data: Value,
}

impl PresetEnvelope {
    pub fn new(exercise: &str, version: u32, name: &str, data: Value) -> Self {
        let now = Utc::now();
        Self {
            format: ENVELOPE_FORMAT,
            id: Uuid::new_v4(),
            name: name.to_owned(),
            exercise: exercise.to_owned(),
            version,
            created: now,
            modified: now,
            data,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PresetError {
    #[error(
        "preset was written by a newer version (schema v{found}, this app supports v{supported})"
    )]
    TooNew { found: u32, supported: u32 },
    #[error("no migration from schema v{0}")]
    NoMigration(u32),
    #[error("invalid preset data: {0}")]
    Invalid(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// Schema description of an exercise type's preset data.
pub trait PresetSchema {
    fn current_version(&self) -> u32;
    /// Migrates `data` from `from` to `from + 1`.
    fn migrate_step(&self, from: u32, data: Value) -> Result<Value, PresetError> {
        let _ = data;
        Err(PresetError::NoMigration(from))
    }
}

/// Brings `data` from `version` up to the schema's current version.
pub fn migrate(
    schema: &dyn PresetSchema,
    version: u32,
    mut data: Value,
) -> Result<Value, PresetError> {
    let current = schema.current_version();
    if version > current {
        return Err(PresetError::TooNew {
            found: version,
            supported: current,
        });
    }
    for v in version..current {
        data = schema.migrate_step(v, data)?;
    }
    Ok(data)
}

#[derive(Clone, Debug)]
pub struct StoredPreset {
    pub path: PathBuf,
    pub envelope: PresetEnvelope,
    /// Set when the preset can't be used by this version of the app.
    pub incompatible: Option<String>,
}

/// File-system preset store: `<root>/<exercise id>/<preset uuid>.json`.
#[derive(Clone, Debug)]
pub struct PresetStore {
    root: PathBuf,
}

impl PresetStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn dir(&self, exercise: &str) -> PathBuf {
        self.root.join(exercise)
    }

    /// Lists the presets of one exercise, sorted by name. Unreadable files are reported as
    /// incompatible entries (when their envelope parses) or skipped.
    pub fn list(&self, exercise: &str, schema: &dyn PresetSchema) -> Vec<StoredPreset> {
        let Ok(entries) = std::fs::read_dir(self.dir(exercise)) else {
            return Vec::new();
        };
        let mut out: Vec<StoredPreset> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
            .filter_map(|path| {
                let text = std::fs::read_to_string(&path).ok()?;
                let envelope: PresetEnvelope = match serde_json::from_str(&text) {
                    Ok(e) => e,
                    Err(e) => {
                        log::warn!("skipping unreadable preset {}: {e}", path.display());
                        return None;
                    }
                };
                let incompatible = migrate(schema, envelope.version, envelope.data.clone())
                    .err()
                    .map(|e| e.to_string());
                Some(StoredPreset {
                    path,
                    envelope,
                    incompatible,
                })
            })
            .collect();
        out.sort_by_key(|p| p.envelope.name.to_lowercase());
        out
    }

    pub fn path_for(&self, envelope: &PresetEnvelope) -> PathBuf {
        self.dir(&envelope.exercise)
            .join(format!("{}.json", envelope.id))
    }

    /// Writes atomically (temp file + rename).
    pub fn save(&self, envelope: &PresetEnvelope) -> Result<PathBuf, PresetError> {
        let path = self.path_for(envelope);
        std::fs::create_dir_all(path.parent().unwrap())?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(envelope)?)?;
        std::fs::rename(&tmp, &path)?;
        Ok(path)
    }

    pub fn delete(&self, envelope: &PresetEnvelope) -> Result<(), PresetError> {
        std::fs::remove_file(self.path_for(envelope))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct V3;
    impl PresetSchema for V3 {
        fn current_version(&self) -> u32 {
            3
        }
        fn migrate_step(&self, from: u32, mut data: Value) -> Result<Value, PresetError> {
            match from {
                1 => {
                    // v2 renamed `bpm` to `tempo`
                    let bpm = data["bpm"].take();
                    data.as_object_mut().unwrap().remove("bpm");
                    data["tempo"] = bpm;
                    Ok(data)
                }
                2 => {
                    data["swing"] = json!(0.0);
                    Ok(data)
                }
                _ => Err(PresetError::NoMigration(from)),
            }
        }
    }

    #[test]
    fn migrates_stepwise() {
        let d = migrate(&V3, 1, json!({"bpm": 100})).unwrap();
        assert_eq!(d, json!({"tempo": 100, "swing": 0.0}));
        assert!(matches!(
            migrate(&V3, 4, json!({})),
            Err(PresetError::TooNew { .. })
        ));
    }

    #[test]
    fn store_roundtrip_and_incompatible() {
        let dir = std::env::temp_dir().join(format!("bg-preset-test-{}", Uuid::new_v4()));
        let store = PresetStore::new(&dir);
        let a = PresetEnvelope::new("ex", 3, "Bravo", json!({"tempo": 1}));
        let b = PresetEnvelope::new("ex", 9, "alpha", json!({}));
        store.save(&a).unwrap();
        store.save(&b).unwrap();
        std::fs::write(dir.join("ex/garbage.json"), "{nope").unwrap();
        let list = store.list("ex", &V3);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].envelope.name, "alpha");
        assert!(list[0].incompatible.is_some());
        assert_eq!(list[1].envelope, a);
        assert!(list[1].incompatible.is_none());
        store.delete(&a).unwrap();
        assert_eq!(store.list("ex", &V3).len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
