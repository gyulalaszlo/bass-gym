//! The live part of the exercise area: the keyboard, the progression strip, the
//! performance strip and the beat grid.

use bg_ui::egui::Ui;
use bg_ui::kit;
use bg_ui::views::beatgrid::{self, Beatgrid, LiveScore};
use bg_ui::widgets::keyboard::Keyboard;
use bg_ui::widgets::performance::performance_strip;

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
    // `Keyboard` isn't migrated yet: it reads the global theme for everything else.
    let bass = k.bass.map(|(midi, grade)| {
        let color = grade.map_or(theme.bass_unjudged, |g| theme.bass.get(g));
        (midi, color)
    });
    Keyboard {
        keys: k.keys,
        active: &k.active,
        ghost: &k.ghost,
        bass,
        labels: k.labels,
    }
    .show(ui);
    ui.add_space(m.keyboard_gap);

    let strip = strip::Props {
        chords: &p.strip,
        current: p.strip_current,
    };
    kit::show::<Strip>(ui, "strip", &strip, &theme.strip);
    ui.add_space(m.strip_gap);

    performance_strip(ui, PERFORMANCE_HEIGHT, &[&p.meter]);
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
