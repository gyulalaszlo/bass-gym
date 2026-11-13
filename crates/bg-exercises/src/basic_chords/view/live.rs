//! The live part of the exercise area: the keyboard, the progression strip, the
//! performance strip and the beat grid.

use bg_ui::egui::Ui;
use bg_ui::kit;
use bg_ui::views::beatgrid::{self, Beatgrid, LiveScore};
use bg_ui::views::keyboard::{self, Keyboard};
use bg_ui::views::performance::{self, PerformanceStrip};

use super::strip::{self, Strip};
use crate::basic_chords::props;
use crate::basic_chords::theme::Theme;
use crate::timing_session::PERFORMANCE_HEIGHT;

/// Height of the beat grid's note lane: 180 in all at the default text size, chord band
/// included.
const LANE_HEIGHT: f32 = 108.0;

pub fn live(p: &props::Live, theme: &Theme, ui: &mut Ui) {
    let m = &theme.metrics;
    ui.add_space(m.top_gap);
    let k = &p.keyboard;
    let bass = k.bass.map(|(midi, grade)| {
        let color = grade.map_or(theme.bass_unjudged, |g| theme.bass.get(g));
        (midi, color)
    });
    let props = keyboard::Props {
        keys: k.keys,
        active: &k.active,
        ghost: &k.ghost,
        bass,
        labels: k.labels,
    };
    if k.visible {
        kit::show::<Keyboard>(ui, "keyboard", &props, &theme.keyboard);
        ui.add_space(m.keyboard_gap);
    }

    let strip = strip::Props {
        chords: &p.strip,
        current: p.strip_current,
    };
    kit::show::<Strip>(ui, "strip", &strip, &theme.strip);
    ui.add_space(m.strip_gap);

    let indicators: [&dyn performance::Indicator; 1] = [&p.meter];
    let strip = performance::Props {
        height: PERFORMANCE_HEIGHT,
        indicators: &indicators,
    };
    kit::show::<PerformanceStrip>(ui, "performance", &strip, &theme.performance);
    ui.add_space(m.meter_gap);

    let grid = beatgrid::Props {
        now: p.grid_now,
        bars: p.grid.bars(),
        matcher: p.grid.matcher(),
        steps_per_beat: p.steps_per_beat,
        bars_behind: 1.0,
        bars_ahead: 2.0,
        chords: &p.grid_chords,
        score: p.prefs.show_score.then_some(LiveScore {
            chords: &p.score_chords,
            scale: Some(p.key),
            a4: p.a4,
            size: p.prefs.score_size,
            targets: &[],
        }),
        lane_height: LANE_HEIGHT,
        zoom: p.prefs.zoom_factor(),
    };
    kit::show::<Beatgrid>(ui, "grid", &grid, &theme.grid);
    if p.running {
        ui.ctx().request_repaint();
    }
    ui.add_space(m.grid_gap);
}
