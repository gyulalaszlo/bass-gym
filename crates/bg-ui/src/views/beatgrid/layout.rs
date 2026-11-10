//! Where everything in the beat grid goes. Pure: no painting and no input.

use egui::{Rect, Vec2, vec2};

use super::Props;
use crate::kit::{Axis, Part, split};
use crate::widgets::time_axis::TimeAxis;

/// Height of the chord band, when there are chords.
const CHORD_BAND: f32 = 24.0;
/// Room for the bar header above the note lane.
const HEADER: f32 = 30.0;
/// Room for the "click off" band below the note lane.
const FOOTER: f32 = 18.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    /// The live score above the grid, if shown.
    pub staff: Option<Rect>,
    pub grid: Rect,
    /// Time to x, shared by the grid and the score.
    pub axis: TimeAxis,
    /// The visible time span.
    pub t0: f64,
    pub t1: f64,
    /// The chord band, if there are chords.
    pub chord_band: Option<Rect>,
    /// The note lane: top, bottom and middle.
    pub lane_top: f32,
    pub lane_bottom: f32,
    pub mid: f32,
}

/// The size the grid wants at `width`.
pub fn size(props: &Props, width: f32) -> Vec2 {
    vec2(width, props.height + score_height(props))
}

fn score_height(props: &Props) -> f32 {
    props.score.map_or(0.0, |s| s.layout().height())
}

/// The length of the bar playing at `props.now` (or the first bar), in seconds.
fn bar_len(props: &Props) -> f64 {
    props
        .bars
        .iter()
        .rev()
        .find(|b| b.start <= props.now)
        .or(props.bars.first())
        .map_or(2.0, |b| b.duration)
}

pub fn layout(props: &Props, rect: Rect) -> Layout {
    let [staff, grid] = split(
        rect,
        Axis::Vertical,
        0.0,
        [Part::fixed(score_height(props)), Part::free(1.0)],
    );
    let bar_len = bar_len(props);
    let behind = props.bars_behind * bar_len / props.zoom;
    let ahead = props.bars_ahead * bar_len / props.zoom;
    let (t0, t1) = (props.now - behind, props.now + ahead);
    let band = if props.chords.is_empty() {
        0.0
    } else {
        CHORD_BAND
    };
    let lane_top = grid.top() + HEADER + band;
    let lane_bottom = grid.bottom() - FOOTER;
    Layout {
        staff: props.score.is_some().then_some(staff),
        grid,
        axis: TimeAxis::new(grid.x_range(), t0, t1),
        t0,
        t1,
        chord_band: (band > 0.0).then(|| {
            Rect::from_x_y_ranges(
                grid.x_range(),
                (grid.top() + 26.0)..=(grid.top() + 26.0 + band - 4.0),
            )
        }),
        lane_top,
        lane_bottom,
        mid: (lane_top + lane_bottom) / 2.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::beatgrid::LiveScore;
    use bg_core::{BarPlan, Matcher};
    use egui::pos2;

    fn props<'a>(bars: &'a [BarPlan], matcher: &'a Matcher) -> Props<'a> {
        Props {
            now: 10.0,
            bars,
            matcher,
            steps_per_beat: 2,
            bars_behind: 1.0,
            bars_ahead: 2.0,
            chords: &[],
            score: None,
            height: 150.0,
            zoom: 1.0,
        }
    }

    fn rect() -> Rect {
        Rect::from_min_max(pos2(0.0, 0.0), pos2(300.0, 150.0))
    }

    #[test]
    fn now_line_sits_at_the_share_behind() {
        let matcher = Matcher::default();
        let p = props(&[], &matcher);
        let l = layout(&p, rect());
        // No bars: two seconds a bar, one behind and two ahead.
        assert_eq!((l.t0, l.t1), (8.0, 14.0));
        assert_eq!(l.axis.x(p.now), 100.0);
        assert_eq!(l.chord_band, None);
        assert_eq!(l.staff, None);
    }

    #[test]
    fn zoom_narrows_the_span_around_now() {
        let matcher = Matcher::default();
        let p = Props {
            zoom: 2.0,
            ..props(&[], &matcher)
        };
        let l = layout(&p, rect());
        assert_eq!((l.t0, l.t1), (9.0, 12.0));
        assert_eq!(l.axis.x(p.now), 100.0);
    }

    #[test]
    fn chords_push_the_lane_down() {
        let matcher = Matcher::default();
        let chords = vec![(0.0, 4.0, "C".to_owned())];
        let p = Props {
            chords: &chords,
            ..props(&[], &matcher)
        };
        let l = layout(&p, rect());
        assert_eq!(l.lane_top, HEADER + CHORD_BAND);
        assert!(l.chord_band.is_some());
    }

    #[test]
    fn the_score_goes_above_the_grid() {
        let matcher = Matcher::default();
        let live = LiveScore {
            chords: &[],
            scale: None,
            a4: 440.0,
            size: 8.0,
        };
        let p = Props {
            score: Some(live),
            ..props(&[], &matcher)
        };
        let h = live.layout().height();
        let full = Rect::from_min_size(pos2(0.0, 0.0), size(&p, 300.0));
        let l = layout(&p, full);
        assert_eq!(l.staff.map(|s| s.height()), Some(h));
        assert_eq!((l.grid.top(), l.grid.height()), (h, 150.0));
    }
}
