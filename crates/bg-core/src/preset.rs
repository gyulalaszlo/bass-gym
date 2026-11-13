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
    /// Longer notes on the exercise; free text.
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
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
            description: String::new(),
            tags: Vec::new(),
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
    /// Index of the preset dir it was found in, and its folder relative to that dir.
    pub root: usize,
    pub folder: PathBuf,
    pub envelope: PresetEnvelope,
    /// Set when the preset can't be used by this version of the app.
    pub incompatible: Option<String>,
}

/// What a scan of the preset dirs found for one exercise.
#[derive(Clone, Debug, Default)]
pub struct Scan {
    /// Sorted by name.
    pub presets: Vec<StoredPreset>,
    /// Every subfolder of every dir (empty ones too), as `(root, relative path)`, sorted.
    pub folders: Vec<(usize, PathBuf)>,
}

/// File-system preset store: any number of preset dirs, each a free tree of folders
/// holding `<uuid>.json` files. A file belongs to the exercise named in its envelope.
#[derive(Clone, Debug)]
pub struct PresetStore {
    roots: Vec<PathBuf>,
}

impl PresetStore {
    /// `roots[0]` is the user's own dir, where the app saves by default.
    pub fn new(roots: Vec<PathBuf>) -> Self {
        assert!(!roots.is_empty(), "a preset store needs at least one dir");
        Self { roots }
    }

    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    /// Lists the presets of one exercise and all folders. Unreadable files are reported as
    /// incompatible entries (when their envelope parses) or skipped. When two files share
    /// an id, the first found wins.
    pub fn scan(&self, exercise: &str, schema: &dyn PresetSchema) -> Scan {
        let mut scan = Scan::default();
        for (root, dir) in self.roots.iter().enumerate() {
            walk(dir, dir, root, exercise, schema, &mut scan);
        }
        let mut seen = std::collections::HashSet::new();
        scan.presets.retain(|p| {
            let fresh = seen.insert(p.envelope.id);
            if !fresh {
                log::warn!("skipping duplicate preset {}", p.path.display());
            }
            fresh
        });
        scan.presets.sort_by_key(|p| p.envelope.name.to_lowercase());
        scan.folders.sort();
        scan
    }

    /// Where a new preset goes: `<root>/<folder>/<uuid>.json`.
    pub fn new_path(&self, root: usize, folder: &Path, envelope: &PresetEnvelope) -> PathBuf {
        let root = self.roots.get(root).unwrap_or(&self.roots[0]);
        root.join(folder).join(format!("{}.json", envelope.id))
    }
}

fn walk(
    root_dir: &Path,
    dir: &Path,
    root: usize,
    exercise: &str,
    schema: &dyn PresetSchema,
    scan: &mut Scan,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for path in entries.flatten().map(|e| e.path()) {
        if path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with('.'))
        {
            continue;
        }
        let folder = dir
            .strip_prefix(root_dir)
            .unwrap_or(Path::new(""))
            .to_path_buf();
        if path.is_dir() {
            if let Ok(rel) = path.strip_prefix(root_dir) {
                scan.folders.push((root, rel.to_path_buf()));
            }
            walk(root_dir, &path, root, exercise, schema, scan);
            continue;
        }
        if !path.extension().is_some_and(|e| e == "json") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let envelope: PresetEnvelope = match serde_json::from_str(&text) {
            Ok(e) => e,
            Err(e) => {
                log::warn!("skipping unreadable preset {}: {e}", path.display());
                continue;
            }
        };
        if envelope.exercise != exercise {
            continue;
        }
        let incompatible = migrate(schema, envelope.version, envelope.data.clone())
            .err()
            .map(|e| e.to_string());
        scan.presets.push(StoredPreset {
            path,
            root,
            folder,
            envelope,
            incompatible,
        });
    }
}

/// Writes atomically (temp file + rename), creating the folder.
pub fn save(path: &Path, envelope: &PresetEnvelope) -> Result<(), PresetError> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(envelope)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub fn delete(path: &Path) -> Result<(), PresetError> {
    std::fs::remove_file(path)?;
    Ok(())
}

/// What differs between two preset data trees, one line per changed leaf:
/// `groove.tempo: 90 → 100`.
pub fn diff(old: &Value, new: &Value) -> Vec<String> {
    let mut out = Vec::new();
    diff_into("", old, new, &mut out);
    out
}

fn diff_into(at: &str, old: &Value, new: &Value, out: &mut Vec<String>) {
    let key = |k: &str| {
        if at.is_empty() {
            k.to_owned()
        } else {
            format!("{at}.{k}")
        }
    };
    match (old, new) {
        (Value::Object(a), Value::Object(b)) => {
            let mut keys: Vec<&String> = a.keys().chain(b.keys()).collect();
            keys.sort();
            keys.dedup();
            for k in keys {
                let (x, y) = (
                    a.get(k).unwrap_or(&Value::Null),
                    b.get(k).unwrap_or(&Value::Null),
                );
                diff_into(&key(k), x, y, out);
            }
        }
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
            for (i, (x, y)) in a.iter().zip(b).enumerate() {
                diff_into(&key(&i.to_string()), x, y, out);
            }
        }
        _ if old != new => {
            let name = if at.is_empty() { "(all)" } else { at };
            out.push(format!("{name}: {} → {}", short(old), short(new)));
        }
        _ => {}
    }
}

/// A value, cut short so a whole changed list stays on one line.
fn short(v: &Value) -> String {
    let s = v.to_string();
    if s.chars().count() > 40 {
        format!("{}…", s.chars().take(40).collect::<String>())
    } else {
        s
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
    fn store_scans_nested_folders_and_roots() {
        let dir = std::env::temp_dir().join(format!("bg-preset-test-{}", Uuid::new_v4()));
        let (user, pack) = (dir.join("user"), dir.join("pack"));
        let store = PresetStore::new(vec![user.clone(), pack.clone()]);
        let a = PresetEnvelope::new("ex", 3, "Bravo", json!({"tempo": 1}));
        let b = PresetEnvelope::new("ex", 9, "alpha", json!({}));
        let other = PresetEnvelope::new("other", 3, "Other", json!({}));
        let pa = store.new_path(0, Path::new("slow/swing"), &a);
        save(&pa, &a).unwrap();
        save(&store.new_path(1, Path::new(""), &b), &b).unwrap();
        save(&store.new_path(1, Path::new(""), &other), &other).unwrap();
        // The same preset copied into the pack: the user dir's copy wins.
        save(&store.new_path(1, Path::new("dup"), &a), &a).unwrap();
        std::fs::create_dir_all(user.join("empty")).unwrap();
        std::fs::write(user.join("garbage.json"), "{nope").unwrap();

        let scan = store.scan("ex", &V3);
        assert_eq!(scan.presets.len(), 2);
        assert_eq!(scan.presets[0].envelope.name, "alpha");
        assert!(scan.presets[0].incompatible.is_some());
        assert_eq!(
            (scan.presets[0].root, scan.presets[0].folder.as_path()),
            (1, Path::new(""))
        );
        assert_eq!(scan.presets[1].envelope, a);
        assert_eq!(scan.presets[1].path, pa);
        assert_eq!(scan.presets[1].folder, Path::new("slow/swing"));
        assert!(scan.presets[1].incompatible.is_none());
        let folders: Vec<_> = scan
            .folders
            .iter()
            .map(|(r, p)| (*r, p.to_str().unwrap()))
            .collect();
        assert_eq!(
            folders,
            [(0, "empty"), (0, "slow"), (0, "slow/swing"), (1, "dup")]
        );

        delete(&pa).unwrap();
        assert_eq!(
            store.scan("ex", &V3).presets.len(),
            2,
            "the pack's copy shows again"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn diff_lists_changed_leaves() {
        let old = json!({"groove": {"tempo": 90, "swing": 0.0}, "bars": [1, 2], "gone": true});
        let new = json!({"groove": {"tempo": 100, "swing": 0.0}, "bars": [1, 3], "added": "x"});
        assert_eq!(
            diff(&old, &new),
            [
                "added: null → \"x\"",
                "bars.1: 2 → 3",
                "gone: true → null",
                "groove.tempo: 90 → 100",
            ]
        );
        assert!(diff(&old, &old).is_empty());
    }
}
