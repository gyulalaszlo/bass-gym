//! The scrolling exercise area: the live part (performance strip and beat grid), then
//! the groove panel. Two regions, so the timing lock the live part holds is released
//! before the panel.

use bg_ui::egui::Ui;
use bg_ui::kit::{self, Emit};
use bg_ui::theme::caption;
use bg_ui::views::beatgrid::{self, Beatgrid, LiveScore};
use bg_ui::views::neck::{self, Neck, Tuning};
use bg_ui::views::performance::{self, PerformanceStrip};
use bg_ui::widgets::button::selectable;
use bg_ui::widgets::session_panel::{self, SessionPanel};

use crate::basic_timing::msg::Msg;
use crate::basic_timing::props;
use crate::basic_timing::theme::Theme;
use crate::timing_session::PERFORMANCE_HEIGHT;

/// Height of the beat grid's note lane: 150 in all at the default text size, or 180 with
/// Basic scales' note band (as Basic chords' chord band).
const LANE_HEIGHT: f32 = 102.0;
const LANE_HEIGHT_NOTES: f32 = 108.0;
/// Frets shown: every scale's octave fits below the 12th.
const FRETS: u8 = 12;

pub fn live(p: &props::Main, theme: &Theme, ui: &mut Ui) {
    if let Some(n) = &p.neck {
        ui.add_space(theme.neck_gap);
        let props = neck::Props {
            tuning: n.tuning,
            frets: FRETS,
            scale: n.scale,
            target: n.target,
            next: n.next,
            names: &n.names,
        };
        kit::show::<Neck>(ui, "neck", &props, &theme.neck);
        ui.add_space(theme.neck_gap);
    }

    // Live performance, large, above the grid.
    let indicators: [&dyn performance::Indicator; 1] = [&p.meter];
    let strip = performance::Props {
        height: PERFORMANCE_HEIGHT,
        indicators: &indicators,
    };
    kit::show::<PerformanceStrip>(ui, "performance", &strip, &theme.performance);
    ui.add_space(theme.meter_gap);

    let grid = beatgrid::Props {
        now: p.grid_now,
        bars: p.grid.bars(),
        matcher: p.grid.matcher(),
        steps_per_beat: p.session.groove.steps_per_beat,
        bars_behind: p.view.bars_behind,
        bars_ahead: p.view.bars_ahead,
        chords: &p.grid_notes,
        score: p.prefs.show_score.then_some(LiveScore {
            chords: &[],
            scale: Some(p.session.key),
            a4: p.a4,
            size: p.prefs.score_size,
        }),
        lane_height: if p.grid_notes.is_empty() {
            LANE_HEIGHT
        } else {
            LANE_HEIGHT_NOTES
        },
        zoom: p.prefs.zoom_factor(),
    };
    kit::show::<Beatgrid>(ui, "grid", &grid, &theme.grid);
    if p.running {
        ui.ctx().request_repaint();
    }
    ui.add_space(theme.grid_gap);
}

pub fn settings(p: &props::Settings, theme: &Theme, ui: &mut Ui, out: &mut Emit<Msg>) {
    let props = session_panel::Props {
        cfg: p.session,
        enabled: p.editable,
    };
    let edits = kit::show::<SessionPanel>(ui, "session", &props, &theme.session);
    out.extend_map(edits, |cfg| Msg::Session(Box::new(cfg)));
    if let Some(current) = p.tuning {
        ui.horizontal(|ui| {
            caption(ui, "Neck");
            for t in Tuning::ALL {
                let resp = ui.add_enabled(p.editable, selectable(t == current, t.label()));
                if resp.clicked() && t != current {
                    out.send(Msg::Tuning(t));
                }
            }
        });
    }
}
