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

/// The blue call to action of a dialog.
/// A context-menu entry: glyph, label and the shortcut at the far end. The
/// glyph comes from the icon font, so it is laid out as its own section
/// rather than relying on the text font to carry it.
fn entry_job(ui: &Ui, glyph: &str, label: &str, enabled: bool) -> egui::text::LayoutJob {
    let p = palette_of(ui.ctx());
    let style = ui.style();
    let ink = if enabled {
        style.visuals.text_color()
    } else {
        style.visuals.weak_text_color()
    };
    // The trailing dots belong to entries that go on to ask where; a plain
    // verb reads better without them, even when a picker follows.
    let label = label.trim_end_matches('…');
    let mut job = egui::text::LayoutJob::default();
    job.append(
        glyph,
        0.0,
        egui::TextFormat {
            font_id: theme::icon_font(15.0),
            color: if enabled { hex(p.muted) } else { ink },
            valign: egui::Align::Center,
            ..Default::default()
        },
    );
    job.append(
        label,
        9.0,
        egui::TextFormat {
            font_id: egui::TextStyle::Body.resolve(style),
            color: ink,
            valign: egui::Align::Center,
            ..Default::default()
        },
    );
    job
}

/// A context-menu entry: glyph, label and the shortcut at the far end. The
/// glyph comes from the icon font, so it is laid out as its own section
/// rather than relying on the text font to carry it.
pub(crate) fn menu_entry(
    ui: &mut Ui,
    glyph: &str,
    label: &str,
    keys: &str,
    enabled: bool,
) -> Response {
    let p = palette_of(ui.ctx());
    let job = entry_job(ui, glyph, label, enabled);
    ui.add_enabled(
        enabled,
        egui::Button::new(job).shortcut_text(egui::RichText::new(keys).color(hex(p.muted))),
    )
}

/// A menu entry that is one of a set, with a tick where the shortcut goes
/// when it is the one in force.
pub(crate) fn pick_entry(ui: &mut Ui, glyph: &str, label: &str, on: bool) -> Response {
    let p = palette_of(ui.ctx());
    let job = entry_job(ui, glyph, label, true);
    let mark = if on {
        theme::icon_text(egui_phosphor::regular::CHECK).color(hex(p.focus))
    } else {
        egui::RichText::new(" ")
    };
    ui.add(egui::Button::new(job).shortcut_text(mark))
}

/// A sort-menu entry: the column's glyph and label, with the direction
/// arrow where the shortcut goes for the column in force.
pub(crate) fn sort_entry(
    ui: &mut Ui,
    glyph: &str,
    label: &str,
    on: bool,
    ascending: bool,
) -> Response {
    let p = palette_of(ui.ctx());
    let job = entry_job(ui, glyph, label, true);
    let mark = if on {
        let arrow = if ascending {
            egui_phosphor::regular::ARROW_UP
        } else {
            egui_phosphor::regular::ARROW_DOWN
        };
        theme::icon_text(arrow).color(hex(p.focus))
    } else {
        egui::RichText::new(" ")
    };
    ui.add(egui::Button::new(job).shortcut_text(mark))
}

/// A one-line text field as tall as the buttons beside it, with the text
/// centred in it; egui's own comes out shorter and sits oddly in a row.
pub(crate) fn field<'t>(ui: &Ui, text: &'t mut dyn egui::TextBuffer) -> egui::TextEdit<'t> {
    let tall = ui.spacing().interact_size.y;
    egui::TextEdit::singleline(text)
        .min_size(egui::vec2(0.0, tall))
        .vertical_align(egui::Align::Center)
        .margin(egui::Margin::symmetric(8, 4))
}

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
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label(
            RichText::new(text.to_uppercase())
                .size(10.5)
                .strong()
                .extra_letter_spacing(0.6)
                .color(hex(p.muted)),
        );
    });
    ui.add_space(2.0);
}

/// A settings card: a title, an optional description, and its control.
pub(crate) fn card(
    ui: &mut Ui,
    title: &str,
    description: Option<&str>,
    inline: bool,
    control: impl FnOnce(&mut Ui),
) {
    let p = palette_of(ui.ctx());
    egui::Frame::new()
        .fill(theme::alpha(p.background, 0.55))
        .stroke(Stroke::new(1.0, theme::alpha(p.border, 0.6)))
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let words = |ui: &mut Ui| {
                ui.vertical(|ui| {
                    ui.set_max_width(ui.available_width());
                    ui.label(RichText::new(title.trim_end_matches(':')).strong());
                    if let Some(description) = description {
                        ui.add(
                            egui::Label::new(
                                RichText::new(description).size(12.0).color(hex(p.muted)),
                            )
                            .wrap(),
                        );
                    }
                });
            };
            if inline {
                // The control goes down first so that the words get what is
                // left, and a clear gap, instead of running into it.
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        control(ui);
                        ui.add_space(24.0);
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), words);
                    });
                });
            } else {
                words(ui);
                ui.add_space(6.0);
                control(ui);
            }
        });
    ui.add_space(8.0);
}

/// A section title inside a settings page.
pub(crate) fn section_title(ui: &mut Ui, text: &str) {
    let p = palette_of(ui.ctx());
    ui.add_space(6.0);
    ui.label(
        RichText::new(text.to_uppercase())
            .size(11.0)
            .strong()
            .extra_letter_spacing(1.0)
            .color(hex(p.muted)),
    );
    ui.add_space(6.0);
}

/// An on/off switch with the semantics of a checkbox.
pub(crate) fn switch(ui: &mut Ui, on: &mut bool, label: &str) -> Response {
    let p = palette_of(ui.ctx());
    let size = Vec2::new(34.0, 18.0);
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    let t = ui.ctx().animate_bool_responsive(response.id, *on);
    let track = if *on { hex(p.focus) } else { hex(p.raised) };
    ui.painter().rect(
        rect,
        9,
        track,
        Stroke::new(1.0, hex(p.border)),
        egui::StrokeKind::Inside,
    );
    let x = egui::lerp(rect.left() + 9.0..=rect.right() - 9.0, t);
    let knob = if *on {
        hex(p.state_foreground)
    } else {
        hex(p.muted)
    };
    ui.painter()
        .circle_filled(egui::pos2(x, rect.center().y), 6.0, knob);
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect.expand(2.0),
            11,
            Stroke::new(1.5, hex(p.focus)),
            egui::StrokeKind::Outside,
        );
    }
    let label = label.to_string();
    let value = *on;
    response.widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, true, value, &label));
    response
}

/// A row of mutually exclusive choices drawn as one segmented control.
pub(crate) fn segmented(ui: &mut Ui, choices: &[(&str, bool)]) -> Option<usize> {
    let p = palette_of(ui.ctx());
    let mut picked = None;
    egui::Frame::new()
        .fill(hex(p.raised))
        .corner_radius(6)
        .inner_margin(egui::Margin::same(2))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let gap = 2.0;
            let count = choices.len().max(1) as f32;
            let each = (ui.available_width() - gap * (count - 1.0)) / count;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for (index, (label, on)) in choices.iter().enumerate() {
                    let (rect, response) =
                        ui.allocate_exact_size(Vec2::new(each, 26.0), Sense::click());
                    let fill = if *on {
                        theme::alpha(p.focus, 0.28)
                    } else if response.hovered() {
                        theme::alpha(p.focus, 0.12)
                    } else {
                        Color32::TRANSPARENT
                    };
                    ui.painter().rect_filled(rect, 5, fill);
                    if response.has_focus() {
                        ui.painter().rect_stroke(
                            rect,
                            5,
                            Stroke::new(1.5, hex(p.focus)),
                            egui::StrokeKind::Inside,
                        );
                    }
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        *label,
                        egui::FontId::proportional(13.0),
                        if *on { hex(p.text) } else { hex(p.muted) },
                    );
                    let text = label.to_string();
                    let selected = *on;
                    response.widget_info(|| {
                        WidgetInfo::selected(WidgetType::RadioButton, true, selected, &text)
                    });
                    if response.clicked() {
                        picked = Some(index);
                    }
                }
            });
        });
    picked
}

/// A key cap, as drawn in the shortcut sheet and the footer.
/// `− 13 +` in one piece: returns -1 or +1 when a side is pressed.
pub(crate) fn stepper(ui: &mut Ui, shown: &str, can_dec: bool, can_inc: bool) -> Option<i8> {
    let p = palette_of(ui.ctx());
    let mut step = None;
    let tall = ui.spacing().interact_size.y;
    egui::Frame::new()
        .fill(hex(p.raised))
        .stroke(Stroke::new(1.0, hex(p.border)))
        .corner_radius(6)
        .show(ui, |ui| {
            // Always minus, number, plus: `horizontal` would follow a
            // right-to-left parent and turn it round.
            let want = egui::vec2(2.0 * (tall + 6.0) + tall + 10.0, tall);
            let layout = egui::Layout::left_to_right(egui::Align::Center);
            ui.allocate_ui_with_layout(want, layout, |ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let side = |ui: &mut Ui, label: &str, on: bool| {
                    ui.add_enabled(
                        on,
                        egui::Button::new(RichText::new(label).size(15.0).color(hex(p.focus)))
                            .frame(false)
                            .min_size(egui::vec2(tall + 6.0, tall)),
                    )
                };
                if side(ui, "−", can_dec).clicked() {
                    step = Some(-1);
                }
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(tall + 10.0, tall), Sense::hover());
                ui.painter()
                    .vline(rect.left(), rect.y_range(), Stroke::new(1.0, hex(p.border)));
                ui.painter().vline(
                    rect.right(),
                    rect.y_range(),
                    Stroke::new(1.0, hex(p.border)),
                );
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    shown,
                    egui::TextStyle::Body.resolve(ui.style()),
                    hex(p.text),
                );
                if side(ui, "+", can_inc).clicked() {
                    step = Some(1);
                }
            });
        });
    step
}

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
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 30.0), Sense::click());
    let visuals = ui.style().interact_selectable(&response, selected);
    if response.dnd_hover_payload::<super::DraggedRows>().is_some() {
        ui.painter().rect_stroke(
            rect,
            6,
            Stroke::new(1.5, hex(p.spark)),
            egui::StrokeKind::Inside,
        );
    }
    if selected || response.hovered() || response.has_focus() {
        let fill = if selected {
            theme::alpha(p.focus, 0.18)
        } else {
            theme::alpha(p.focus, 0.12)
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
        let mut job = egui::text::LayoutJob::single_section(
            label.to_string(),
            egui::TextFormat::simple(egui::FontId::proportional(13.0), visuals.text_color()),
        );
        job.wrap = egui::text::TextWrapping::truncate_at_width(rect.width() - 36.0);
        let galley = ui.painter().layout_job(job);
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
