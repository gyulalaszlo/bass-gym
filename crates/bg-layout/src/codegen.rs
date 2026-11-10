//! Generating typed Rust for layout files, from a build script:
//!
//! ```ignore
//! // build.rs
//! fn main() {
//!     bg_layout::codegen::build("src");
//! }
//! ```
//!
//! Every `<name>.layout.xml` under the directory becomes `$OUT_DIR/layouts/<name>.rs`.
//! A view includes it in a module of its own, next to its code:
//!
//! ```ignore
//! mod xml {
//!     include!(concat!(env!("OUT_DIR"), "/layouts/beatgrid.rs"));
//! }
//! ```
//!
//! which defines `<Name>Inputs`, `<Name>Slots` and `fn layout(rect, &inputs) -> slots`.

use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::{Path, PathBuf};

use super::{Kind, Layout, Node, Size, parse};

/// Generates the code for every layout file under `dir` (relative to the crate) into
/// `$OUT_DIR/layouts/`. Panics, failing the build, on a file that doesn't parse.
pub fn build(dir: impl AsRef<Path>) {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("run from build.rs"));
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("run from build.rs")).join("layouts");
    std::fs::create_dir_all(&out).expect("create OUT_DIR/layouts");
    let root = manifest.join(dir);
    println!("cargo:rerun-if-changed={}", root.display());
    let mut files = Vec::new();
    find(&root, &mut files);
    for path in files {
        println!("cargo:rerun-if-changed={}", path.display());
        let xml = std::fs::read_to_string(&path).expect("read layout file");
        let layout = parse(&xml).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let stem = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix(".layout.xml"))
            .expect("a .layout.xml name");
        let code = generate(&layout, &path);
        std::fs::write(out.join(format!("{stem}.rs")), code).expect("write generated layout");
    }
}

fn find(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            find(&p, out);
        } else if p.to_str().is_some_and(|s| s.ends_with(".layout.xml")) {
            out.push(p);
        }
    }
}

/// What a layout names: its sizes, flags and slots (with whether each can be hidden).
#[derive(Default)]
struct Names {
    sizes: BTreeMap<String, ()>,
    flags: BTreeMap<String, ()>,
    /// In tree order.
    slots: Vec<(String, bool)>,
}

fn names(root: &Node) -> Names {
    let mut n = Names::default();
    root.walk(&mut |node, hideable| {
        if let Some(Size::Input(s)) = &node.size {
            n.sizes.insert(s.clone(), ());
        }
        if let Kind::Stack {
            spacing: Size::Input(s),
            ..
        } = &node.kind
        {
            n.sizes.insert(s.clone(), ());
        }
        if let Some(c) = &node.visible {
            n.flags.insert(c.flag.clone(), ());
        }
        if let Some(name) = &node.name {
            n.slots.push((name.clone(), hideable));
        }
    });
    n
}

/// The generated code for one layout.
pub fn generate(layout: &Layout, path: &Path) -> String {
    let n = names(&layout.root);
    let name = &layout.name;
    let mut c = String::new();
    let path = path.display().to_string();
    let _ = writeln!(
        c,
        "// Generated from {path} by bg_layout::codegen. Don't edit: edit the XML.\n"
    );
    let _ = writeln!(
        c,
        "/// What the `{name}` layout reads.\n#[derive(Clone, Copy, Debug, Default, PartialEq)]\npub struct {name}Inputs {{"
    );
    for s in n.sizes.keys() {
        let _ = writeln!(c, "    pub {s}: f32,");
    }
    for f in n.flags.keys() {
        let _ = writeln!(c, "    pub {f}: bool,");
    }
    let _ = writeln!(c, "}}\n");
    let _ = writeln!(c, "impl ::bg_layout::Inputs for {name}Inputs {{");
    let _ = writeln!(
        c,
        "    fn size(&self, name: &str) -> Option<f32> {{\n        match name {{"
    );
    for s in n.sizes.keys() {
        let _ = writeln!(c, "            {s:?} => Some(self.{s}),");
    }
    let _ = writeln!(c, "            _ => None,\n        }}\n    }}");
    let _ = writeln!(
        c,
        "    fn flag(&self, name: &str) -> Option<bool> {{\n        match name {{"
    );
    for f in n.flags.keys() {
        let _ = writeln!(c, "            {f:?} => Some(self.{f}),");
    }
    let _ = writeln!(c, "            _ => None,\n        }}\n    }}\n}}\n");
    let _ = writeln!(
        c,
        "/// Where the `{name}` layout put its slots (`None`: hidden).\n#[derive(Clone, Copy, Debug, PartialEq)]\npub struct {name}Slots {{"
    );
    for (s, hideable) in &n.slots {
        let ty = if *hideable {
            "Option<::bg_layout::egui::Rect>"
        } else {
            "::bg_layout::egui::Rect"
        };
        let _ = writeln!(c, "    pub {s}: {ty},");
    }
    let _ = writeln!(c, "}}\n");
    // `Rect` has no `Default`: nothing placed yet is `Rect::NOTHING`, or `None`.
    let _ = writeln!(
        c,
        "impl Default for {name}Slots {{\n    fn default() -> Self {{\n        Self {{"
    );
    for (s, hideable) in &n.slots {
        let value = if *hideable {
            "None"
        } else {
            "::bg_layout::egui::Rect::NOTHING"
        };
        let _ = writeln!(c, "            {s}: {value},");
    }
    let _ = writeln!(c, "        }}\n    }}\n}}\n");
    let _ = writeln!(c, "impl ::bg_layout::Slots for {name}Slots {{");
    let _ = writeln!(
        c,
        "    fn set(&mut self, name: &str, rect: ::bg_layout::egui::Rect) -> bool {{\n        match name {{"
    );
    for (s, hideable) in &n.slots {
        let value = if *hideable { "Some(rect)" } else { "rect" };
        let _ = writeln!(c, "            {s:?} => self.{s} = {value},");
    }
    let _ = writeln!(
        c,
        "            _ => return false,\n        }}\n        true\n    }}\n}}\n"
    );
    let _ = writeln!(
        c,
        "static {upper}_LAYOUT: ::std::sync::OnceLock<::bg_layout::runtime::Live> = ::std::sync::OnceLock::new();\n",
        upper = name.to_uppercase()
    );
    let _ = writeln!(
        c,
        "/// Lays out `rect` as `{name}.layout.xml` says (reloaded when it changes, in debug\n/// builds).\npub fn layout(rect: ::bg_layout::egui::Rect, inputs: &{name}Inputs) -> {name}Slots {{\n    let live = {upper}_LAYOUT.get_or_init(|| {{\n        ::bg_layout::runtime::Live::new(include_str!({path:?}), {path:?})\n    }});\n    let mut slots = {name}Slots::default();\n    live.solve(rect, inputs, &mut slots);\n    slots\n}}",
        upper = name.to_uppercase()
    );
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_typed_inputs_and_slots() {
        let l = parse(
            r#"<layout name="Demo"><vstack spacing="gap">
                 <slot name="top" height="top_h" visible="show_top"/>
                 <slot name="rest" grow="1"/>
               </vstack></layout>"#,
        )
        .unwrap();
        let code = generate(&l, Path::new("/x/demo.layout.xml"));
        for want in [
            "pub struct DemoInputs",
            "pub gap: f32,",
            "pub top_h: f32,",
            "pub show_top: bool,",
            "pub top: Option<::bg_layout::egui::Rect>,",
            "pub rest: ::bg_layout::egui::Rect,",
            "pub fn layout(",
        ] {
            assert!(code.contains(want), "missing {want:?} in\n{code}");
        }
    }
}
