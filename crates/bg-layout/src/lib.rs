//! The layout DSL: a component's layout as an XML tree of stacks, gaps and named slots,
//! solved into rects at runtime.
//!
//! ```xml
//! <layout name="Beatgrid">
//!   <vstack>
//!     <slot name="staff" height="score_height" visible="show_score"/>
//!     <vstack name="grid" grow="1">
//!       <gap height="header"/>
//!       <slot name="lane" grow="1"/>
//!     </vstack>
//!   </vstack>
//! </layout>
//! ```
//!
//! - `<vstack>` / `<hstack>` lay out their children top to bottom / left to right,
//!   `spacing` apart. A named stack is a slot too.
//! - `<slot name>` is a rect the component fills. `<gap>` is empty space.
//! - `<group>` is an invisible wrapper: its children belong to the stack around it, and
//!   its `visible` applies to all of them, so related items show and hide together.
//! - Along the stack, a child is `height` / `width` points plus a `grow` share of what's
//!   left (both a number or an input's name). Across it, it fills the stack.
//! - `visible="flag"` (or `"!flag"`) hides a child: it takes no room and its slot is
//!   `None`.
//!
//! A build script runs [`codegen::build`] over a crate's `*.layout.xml` files. Each gets a
//! typed inputs struct (every size and flag it names) and a typed slots struct, with a
//! `layout(rect, &inputs) -> slots` function. Debug builds reload the file when it
//! changes, so a layout can be edited while the app runs.

pub mod codegen;
mod parse;
pub mod runtime;
mod solve;

pub use egui;
pub use parse::{ParseError, parse};
pub use solve::{Inputs, Slots, solve};

/// A stack's direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    /// Top to bottom.
    Vertical,
    /// Left to right.
    Horizontal,
}

/// A size along a stack: a number, or the name of an input.
#[derive(Clone, Debug, PartialEq)]
pub enum Size {
    Points(f32),
    Input(String),
}

/// A `visible` condition: an input flag, possibly negated.
#[derive(Clone, Debug, PartialEq)]
pub struct Condition {
    pub flag: String,
    pub negated: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Stack {
        axis: Axis,
        spacing: Size,
        children: Vec<Node>,
    },
    Slot,
    Gap,
    /// Children spliced into the enclosing stack.
    Group {
        children: Vec<Node>,
    },
}

/// One element of the layout tree.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub kind: Kind,
    /// Slots and named stacks.
    pub name: Option<String>,
    /// Fixed size along the parent stack.
    pub size: Option<Size>,
    /// Share of the parent stack's free space.
    pub grow: Size,
    pub visible: Option<Condition>,
}

/// A parsed layout file: its name and its root.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub name: String,
    pub root: Node,
}

impl Node {
    /// Every node in the tree, depth first.
    pub fn walk(&self, f: &mut impl FnMut(&Node, bool)) {
        self.walk_inner(false, f);
    }

    /// `hideable`: this node or an ancestor has a `visible` condition.
    fn walk_inner(&self, hideable: bool, f: &mut impl FnMut(&Node, bool)) {
        let hideable = hideable || self.visible.is_some();
        f(self, hideable);
        if let Kind::Stack { children, .. } | Kind::Group { children } = &self.kind {
            for c in children {
                c.walk_inner(hideable, f);
            }
        }
    }
}
