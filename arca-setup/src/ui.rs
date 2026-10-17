use crate::theme::{
    self, alpha, color, BLUE, BLUE_2, BLUE_HOVER, BLUE_PRESSED, INK, LINE, MUTED, PALE, PALE_HOVER,
    SLATE, SWITCH_OFF, TRACK, WHITE,
};
use eframe::egui::{
    self, pos2, vec2, Color32, CursorIcon, Painter, Pos2, Rect, Response, Sense, Shadow, Shape,
    Stroke, StrokeKind, Ui, WidgetInfo, WidgetType,
};
use std::f32::consts::TAU;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Glyph {
    ArrowRight,
    Check,
    ChevronLeft,
    Folder,
    Minus,
    Close,
}

pub fn glyph(painter: &Painter, rect: Rect, which: Glyph, tint: Color32) {
    let size = rect.width();
    let stroke = Stroke::new((size / 9.).max(1.4), tint);
    let at = |x: f32, y: f32| pos2(rect.left() + x * size, rect.top() + y * size);
    match which {
        Glyph::ArrowRight => {
            painter.line_segment([at(0.2, 0.5), at(0.8, 0.5)], stroke);
            painter.add(Shape::line(
                vec![at(0.55, 0.25), at(0.8, 0.5), at(0.55, 0.75)],
                stroke,
            ));
        }
        Glyph::Check => {
            painter.add(Shape::line(
                vec![at(0.2, 0.52), at(0.42, 0.72), at(0.8, 0.3)],
                stroke,
            ));
        }
        Glyph::ChevronLeft => {
            painter.add(Shape::line(
                vec![at(0.62, 0.22), at(0.36, 0.5), at(0.62, 0.78)],
                stroke,
            ));
        }
        Glyph::Folder => {
            painter.add(Shape::closed_line(
                vec![
                    at(0.12, 0.26),
                    at(0.4, 0.26),
                    at(0.5, 0.36),
                    at(0.88, 0.36),
                    at(0.88, 0.78),
                    at(0.12, 0.78),
                ],
                stroke,
            ));
        }
        Glyph::Minus => {
            painter.line_segment([at(0.25, 0.5), at(0.75, 0.5)], stroke);
        }
        Glyph::Close => {
            painter.line_segment([at(0.28, 0.28), at(0.72, 0.72)], stroke);
            painter.line_segment([at(0.72, 0.28), at(0.28, 0.72)], stroke);
        }
    }
}

pub fn spinner(painter: &Painter, center: Pos2, radius: f32, tint: Color32, time: f64) {
    let start = (time / 0.9).fract() as f32 * TAU;
    let points = (0..=24)
        .map(|step| {
            let angle = start + step as f32 / 24. * TAU * 0.75;
            center + vec2(angle.cos(), angle.sin()) * radius
        })
        .collect();
    painter.add(Shape::line(points, Stroke::new(2., tint)));
}

fn focus_ring(ui: &Ui, response: &Response, rect: Rect, radius: f32) {
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect.expand(2.),
            radius + 2.,
            Stroke::new(2., color(BLUE_2)),
            StrokeKind::Outside,
        );
    }
}

pub fn primary_button(ui: &mut Ui, label: &str, width: Option<f32>, arrow: bool) -> Response {
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), theme::display(15.), color(WHITE));
    let text = galley.size();
    let content = text.x + if arrow { 27. } else { 0. };
    let (rect, response) =
        ui.allocate_exact_size(vec2(width.unwrap_or(content + 48.), 46.), Sense::click());
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, ui.is_enabled(), label));
    if ui.is_rect_visible(rect) {
        let fill = if response.is_pointer_button_down_on() {
            BLUE_PRESSED
        } else if response.hovered() {
            BLUE_HOVER
        } else {
            BLUE
        };
        let painter = ui.painter();
        painter.add(
            Shadow {
                offset: [0, 8],
                blur: 20,
                spread: 0,
                color: alpha(BLUE, 0.30),
            }
            .as_shape(rect, 12),
        );
        painter.rect_filled(rect, 12, color(fill));
        let left = rect.center().x - content / 2.;
        painter.galley(
            pos2(left, rect.center().y - text.y / 2.),
            galley,
            color(WHITE),
        );
        if arrow {
            glyph(
                painter,
                Rect::from_center_size(pos2(left + content - 8.5, rect.center().y), vec2(17., 17.)),
                Glyph::ArrowRight,
                color(WHITE),
            );
        }
        focus_ring(ui, &response, rect, 12.);
    }
    response.on_hover_cursor(CursorIcon::PointingHand)
}

pub fn secondary_button(ui: &mut Ui, label: &str, leading: Option<Glyph>) -> Response {
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), theme::display(15.), color(BLUE));
    let text = galley.size();
    let content = text.x + if leading.is_some() { 24. } else { 0. };
    let (rect, response) = ui.allocate_exact_size(vec2(content + 48., 46.), Sense::click());
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, ui.is_enabled(), label));
    if ui.is_rect_visible(rect) {
        let fill = if response.is_pointer_button_down_on() {
            LINE
        } else if response.hovered() {
            PALE_HOVER
        } else {
            PALE
        };
        let painter = ui.painter();
        painter.rect(
            rect,
            12,
            color(fill),
            Stroke::new(1., color(LINE)),
            StrokeKind::Inside,
        );
        let mut left = rect.center().x - content / 2.;
        if let Some(which) = leading {
            glyph(
                painter,
                Rect::from_center_size(pos2(left + 8., rect.center().y), vec2(16., 16.)),
                which,
                color(BLUE),
            );
            left += 24.;
        }
        painter.galley(
            pos2(left, rect.center().y - text.y / 2.),
            galley,
            color(BLUE),
        );
        focus_ring(ui, &response, rect, 12.);
    }
    response.on_hover_cursor(CursorIcon::PointingHand)
}

pub fn link(ui: &mut Ui, label: &str) -> Response {
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), theme::text(13.5), color(BLUE));
    let (rect, response) = ui.allocate_exact_size(galley.size() + vec2(0., 2.), Sense::click());
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Link, ui.is_enabled(), label));
    let tint = color(if response.hovered() {
        BLUE_PRESSED
    } else {
        BLUE
    });
    let painter = ui.painter();
    let bottom = rect.top() + galley.size().y;
    painter.galley(rect.min, galley, tint);
    painter.line_segment(
        [pos2(rect.left(), bottom), pos2(rect.right(), bottom)],
        Stroke::new(1., tint),
    );
    focus_ring(ui, &response, rect, 3.);
    response.on_hover_cursor(CursorIcon::PointingHand)
}

pub fn switch(ui: &mut Ui, on: bool, label: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(42., 24.), Sense::click());
    response.widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, ui.is_enabled(), on, label));
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        painter.rect_filled(rect, 12, color(if on { BLUE } else { SWITCH_OFF }));
        let knob = if on {
            pos2(rect.right() - 12., rect.center().y)
        } else {
            pos2(rect.left() + 12., rect.center().y)
        };
        painter.circle_filled(knob + vec2(0., 1.), 9.5, alpha(INK, 0.18));
        painter.circle_filled(knob, 9., color(WHITE));
        focus_ring(ui, &response, rect, 12.);
    }
    response.on_hover_cursor(CursorIcon::PointingHand)
}

pub fn card() -> egui::Frame {
    egui::Frame::new()
        .fill(alpha(WHITE, 0.82))
        .stroke(Stroke::new(1., alpha(LINE, 0.9)))
        .corner_radius(16)
        .shadow(Shadow {
            offset: [0, 10],
            blur: 28,
            spread: 0,
            color: alpha(INK, 0.06),
        })
        .inner_margin(18)
}

pub fn progress_bar(ui: &mut Ui, percent: f32, width: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 12.), Sense::hover());
    response.widget_info(|| {
        WidgetInfo::labeled(
            WidgetType::ProgressIndicator,
            true,
            format!("{}%", percent.floor() as u32),
        )
    });
    let painter = ui.painter();
    painter.rect_filled(rect, 6, color(TRACK));
    let filled = rect.width() * (percent / 100.).clamp(0., 1.);
    if filled > 0. {
        let bar = Rect::from_min_size(rect.min, vec2(filled.max(12.), rect.height()));
        painter.rect_filled(bar, 6, color(BLUE));
        let shine = Rect::from_min_max(pos2(bar.center().x, bar.top()), bar.max);
        painter.rect_filled(shine, 6, alpha(BLUE_2, 0.45));
    }
    response
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StepState {
    Done,
    Active,
    Pending,
}

pub fn step_row(ui: &mut Ui, state: StepState, label: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 12.;
        let (rect, _) = ui.allocate_exact_size(vec2(20., 20.), Sense::hover());
        let painter = ui.painter();
        let center = rect.center();
        match state {
            StepState::Done => {
                painter.circle_filled(center, 10., color(BLUE));
                glyph(
                    painter,
                    Rect::from_center_size(center, vec2(12., 12.)),
                    Glyph::Check,
                    color(WHITE),
                );
            }
            StepState::Active => {
                painter.circle_stroke(center, 9., Stroke::new(2., color(TRACK)));
                spinner(
                    painter,
                    center,
                    9.,
                    color(BLUE),
                    ui.input(|input| input.time),
                );
                ui.ctx().request_repaint();
            }
            StepState::Pending => {
                painter.circle_stroke(center, 9., Stroke::new(2., color(MUTED)));
            }
        }
        ui.label(
            egui::RichText::new(label)
                .font(theme::text(14.))
                .color(color(SLATE)),
        );
    });
}

pub fn window_control(
    ui: &mut Ui,
    rect: Rect,
    which: Glyph,
    closes: bool,
    label: &str,
) -> Response {
    let response = ui.interact(rect, ui.id().with(label), Sense::click());
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, label));
    let hovered = response.hovered();
    if hovered {
        ui.painter().rect_filled(
            rect,
            0,
            if closes {
                Color32::from_rgb(0xE8, 0x11, 0x23)
            } else {
                alpha(INK, 0.07)
            },
        );
    }
    glyph(
        ui.painter(),
        Rect::from_center_size(rect.center(), vec2(15., 15.)),
        which,
        color(if closes && hovered { WHITE } else { INK }),
    );
    response
}
