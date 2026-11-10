//! The results dialog: statistics, and the Score + Audio review.

use std::path::Path;

use bg_ui::egui::Context;
use bg_ui::kit::{self, Emit};
use bg_ui::views::results::{self, ResultsTab, Stats, results_dialog};
use bg_ui::views::review::{self, ReviewView};

use crate::basic_timing::msg::Msg;
use crate::basic_timing::props;
use crate::basic_timing::theme::Theme;

pub fn results_region(
    p: &props::Results,
    theme: &Theme,
    data_dir: Option<&Path>,
    ctx: &Context,
    out: &mut Emit<Msg>,
) {
    let mut open = p.open;
    let mut tab = p.tab.unwrap_or_default();
    let tabs = p.tab.is_some().then_some(&mut tab);
    results_dialog(
        ctx,
        &theme.results.dialog,
        &mut open,
        "Results",
        tabs,
        |ui, tab| match (tab, p.review, &p.stats) {
            (ResultsTab::Review, Some(r), _) => {
                let props = review::Props {
                    review: r,
                    data_dir,
                    staff_spacing: p.staff_spacing,
                };
                let msgs = kit::show::<ReviewView>(ui, "review", &props, &theme.review);
                out.extend_map(msgs, Msg::Review);
            }
            (_, _, Some(stats)) => {
                let props = results::Props {
                    stats,
                    thresholds: p.scoring,
                    tables: None,
                };
                kit::show::<Stats>(ui, "stats", &props, &theme.results);
            }
            _ => {}
        },
    );
    if p.review.is_some_and(|r| r.is_playing()) {
        // Ends the replay when the dialog closed or playback finished.
        out.send(Msg::Review(review::Msg::Tick { dialog_open: open }));
    }
    if open != p.open {
        out.send(Msg::ResultsOpen(open));
    }
    if p.tab.is_some_and(|t| t != tab) {
        out.send(Msg::ResultsTab(tab));
    }
}
