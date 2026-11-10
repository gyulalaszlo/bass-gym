//! The marker editor around a time, with the score above the waveform.
//!
//! **Bridge**: `MarkerEditor` isn't migrated yet and edits markers and its state in
//! place, so it works on copies, and any change is sent as [`Msg::Editor`].

use bg_core::score::KeySignature;
use egui::{Painter, Rect, RichText, Ui};

use super::Props;
use crate::icons;
use crate::kit::{Emit, edited};
use crate::theme;
use crate::views::review::msg::EditorChange;
use crate::views::review::theme::Theme;
use crate::views::review::{Msg, RecordingPeaks};
use crate::widgets::button::button;
use crate::widgets::marker_editor::{Header, Line, Marker, MarkerEditor};
use crate::widgets::score::{ScoreView, StaffLayout, paint_fixed_lead};
use crate::widgets::time_axis::TimeAxis;

pub fn editor(p: &Props, theme: &Theme, ui: &mut Ui, playhead: Option<f64>, out: &mut Emit<Msg>) {
    let r = p.review;
    ui.horizontal(|ui| {
        if ui
            .add(button(format!("{} Back to score", icons::ARROW_LEFT)).small())
            .clicked()
        {
            out.send(Msg::BackToScore);
        }
        ui.label(
            RichText::new(
                "Drag down to zoom in, up to zoom out, sideways to move. Double-click to add a marker.",
            )
            .small()
            .color(theme.hint),
        );
    });
    let Some(state) = &r.editor else {
        return;
    };
    ui.add_space(theme.metrics.editor_gap);
    let score = &r.score;
    let key = r.input.scale.map(|s| KeySignature::of(&s));
    // Bars are named by the editor's own markers.
    let layout = StaffLayout {
        labels: false,
        ..p.layout()
    };
    let paint_score = |painter: &Painter, rect: Rect, axis: TimeAxis| {
        let (from, to) = (axis.t(rect.left()), axis.t(rect.right()));
        // With the bar before, for notes tied into the view.
        let first = score.partition_point(|b| b.end < from).saturating_sub(1);
        let last = score.partition_point(|b| b.start <= to).max(first);
        ScoreView {
            bars: &score[first..last],
            axis,
            layout,
            lead: None,
        }
        .paint(painter, rect);
        paint_fixed_lead(painter, rect, layout, key);
    };
    let before = EditorChange {
        markers: r.markers.clone(),
        state: state.clone(),
        next_marker: r.next_marker,
    };
    let change = edited(&before, |c| {
        let next = &mut c.next_marker;
        let mut new_marker = |time| {
            *next += 1;
            Marker {
                id: *next,
                time,
                end: None,
                label: String::new(),
                // `MarkerEditor` still reads the global theme.
                color: theme::cyan(),
                line: Line::Solid,
            }
        };
        MarkerEditor {
            header: Some(Header {
                height: layout.height(),
                paint: &paint_score,
            }),
            source: &RecordingPeaks(&r.input.recording),
            overlays: &r.overlays,
            markers: &mut c.markers,
            state: &mut c.state,
            snap: &|t| t,
            add_remove: true,
            new_marker: &mut new_marker,
            playhead,
        }
        .show(ui, theme.metrics.editor_height);
    });
    if let Some(c) = change {
        out.send(Msg::Editor(Box::new(c)));
    }
}
