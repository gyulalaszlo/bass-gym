//! Turning a layout tree into rects.

use egui::{Rect, pos2};

use super::{Axis, Kind, Node, Size};

/// What a layout reads: its sizes and flags, by name. Generated for each layout file.
pub trait Inputs {
    fn size(&self, name: &str) -> Option<f32>;
    fn flag(&self, name: &str) -> Option<bool>;
}

/// Where a layout puts its slots, by name. Generated for each layout file.
pub trait Slots {
    /// Places a slot; returns false if there's no slot of that name.
    fn set(&mut self, name: &str, rect: Rect) -> bool;
}

/// What went wrong while solving: names the inputs or slots don't have. Only possible
/// with a layout reloaded at runtime (generated code checks names when it's built).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Unknown(pub Vec<String>);

fn resolve(size: &Size, inputs: &dyn Inputs, unknown: &mut Unknown) -> f32 {
    match size {
        Size::Points(p) => *p,
        Size::Input(name) => inputs.size(name).unwrap_or_else(|| {
            unknown.0.push(format!("size {name:?}"));
            0.0
        }),
    }
}

fn visible(n: &Node, inputs: &dyn Inputs, unknown: &mut Unknown) -> bool {
    n.visible.as_ref().is_none_or(|c| {
        let on = inputs.flag(&c.flag).unwrap_or_else(|| {
            unknown.0.push(format!("flag {:?}", c.flag));
            true
        });
        on != c.negated
    })
}

/// Lays out `root` in `rect`, placing every visible named node in `slots`. Returns the
/// names the inputs and slots didn't know.
pub fn solve(root: &Node, rect: Rect, inputs: &dyn Inputs, slots: &mut dyn Slots) -> Unknown {
    let mut unknown = Unknown::default();
    place(root, rect, inputs, slots, &mut unknown);
    unknown
}

/// The visible children of a stack, with groups' children in their place.
fn flatten<'a>(
    children: &'a [Node],
    inputs: &dyn Inputs,
    unknown: &mut Unknown,
    out: &mut Vec<&'a Node>,
) {
    for c in children {
        if !visible(c, inputs, unknown) {
            continue;
        }
        match &c.kind {
            Kind::Group { children } => flatten(children, inputs, unknown, out),
            _ => out.push(c),
        }
    }
}

fn place(n: &Node, rect: Rect, inputs: &dyn Inputs, slots: &mut dyn Slots, unknown: &mut Unknown) {
    if let Some(name) = &n.name
        && !slots.set(name, rect)
    {
        unknown.0.push(format!("slot {name:?}"));
    }
    let Kind::Stack {
        axis,
        spacing,
        children,
    } = &n.kind
    else {
        return;
    };
    let mut shown = Vec::new();
    flatten(children, inputs, unknown, &mut shown);
    let spacing = resolve(spacing, inputs, unknown);
    let (start, length) = match axis {
        Axis::Vertical => (rect.top(), rect.height()),
        Axis::Horizontal => (rect.left(), rect.width()),
    };
    let fixed: Vec<f32> = shown
        .iter()
        .map(|c| c.size.as_ref().map_or(0.0, |s| resolve(s, inputs, unknown)))
        .collect();
    let grow: Vec<f32> = shown
        .iter()
        .map(|c| resolve(&c.grow, inputs, unknown))
        .collect();
    let weights: f32 = grow.iter().sum();
    let gaps = spacing * shown.len().saturating_sub(1) as f32;
    let free = (length - fixed.iter().sum::<f32>() - gaps).max(0.0);
    let mut at = start;
    for ((c, fixed), grow) in shown.iter().zip(fixed).zip(grow) {
        let share = if weights > 0.0 {
            free * grow / weights
        } else {
            0.0
        };
        let size = fixed + share;
        let r = match axis {
            Axis::Vertical => {
                Rect::from_min_max(pos2(rect.left(), at), pos2(rect.right(), at + size))
            }
            Axis::Horizontal => {
                Rect::from_min_max(pos2(at, rect.top()), pos2(at + size, rect.bottom()))
            }
        };
        place(c, r, inputs, slots, unknown);
        at += size + spacing;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use std::collections::HashMap;

    struct Map(HashMap<&'static str, f32>, HashMap<&'static str, bool>);

    impl Inputs for Map {
        fn size(&self, name: &str) -> Option<f32> {
            self.0.get(name).copied()
        }
        fn flag(&self, name: &str) -> Option<bool> {
            self.1.get(name).copied()
        }
    }

    #[derive(Default)]
    struct Placed(HashMap<String, Rect>);

    impl Slots for Placed {
        fn set(&mut self, name: &str, rect: Rect) -> bool {
            self.0.insert(name.to_owned(), rect);
            true
        }
    }

    fn rect() -> Rect {
        Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 100.0))
    }

    #[test]
    fn fixed_sizes_first_then_growth_and_hidden_children_take_no_room() {
        let l = parse(
            r#"<layout name="T"><vstack spacing="5">
                 <slot name="a" height="top"/>
                 <slot name="hidden" height="50" visible="off"/>
                 <slot name="b" grow="1"/>
                 <hstack name="row" height="10"><slot name="l" width="30"/><slot name="r" grow="1"/></hstack>
               </vstack></layout>"#,
        )
        .unwrap();
        let inputs = Map([("top", 20.0)].into(), [("off", false)].into());
        let mut placed = Placed::default();
        assert_eq!(
            solve(&l.root, rect(), &inputs, &mut placed),
            Unknown::default()
        );
        let p = &placed.0;
        assert_eq!((p["a"].top(), p["a"].bottom()), (0.0, 20.0));
        assert!(!p.contains_key("hidden"));
        // 100 - 20 - 10 - 2 × 5 = 60 for b.
        assert_eq!((p["b"].top(), p["b"].bottom()), (25.0, 85.0));
        assert_eq!((p["row"].top(), p["row"].bottom()), (90.0, 100.0));
        assert_eq!((p["l"].left(), p["l"].right()), (0.0, 30.0));
        assert_eq!((p["r"].left(), p["r"].right()), (30.0, 100.0));
    }

    #[test]
    fn a_group_shows_and_hides_its_children_together() {
        let l = parse(
            r#"<layout name="T"><vstack>
                 <group visible="band"><gap height="10"/><slot name="b" height="20"/></group>
                 <slot name="rest" grow="1"/>
               </vstack></layout>"#,
        )
        .unwrap();
        let mut placed = Placed::default();
        solve(
            &l.root,
            rect(),
            &Map(HashMap::new(), [("band", true)].into()),
            &mut placed,
        );
        assert_eq!((placed.0["b"].top(), placed.0["rest"].top()), (10.0, 30.0));
        let mut placed = Placed::default();
        solve(
            &l.root,
            rect(),
            &Map(HashMap::new(), [("band", false)].into()),
            &mut placed,
        );
        assert!(!placed.0.contains_key("b"));
        assert_eq!(placed.0["rest"].top(), 0.0);
    }

    #[test]
    fn unknown_names_are_reported_not_fatal() {
        let l =
            parse(r#"<layout name="T"><vstack><slot name="a" height="nope"/></vstack></layout>"#)
                .unwrap();
        let inputs = Map(HashMap::new(), HashMap::new());
        let unknown = solve(&l.root, rect(), &inputs, &mut Placed::default());
        assert_eq!(unknown.0, vec!["size \"nope\"".to_owned()]);
    }
}
