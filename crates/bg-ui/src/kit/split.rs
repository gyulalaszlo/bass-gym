//! [`split`]: divides a rect into named parts along one axis, for pure layout functions.

use egui::{Rect, pos2};

/// One part of a [`split`]: a fixed size in points plus a share of the space left over.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Part {
    pub fixed: f32,
    pub weight: f32,
}

impl Part {
    /// Exactly `size` points.
    pub const fn fixed(size: f32) -> Self {
        Self {
            fixed: size,
            weight: 0.0,
        }
    }

    /// A share of the free space, in proportion to `weight`.
    pub const fn free(weight: f32) -> Self {
        Self { fixed: 0.0, weight }
    }

    /// Nothing: for a part that's hidden.
    pub const NONE: Self = Self::fixed(0.0);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    /// Left to right.
    Horizontal,
    /// Top to bottom.
    Vertical,
}

/// Splits `rect` along `axis` into `N` parts with `gap` points between them. Fixed sizes
/// come first, and the remaining space is shared by weight. Parts may overflow `rect`
/// when the fixed sizes don't fit; a painter clips.
pub fn split<const N: usize>(rect: Rect, axis: Axis, gap: f32, parts: [Part; N]) -> [Rect; N] {
    let (start, length) = match axis {
        Axis::Horizontal => (rect.left(), rect.width()),
        Axis::Vertical => (rect.top(), rect.height()),
    };
    let fixed: f32 = parts.iter().map(|p| p.fixed).sum();
    let weights: f32 = parts.iter().map(|p| p.weight).sum();
    let gaps = gap * N.saturating_sub(1) as f32;
    let free = (length - fixed - gaps).max(0.0);
    let mut at = start;
    parts.map(|p| {
        let share = if weights > 0.0 {
            free * p.weight / weights
        } else {
            0.0
        };
        let size = p.fixed + share;
        let r = match axis {
            Axis::Horizontal => {
                Rect::from_min_max(pos2(at, rect.top()), pos2(at + size, rect.bottom()))
            }
            Axis::Vertical => {
                Rect::from_min_max(pos2(rect.left(), at), pos2(rect.right(), at + size))
            }
        };
        at += size + gap;
        r
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_parts_first_then_free_space_by_weight() {
        let r = Rect::from_min_max(pos2(0.0, 0.0), pos2(110.0, 10.0));
        let [a, b, c] = split(
            r,
            Axis::Horizontal,
            5.0,
            [Part::fixed(20.0), Part::free(1.0), Part::free(3.0)],
        );
        assert_eq!((a.left(), a.right()), (0.0, 20.0));
        // 110 - 20 fixed - 2 × 5 gap = 80 free, shared 1:3.
        assert_eq!((b.left(), b.right()), (25.0, 45.0));
        assert_eq!((c.left(), c.right()), (50.0, 110.0));
        assert_eq!(c.height(), 10.0);
    }

    #[test]
    fn vertical_with_a_hidden_part() {
        let r = Rect::from_min_max(pos2(0.0, 0.0), pos2(10.0, 100.0));
        let [top, rest] = split(r, Axis::Vertical, 0.0, [Part::NONE, Part::free(1.0)]);
        assert_eq!(top.height(), 0.0);
        assert_eq!((rest.top(), rest.bottom()), (0.0, 100.0));
    }
}
