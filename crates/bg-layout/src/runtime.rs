//! What generated code calls: a layout's tree, parsed once, and in debug builds parsed
//! again whenever its file changes.

use std::path::PathBuf;
use std::sync::Mutex;
#[cfg(debug_assertions)]
use std::time::{Duration, Instant, SystemTime};

use egui::Rect;

use super::{Inputs, Node, Slots, parse, solve};

/// How often a debug build looks at the layout file.
#[cfg(debug_assertions)]
const CHECK_EVERY: Duration = Duration::from_millis(250);

struct State {
    root: Node,
    #[cfg(debug_assertions)]
    modified: Option<SystemTime>,
    #[cfg(debug_assertions)]
    checked: Instant,
}

/// One layout file's tree.
pub struct Live {
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    path: PathBuf,
    state: Mutex<State>,
}

impl Live {
    /// `source` is the file as it was built; `path` is where to look for changes.
    pub fn new(source: &str, path: &str) -> Self {
        let root = parse(source)
            .unwrap_or_else(|e| panic!("{path}: {e} (checked at build time)"))
            .root;
        let path = PathBuf::from(path);
        Self {
            state: Mutex::new(State {
                root,
                #[cfg(debug_assertions)]
                modified: std::fs::metadata(&path).and_then(|m| m.modified()).ok(),
                #[cfg(debug_assertions)]
                checked: Instant::now(),
            }),
            path,
        }
    }

    /// Lays out `rect`. A reloaded tree that doesn't parse, or names something the
    /// inputs or slots don't have, is logged and the last good one is kept.
    pub fn solve(&self, rect: Rect, inputs: &dyn Inputs, slots: &mut dyn Slots) {
        #[allow(unused_mut)]
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        #[cfg(debug_assertions)]
        self.reload(&mut state, rect, inputs);
        solve(&state.root, rect, inputs, slots);
    }

    #[cfg(debug_assertions)]
    fn reload(&self, state: &mut State, rect: Rect, inputs: &dyn Inputs) {
        if state.checked.elapsed() < CHECK_EVERY {
            return;
        }
        state.checked = Instant::now();
        let modified = std::fs::metadata(&self.path)
            .and_then(|m| m.modified())
            .ok();
        if modified.is_none() || modified == state.modified {
            return;
        }
        state.modified = modified;
        let Ok(xml) = std::fs::read_to_string(&self.path) else {
            return;
        };
        match parse(&xml) {
            Err(e) => log::error!("{}: {e}; keeping the previous layout", self.path.display()),
            Ok(layout) => {
                // Try it on a throwaway set of slots, to catch unknown names.
                struct Probe;
                impl Slots for Probe {
                    fn set(&mut self, _: &str, _: Rect) -> bool {
                        true
                    }
                }
                let unknown = solve(&layout.root, rect, inputs, &mut Probe);
                if unknown.0.is_empty() {
                    log::info!("{}: reloaded", self.path.display());
                    state.root = layout.root;
                } else {
                    log::error!(
                        "{}: unknown {}; rebuild to add them. Keeping the previous layout.",
                        self.path.display(),
                        unknown.0.join(", ")
                    );
                }
            }
        }
    }
}

#[cfg(all(test, debug_assertions))]
mod tests {
    use super::*;
    use egui::pos2;

    struct Height(f32);

    impl Inputs for Height {
        fn size(&self, name: &str) -> Option<f32> {
            (name == "h").then_some(self.0)
        }
        fn flag(&self, _: &str) -> Option<bool> {
            None
        }
    }

    #[derive(Default)]
    struct Top(Option<Rect>);

    impl Slots for Top {
        fn set(&mut self, name: &str, rect: Rect) -> bool {
            if name == "top" {
                self.0 = Some(rect);
            }
            true
        }
    }

    fn top_height(live: &Live) -> f32 {
        let mut top = Top::default();
        let rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(10.0, 100.0));
        live.solve(rect, &Height(30.0), &mut top);
        top.0.map_or(-1.0, |r| r.height())
    }

    #[test]
    fn a_changed_file_is_picked_up_and_a_broken_one_ignored() {
        let dir = std::env::temp_dir().join(format!("bg-layout-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.layout.xml");
        let first = r#"<layout name="T"><vstack><slot name="top" height="10"/></vstack></layout>"#;
        std::fs::write(&path, first).unwrap();
        let live = Live::new(first, path.to_str().unwrap());
        assert_eq!(top_height(&live), 10.0);

        let wait = || std::thread::sleep(CHECK_EVERY + Duration::from_millis(50));
        // Mtimes can be coarse: make sure the next write looks newer.
        std::thread::sleep(Duration::from_millis(1100));
        std::fs::write(
            &path,
            r#"<layout name="T"><vstack><slot name="top" height="h"/></vstack></layout>"#,
        )
        .unwrap();
        wait();
        assert_eq!(top_height(&live), 30.0);

        std::thread::sleep(Duration::from_millis(1100));
        std::fs::write(&path, "<layout name=\"T\"><vstack>").unwrap();
        wait();
        assert_eq!(
            top_height(&live),
            30.0,
            "a broken file keeps the last good layout"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
