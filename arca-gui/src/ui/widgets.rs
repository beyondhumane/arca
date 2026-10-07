use super::theme::{self, hex, palette_of, BLUE, BLUE_ACTIVE, BLUE_HOVER, WHITE};
use eframe::egui::{
    self, Color32, Response, RichText, Sense, Stroke, Ui, Vec2, WidgetInfo, WidgetType,
};

/// A square button holding one glyph, named for screen readers by its tip.
pub(crate) fn icon_button(ui: &mut Ui, glyph: &str, tip: &str, enabled: bool) -> Response {
    icon_toggle(ui, glyph, tip, enabled, false)
}

pub(crate) fn icon_toggle(
    ui: &mut Ui,
    glyph: &str,
    tip: &str,
    enabled: bool,
    selected: bool,
) -> Response {
    let p = palette_of(ui.ctx());
    let text = theme::icon_text(glyph).size(16.0);
    let button = egui::Button::new(text)
        .min_size(Vec2::splat(28.0))
        .selected(selected)
        .frame_when_inactive(selected);
    let response = ui.add_enabled(enabled, button);
    let tip = tip.to_string();
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, enabled, &tip));
    let response = response.on_hover_text(tip.as_str());
    if response.has_focus() {
        ui.painter().rect_stroke(
            response.rect,
            6,
            Stroke::new(2.0, hex(p.focus)),
            egui::StrokeKind::Inside,
        );
    }
    response
}

/// A button with a glyph and a word next to it.
pub(crate) fn labelled_button(ui: &mut Ui, glyph: &str, label: &str, enabled: bool) -> Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(format!("{glyph}  {label}")).min_size(Vec2::new(0.0, 28.0)),
    )
}

/// The blue call to action of a dialog.
pub(crate) fn primary_button(ui: &mut Ui, label: &str, enabled: bool) -> Response {
    ui.scope(|ui| {
        let (rest, hover) = if ui.style().visuals.dark_mode {
            (BLUE, BLUE_HOVER)
        } else {
            (BLUE_HOVER, BLUE_ACTIVE)
        };
        let widgets = &mut ui.visuals_mut().widgets;
        widgets.inactive.weak_bg_fill = hex(rest);
        widgets.hovered.weak_bg_fill = hex(hover);
        widgets.active.weak_bg_fill = hex(BLUE_ACTIVE);
        ui.add_enabled(
            enabled,
            egui::Button::new(RichText::new(label).color(hex(WHITE)))
                .min_size(Vec2::new(88.0, 30.0)),
        )
    })
    .inner
}

pub(crate) fn region(ui: &Ui, role: egui::accesskit::Role, label: &str) {
    ui.ctx().accesskit_node_builder(ui.id(), |node| {
        node.set_role(role);
        node.set_label(label.to_string());
    });
}

pub(crate) fn danger_button(ui: &mut Ui, label: &str) -> Response {
    let p = palette_of(ui.ctx());
    ui.add(
        egui::Button::new(RichText::new(label).color(hex(p.state_foreground)))
            .fill(hex(p.danger))
            .min_size(Vec2::new(88.0, 30.0)),
    )
}

pub(crate) fn secondary_button(ui: &mut Ui, label: &str, enabled: bool) -> Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(label).min_size(Vec2::new(88.0, 30.0)),
    )
}

pub(crate) fn heading(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).font(theme::display(17.0)));
}

pub(crate) fn muted(ui: &mut Ui, text: impl Into<String>) -> Response {
    let p = palette_of(ui.ctx());
    ui.label(RichText::new(text.into()).color(hex(p.muted)))
}

pub(crate) fn group_label(ui: &mut Ui, text: &str) {
    let p = palette_of(ui.ctx());
    ui.add_space(6.0);
    ui.label(
        RichText::new(text.to_uppercase())
            .size(10.5)
            .color(hex(p.muted)),
    );
}

/// A key cap, as drawn in the shortcut sheet and the footer.
pub(crate) fn kbd(ui: &mut Ui, keys: &str) {
    let p = palette_of(ui.ctx());
    egui::Frame::new()
        .fill(hex(p.raised))
        .stroke(Stroke::new(1.0, hex(p.border)))
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(5, 1))
        .show(ui, |ui| {
            ui.label(RichText::new(keys).monospace().size(11.0));
        });
}

/// A row in a list of places or folders, with room for a glyph.
pub(crate) fn nav_item(
    ui: &mut Ui,
    glyph: &str,
    label: &str,
    selected: bool,
    rail: bool,
) -> Response {
    let p = palette_of(ui.ctx());
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 26.0), Sense::click());
    let visuals = ui.style().interact_selectable(&response, selected);
    if selected || response.hovered() || response.has_focus() {
        let fill = if selected {
            hex(p.selected)
        } else {
            theme::alpha(p.focus, 0.10)
        };
        ui.painter().rect_filled(rect, 6, fill);
    }
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            6,
            Stroke::new(1.5, hex(p.focus)),
            egui::StrokeKind::Inside,
        );
    }
    let glyph_at = if rail {
        rect.center()
    } else {
        egui::pos2(rect.left() + 14.0, rect.center().y)
    };
    ui.painter().text(
        glyph_at,
        egui::Align2::CENTER_CENTER,
        glyph,
        theme::icon_font(15.0),
        if selected { hex(p.focus) } else { hex(p.muted) },
    );
    if !rail {
        let galley = ui.painter().layout(
            label.to_string(),
            egui::FontId::proportional(13.0),
            visuals.text_color(),
            rect.width() - 36.0,
        );
        ui.painter().galley(
            egui::pos2(rect.left() + 30.0, rect.center().y - galley.size().y / 2.0),
            galley,
            Color32::PLACEHOLDER,
        );
    }
    let label = label.to_string();
    response
        .widget_info(|| WidgetInfo::selected(WidgetType::SelectableLabel, true, selected, &label));
    if rail {
        response.on_hover_text(label.as_str())
    } else {
        response
    }
}

/// A thin horizontal bar of progress, with an accessible value.
pub(crate) fn progress(ui: &mut Ui, fraction: Option<f32>) -> Response {
    let p = palette_of(ui.ctx());
    let mut bar = egui::ProgressBar::new(fraction.unwrap_or(0.0))
        .desired_height(6.0)
        .fill(hex(p.spark))
        .corner_radius(3);
    if fraction.is_none() {
        bar = bar.animate(true);
    }
    ui.add(bar)
}
