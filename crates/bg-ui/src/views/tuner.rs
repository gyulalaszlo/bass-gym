//! Always-visible tuner strip. A tier-1 view.

use std::convert::Infallible;
use std::time::Duration;

use bg_audio::{InputMeter, TunerState};
use bg_core::music::{freq_to_midi, note_name, note_octave};
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, vec2};
use serde::{Deserialize, Serialize};

use crate::kit::{Emit, View};
use crate::theme::Tokens;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TunerPrefs {
    pub a4: f32,
    /// Deviation shown as in tune (green), in cents.
    pub in_tune_cents: f32,
    /// Deviation up to which the display is amber rather than red.
    pub close_cents: f32,
}

impl Default for TunerPrefs {
    fn default() -> Self {
        Self {
            a4: 440.0,
            in_tune_cents: 3.0,
            close_cents: 12.0,
        }
    }
}

impl TunerPrefs {
    pub const ID: &'static str = "tuner";
}

const STALE: Duration = Duration::from_millis(400);

pub struct Tuner;

pub struct Props<'a> {
    pub state: &'a TunerState,
    pub meter: &'a InputMeter,
    pub prefs: &'a TunerPrefs,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub background: Color32,
    /// In tune, close, and off; and without a note.
    pub in_tune: Color32,
    pub close: Color32,
    pub off: Color32,
    pub idle: Color32,
    pub note_font: FontId,
    pub octave_font: FontId,
    pub no_note_font: FontId,
    pub cents_font: FontId,
    pub hz_font: FontId,
    pub hz: Color32,
    pub unit_font: FontId,
    pub unit: Color32,
    pub no_signal_font: FontId,
    pub scale_font: FontId,
    pub scale_label: Color32,
    /// The scale's line and major ticks, and its minor ticks.
    pub line: Color32,
    pub minor: Color32,
    /// The input meter's frame, and its level when clipping.
    pub meter_frame: Stroke,
    pub clipping: Color32,
    pub target: Color32,
    pub zone_alpha: f32,
    pub arrow_font: FontId,
    /// Sizes below are at a text scale of 1 and grow with the text.
    pub scale: f32,
    pub height: f32,
}

impl From<&Tokens> for Theme {
    fn from(t: &Tokens) -> Self {
        Self {
            background: t.bg(),
            in_tune: t.green(),
            close: t.amber(),
            off: t.red(),
            idle: t.dim(),
            note_font: t.mono(50.0),
            octave_font: t.mono(20.0),
            no_note_font: t.mono(44.0),
            cents_font: t.mono(26.0),
            hz_font: t.mono(18.0),
            hz: t.green(),
            unit_font: t.mono(13.0),
            unit: t.cyan(),
            no_signal_font: t.mono(15.0),
            scale_font: t.mono(12.0),
            scale_label: t.text(),
            line: t.line(),
            minor: t.dim(),
            meter_frame: Stroke::new(1.0, t.line()),
            clipping: t.red(),
            target: t.magenta(),
            zone_alpha: 0.6,
            arrow_font: t.prop(22.0),
            scale: t.text_scale,
            height: 100.0,
        }
    }
}

impl Theme {
    /// The colour of a pitch `cents` off.
    pub fn tune_color(&self, prefs: &TunerPrefs, cents: f32) -> Color32 {
        let c = cents.abs();
        if c <= prefs.in_tune_cents {
            self.in_tune
        } else if c <= prefs.close_cents {
            self.close
        } else {
            self.off
        }
    }
}

impl View for Tuner {
    type Props<'a> = Props<'a>;
    type Theme = Theme;
    type Emission = Infallible;

    fn view(p: &Props, theme: &Theme, ui: &mut Ui, _out: &mut Emit<Infallible>) {
        tuner(ui, theme, p.state, p.meter, p.prefs);
    }
}

fn tuner(ui: &mut Ui, theme: &Theme, state: &TunerState, meter: &InputMeter, prefs: &TunerPrefs) {
    let k = theme.scale;
    let height = theme.height * k;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 0.0, theme.background);

    let fresh = state.updated.is_some_and(|t| t.elapsed() < STALE);
    let reading = state.reading.filter(|_| fresh);
    let id = ui.id().with("tuner_needle");
    let (note, cents, freq) = match reading {
        Some(r) => {
            let (midi, cents) = freq_to_midi(r.freq, prefs.a4);
            (Some(midi), cents, r.freq)
        }
        None => (None, 0.0, 0.0),
    };
    let shown = ui.ctx().animate_value_with_time(id, cents, 0.06);
    let color = if note.is_some() {
        theme.tune_color(prefs, cents)
    } else {
        theme.idle
    };

    // Note name.
    let name_w = 130.0 * k;
    let name_rect = Rect::from_min_size(
        rect.min + vec2(0.0, 10.0 * k),
        vec2(name_w, height - 10.0 * k),
    );
    match note {
        Some(m) => {
            let c = name_rect.center() - vec2(10.0 * k, 0.0);
            p.text(
                c,
                Align2::CENTER_CENTER,
                note_name(m),
                theme.note_font.clone(),
                color,
            );
            p.text(
                c + vec2(32.0 * k, 16.0 * k),
                Align2::LEFT_CENTER,
                note_octave(m).to_string(),
                theme.octave_font.clone(),
                color,
            );
        }
        None => {
            p.text(
                name_rect.center(),
                Align2::CENTER_CENTER,
                "--",
                theme.no_note_font.clone(),
                theme.idle,
            );
        }
    }

    // Readouts and the input meter on the right: boxed value + cyan unit.
    let meter_w = 10.0;
    let right_w = 150.0 * k;
    let right = Rect::from_min_max(
        Pos2::new(rect.right() - right_w - meter_w - 18.0, rect.top()),
        Pos2::new(rect.right() - meter_w - 18.0, rect.bottom()),
    );
    if note.is_some() {
        let value = p.text(
            right.center() + vec2(-8.0 * k, -14.0 * k),
            Align2::RIGHT_CENTER,
            format!("{:+.1}", cents),
            theme.cents_font.clone(),
            color,
        );
        p.text(
            value.right_center() + vec2(12.0, 0.0),
            Align2::LEFT_CENTER,
            "CENT",
            theme.unit_font.clone(),
            theme.unit,
        );
        let hz = p.text(
            right.center() + vec2(-8.0 * k, 22.0 * k),
            Align2::RIGHT_CENTER,
            format!("{freq:.2}"),
            theme.hz_font.clone(),
            theme.hz,
        );
        p.text(
            hz.right_center() + vec2(12.0, 0.0),
            Align2::LEFT_CENTER,
            "HZ",
            theme.unit_font.clone(),
            theme.unit,
        );
    } else {
        p.text(
            right.center(),
            Align2::CENTER_CENTER,
            "NO SIGNAL",
            theme.no_signal_font.clone(),
            theme.idle,
        );
    }
    let m_rect = Rect::from_min_max(
        Pos2::new(rect.right() - meter_w - 10.0, rect.top() + 10.0),
        Pos2::new(rect.right() - 10.0, rect.bottom() - 10.0),
    );
    p.rect_stroke(m_rect, 0.0, theme.meter_frame, StrokeKind::Inside);
    let level = ((meter.peak_db + 60.0) / 60.0).clamp(0.0, 1.0);
    let m_color = if meter.peak_db > -3.0 {
        theme.clipping
    } else if meter.note_active {
        theme.in_tune
    } else {
        theme.idle
    };
    p.rect_filled(
        Rect::from_min_max(
            Pos2::new(
                m_rect.left() + 2.0,
                m_rect.bottom() - m_rect.height() * level,
            ),
            m_rect.max - vec2(2.0, 0.0),
        ),
        0.0,
        m_color,
    );

    // Cents scale.
    let scale = Rect::from_min_max(
        Pos2::new(name_rect.right() + 24.0, rect.top() + 20.0 * k),
        Pos2::new(right.left() - 24.0, rect.bottom() - 12.0 * k),
    );
    let x_of = |c: f32| scale.center().x + c.clamp(-50.0, 50.0) / 50.0 * scale.width() / 2.0;
    let mid_y = scale.center().y - 4.0 * k;
    let tick = 10.0 * k;
    p.hline(scale.x_range(), mid_y, Stroke::new(1.5, theme.line));
    // Label every 10 cents, or every 20 / 50 when the scale is too narrow for that.
    let label_w = 34.0 * k;
    let label_every = [10, 20, 50]
        .into_iter()
        .find(|&n| scale.width() / 100.0 * n as f32 >= label_w)
        .unwrap_or(50);
    for c in (-50..=50).step_by(5) {
        let major = c % 10 == 0;
        let h = if c == 0 {
            tick * 1.6
        } else if major {
            tick
        } else {
            tick * 0.45
        };
        p.vline(
            x_of(c as f32),
            (mid_y - h)..=(mid_y + h),
            Stroke::new(
                if major { 1.5 } else { 1.0 },
                if major { theme.line } else { theme.minor },
            ),
        );
        if c % label_every == 0 && c != 0 {
            p.text(
                Pos2::new(x_of(c as f32), scale.bottom()),
                Align2::CENTER_BOTTOM,
                format!("{c:+}"),
                theme.scale_font.clone(),
                theme.scale_label,
            );
        }
    }
    // In-tune zone and target marker.
    let zone = Rect::from_x_y_ranges(
        x_of(-prefs.in_tune_cents)..=x_of(prefs.in_tune_cents),
        (mid_y - tick * 1.8)..=(mid_y + tick * 1.8),
    );
    p.rect_stroke(
        zone,
        0.0,
        Stroke::new(1.0, theme.in_tune.gamma_multiply(theme.zone_alpha)),
        StrokeKind::Inside,
    );
    let t = Pos2::new(x_of(0.0), mid_y - tick * 1.9);
    p.add(egui::Shape::convex_polygon(
        vec![t, t + vec2(-7.0, -10.0) * k, t + vec2(7.0, -10.0) * k],
        theme.target,
        Stroke::NONE,
    ));
    if note.is_some() {
        let x = x_of(shown);
        p.line_segment(
            [
                Pos2::new(x, mid_y - tick * 2.4),
                Pos2::new(x, mid_y + tick * 2.4),
            ],
            Stroke::new(4.0, color),
        );
        if shown.abs() > 50.0 {
            let arrow = if shown > 0.0 {
                crate::icons::CARET_RIGHT
            } else {
                crate::icons::CARET_LEFT
            };
            p.text(
                Pos2::new(x, mid_y),
                Align2::CENTER_CENTER,
                arrow,
                theme.arrow_font.clone(),
                color,
            );
        }
    }
    if fresh {
        ui.ctx().request_repaint();
    } else {
        ui.ctx().request_repaint_after(Duration::from_millis(100));
    }
}
