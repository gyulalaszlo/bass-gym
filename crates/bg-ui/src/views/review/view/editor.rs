//! The marker editor around a time, with the score above the waveform. The editor's
//! messages go through the review (`Msg::Editor`), which keeps its state and markers.

use bg_core::score::KeySignature;
use egui::{Painter, Rect, RichText, Ui};

use super::Props;
use crate::icons;
use crate::kit::Emit;
use crate::views::marker_editor::{self, Header, Line, Marker};
use crate::views::review::theme::Theme;
use crate::views::review::{Msg, RecordingPeaks};
use crate::widgets::button::button;
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
        .paint(painter, rect, &theme.score);
        paint_fixed_lead(painter, rect, layout, key, &theme.score);
    };
    let id = r.next_marker + 1;
    let new_marker = |time| Marker {
        id,
        time,
        end: None,
        label: String::new(),
        color: theme.new_marker,
        line: Line::Solid,
    };
    let props = marker_editor::Props {
        header: Some(Header {
            height: layout.height(),
            paint: &paint_score,
        }),
        source: &RecordingPeaks(&r.input.recording),
        overlays: &r.overlays,
        markers: &r.markers,
        snap: &|t| t,
        add_remove: true,
        new_marker: &new_marker,
        playhead,
        height: theme.metrics.editor_height,
    };
    let mut msgs = Emit::new();
    marker_editor::view(state, &props, &theme.editor, ui, &mut msgs);
    out.extend_map(msgs, Msg::Editor);
}
