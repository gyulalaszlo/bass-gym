//! The scrolling exercise area: the live part (performance strip and beat grid), then
//! the groove panel. Two regions, so the timing lock the live part holds is released
//! before the panel.

use bg_ui::egui::Ui;
use bg_ui::kit::{self, Emit};
use bg_ui::views::beatgrid::{self, Beatgrid, LiveScore};
use bg_ui::widgets::performance::performance_strip;
use bg_ui::widgets::session_panel::session_panel;

use crate::basic_timing::msg::Msg;
use crate::basic_timing::props;
use crate::basic_timing::theme::Theme;
use crate::timing_session::PERFORMANCE_HEIGHT;

/// Height of the beat grid's note lane: 150 in all at the default text size.
const LANE_HEIGHT: f32 = 102.0;

pub fn live(p: &props::Main, theme: &Theme, ui: &mut Ui) {
    // Live performance, large, above the grid.
    performance_strip(ui, PERFORMANCE_HEIGHT, &[&p.meter]);
    ui.add_space(theme.meter_gap);

    let grid = beatgrid::Props {
        now: p.grid_now,
        bars: p.grid.bars(),
        matcher: p.grid.matcher(),
        steps_per_beat: p.session.groove.steps_per_beat,
        bars_behind: p.view.bars_behind,
        bars_ahead: p.view.bars_ahead,
        chords: &[],
        score: p.prefs.show_score.then_some(LiveScore {
            chords: &[],
            scale: Some(p.session.key),
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
    ui.add_space(theme.grid_gap);
}

pub fn settings(p: &props::Settings, ui: &mut Ui, out: &mut Emit<Msg>) {
    // `session_panel` edits in place (not migrated yet): edit a copy, send it if changed.
    let mut cfg = p.session.clone();
    if session_panel(ui, &mut cfg, p.editable) {
        out.send(Msg::Session(Box::new(cfg)));
    }
}
