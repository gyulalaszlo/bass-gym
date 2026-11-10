//! The results dialog: statistics with the per-chord table, and the Score + Audio
//! review.
//!
//! **Bridge** until the review is migrated (phase 4): `SessionReview` owns its state and
//! drives the engine itself, so this region gets it mutably along with the engine.
//! Everything else here only reads props and sends messages.

use std::path::Path;

use bg_audio::AudioEngine;
use bg_ui::egui::Context;
use bg_ui::kit::{self, Emit};
use bg_ui::widgets::results::{ResultsTab, block, results, results_dialog};
use bg_ui::widgets::session_review::SessionReview;

use super::per_chord::PerChordTable;
use crate::basic_chords::msg::Msg;
use crate::basic_chords::props;
use crate::basic_chords::theme::Theme;

#[allow(clippy::too_many_arguments)]
pub fn results_region(
    p: &props::Results,
    theme: &Theme,
    review: &mut Option<SessionReview>,
    engine: &mut AudioEngine,
    data_dir: Option<&Path>,
    ctx: &Context,
    out: &mut Emit<Msg>,
) {
    let mut open = p.open;
    let mut tab = p.tab.unwrap_or_default();
    let tabs = p.tab.is_some().then_some(&mut tab);
    results_dialog(ctx, &mut open, "Results", tabs, |ui, tab| {
        match (tab, review.as_mut(), &p.stats) {
            (ResultsTab::Review, Some(r), _) => r.ui(ui, engine, data_dir, p.staff_spacing),
            (_, _, Some(stats)) => results(ui, stats, &p.scoring, |ui| {
                block(ui, "Per chord", |ui| {
                    if let Some(pc) = &p.per_chord {
                        kit::show::<PerChordTable>(ui, "per_chord", pc, &theme.per_chord);
                    }
                })
            }),
            _ => {}
        }
    });
    if let Some(r) = review.as_mut() {
        r.tick(engine, open);
    }
    if open != p.open {
        out.send(Msg::ResultsOpen(open));
    }
    if p.tab.is_some_and(|t| t != tab) {
        out.send(Msg::ResultsTab(tab));
    }
}
