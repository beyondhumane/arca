use super::theme::{alpha, hex, palette_of, Palette};
use super::widgets::*;
use super::*;
use crate::tree::Kind;
use eframe::egui::{self, Align2, FontId, Rect, RichText, Sense, Stroke, StrokeKind, Vec2};
use egui_phosphor::regular as icon;

#[cfg(test)]
pub(crate) const ROW_HEIGHT: f32 = 24.0;
/// Space around the rows that belongs to the list but to no row.
const GUTTER: f32 = 10.0;

pub(crate) fn column_slot(column: SortColumn) -> usize {
    Columns::ALL
        .iter()
        .position(|(candidate, _)| *candidate == column)
        .map_or(0, |index| index + 1)
}

pub(crate) fn kind_glyph(kind: Kind) -> &'static str {
    match kind {
        Kind::Dir => icon::FOLDER,
        Kind::Image => icon::FILE_IMAGE,
        Kind::Text => icon::FILE_TEXT,
        Kind::Archive => icon::FILE_ZIP,
        Kind::Audio => icon::FILE_AUDIO,
        Kind::Video => icon::FILE_VIDEO,
        Kind::Other => icon::FILE,
    }
}

fn right_aligned(column: SortColumn) -> bool {
    matches!(
        column,
        SortColumn::Size | SortColumn::Packed | SortColumn::Saved
    )
}

pub(crate) fn scroll_target(offset: f32, height: f32, index: usize, row: f32) -> f32 {
    let top = index as f32 * row;
    let bottom = top + row;
    if top < offset {
        top
    } else if bottom > offset + height {
        bottom - height
    } else {
        offset
    }
}

struct RowLook {
    checked: bool,
    cursor: bool,
    open: bool,
    cut: bool,
}

fn paint_row_back(
    ui: &egui::Ui,
    rect: Rect,
    response: &egui::Response,
    look: &RowLook,
    p: Palette,
) {
    let checked = ui
        .ctx()
        .animate_bool(response.id.with("checked"), look.checked);
    let hover = ui
        .ctx()
        .animate_bool(response.id.with("hover"), response.hovered());
    let painter = ui.painter();
    if look.open && !look.checked {
        painter.rect_filled(rect, 4, alpha(p.selected, 0.55));
    } else if hover > 0.0 && !look.checked {
        painter.rect_filled(rect, 4, alpha(p.focus, 0.08 * hover));
    }
    if checked > 0.0 {
        painter.rect_filled(rect, 4, alpha(p.selected, checked));
    }
    if look.cursor {
        painter.rect_stroke(
            rect.shrink(0.5),
            4,
            Stroke::new(1.0, hex(p.focus)),
            StrokeKind::Inside,
        );
    }
    if response.dnd_hover_payload::<DraggedRows>().is_some() {
        painter.rect_stroke(rect, 4, Stroke::new(2.0, hex(p.spark)), StrokeKind::Inside);
    }
}

fn paint_name(ui: &egui::Ui, rect: Rect, row: &Row, look: &RowLook, p: Palette) {
    let painter = ui.painter_at(rect);
    let glyph = if row.up {
        icon::ARROW_BEND_LEFT_UP
    } else {
        kind_glyph(row.kind)
    };
    let glyph_color = if row.is_dir { p.focus } else { p.muted };
    let font = FontId::proportional(14.0);
    painter.text(
        rect.left_center() + Vec2::new(6.0, 0.0),
        Align2::LEFT_CENTER,
        glyph,
        super::theme::icon_font(15.0),
        hex(glyph_color),
    );
    let mut x = rect.left() + 26.0;
    if row.encrypted {
        painter.text(
            egui::pos2(x, rect.center().y),
            Align2::LEFT_CENTER,
            icon::LOCK_SIMPLE,
            super::theme::icon_font(12.0),
            hex(p.warning),
        );
        x += 16.0;
    }
    let color = if look.cut { p.muted } else { p.text };
    painter.text(
        egui::pos2(x, rect.center().y),
        Align2::LEFT_CENTER,
        &row.label,
        font,
        hex(color),
    );
}

impl Shell {
    pub(super) fn browser(&mut self, ui: &mut egui::Ui) {
        if self.controller.state.archive.is_none() && !self.controller.on_disk() {
            self.welcome(ui);
            return;
        }
        // A gutter on all four sides, so that a selection rectangle can be
        // dragged out from anywhere around the rows, not only under them.
        let columns = self.controller.state.browser.columns;
        let pane = columns.then_some(self.controller.state.browser.active);
        let outer = ui.available_rect_before_wrap();
        let response = ui.interact(outer, ui.id().with("gutter"), Sense::click_and_drag());
        self.blank_events(response, pane, ui.painter_at(outer));
        let inner = outer.shrink2(Vec2::new(GUTTER, GUTTER * 0.6));
        ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
            if columns {
                self.columns_view(ui);
            } else {
                self.details(ui);
            }
        });
    }

    fn welcome(&mut self, ui: &mut egui::Ui) {
        let s = self.controller.s();
        let p = palette_of(ui.ctx());
        let idle = self.background_idle();
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() / 2.0 - 120.0).max(24.0));
            ui.add(
                egui::Image::from_bytes("bytes://arca-mark.svg", crate::assets::MARK)
                    .fit_to_exact_size(Vec2::splat(72.0)),
            );
            ui.add_space(10.0);
            if self.controller.state.error {
                ui.label(
                    RichText::new(s.cannot_open)
                        .font(theme::display(22.0))
                        .color(hex(p.danger)),
                );
            } else {
                ui.label(RichText::new("Arca").font(theme::display(24.0)));
            }
            ui.label(RichText::new(s.drop_here).color(hex(p.muted)));
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                let width = 2.0 * 140.0 + ui.spacing().item_spacing.x;
                ui.add_space((ui.available_width() - width).max(0.0) / 2.0);
                if primary_button(ui, s.open, idle).clicked() {
                    self.begin_dialog(DialogKind::Open);
                }
                if secondary_button(ui, s.compress, idle).clicked() {
                    self.controller
                        .dispatch(AppAction::PrepareCompress(Vec::new()));
                }
            });
        });
    }

    fn empty_state(&self, ui: &mut egui::Ui) {
        let s = self.controller.s();
        let p = palette_of(ui.ctx());
        let state = &self.controller.state;
        let text = if state.archive.is_some()
            && state.entries.is_empty()
            && state.filter.trim().is_empty()
        {
            s.empty_archive
        } else {
            empty_state_aria_label(state.error, &state.filter, s)
        };
        ui.add_space(40.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new(text).color(hex(p.muted)));
        });
    }

    fn details(&mut self, ui: &mut egui::Ui) {
        let s = self.controller.s();
        let p = palette_of(ui.ctx());
        let rows = self.controller.visible_rows();
        let settings = &self.controller.state.settings;
        let mut shown = vec![SortColumn::Name];
        shown.extend(
            Columns::ALL
                .iter()
                .map(|(column, _)| *column)
                .filter(|column| settings.columns.on(*column)),
        );
        let width_of = |column: SortColumn| {
            settings
                .widths
                .get(column_slot(column))
                .copied()
                .unwrap_or(CELL_WIDE)
        };
        let others: f32 = shown.iter().skip(1).map(|c| width_of(*c)).sum();
        let available = ui.available_width() - 8.0;
        let widths: Vec<f32> = shown
            .iter()
            .map(|column| {
                if *column == SortColumn::Name {
                    width_of(*column).max(available - others).max(NAME_LEAST)
                } else {
                    width_of(*column)
                }
            })
            .collect();
        let total: f32 = widths.iter().sum();
        let order = self.controller.state.order;
        let current_dir = self.controller.row_root().to_string();
        egui::ScrollArea::horizontal()
            .id_salt("details-across")
            .auto_shrink(false)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let (header, _) =
                    ui.allocate_exact_size(Vec2::new(total, self.row_h() + 2.0), Sense::hover());
                ui.painter().rect_filled(header, 0, hex(p.surface));
                let mut x = header.left();
                let mut resized = None;
                let mut edges = Vec::with_capacity(shown.len());
                for (slot, column) in shown.iter().enumerate() {
                    let rect = Rect::from_min_size(
                        egui::pos2(x, header.top()),
                        Vec2::new(widths[slot], header.height()),
                    );
                    x += widths[slot];
                    let label = if *column == SortColumn::Name {
                        s.col_name
                    } else {
                        Columns::label(*column, s)
                    };
                    let response = ui
                        .interact(rect, ui.id().with(("column", slot)), Sense::click())
                        .on_hover_text(format!("{} {label}", s.sort_by));
                    let sorted = order.0 == *column;
                    let arrow = sorted.then_some(if order.1 {
                        icon::CARET_UP
                    } else {
                        icon::CARET_DOWN
                    });
                    let text = if sorted {
                        let way = if order.1 { s.ascending } else { s.descending };
                        format!("{label}, {way}")
                    } else {
                        label.to_string()
                    };
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &text)
                    });
                    let (anchor, at) = if right_aligned(*column) {
                        (
                            Align2::RIGHT_CENTER,
                            rect.right_center() - Vec2::new(10.0, 0.0),
                        )
                    } else {
                        (
                            Align2::LEFT_CENTER,
                            rect.left_center() + Vec2::new(8.0, 0.0),
                        )
                    };
                    let color = if sorted || response.hovered() {
                        p.text
                    } else {
                        p.muted
                    };
                    let mut job = egui::text::LayoutJob::default();
                    job.append(
                        label,
                        0.0,
                        egui::TextFormat::simple(FontId::proportional(12.5), hex(color)),
                    );
                    if let Some(arrow) = arrow {
                        job.append(
                            arrow,
                            4.0,
                            egui::TextFormat::simple(super::theme::icon_font(12.0), hex(color)),
                        );
                    }
                    let galley = ui.painter().layout_job(job);
                    let pos = anchor.anchor_size(at, galley.size()).min;
                    ui.painter_at(rect).galley(pos, galley, hex(color));
                    if response.clicked() && self.background_idle() {
                        self.controller.dispatch(AppAction::Sort(*column));
                    }
                    response.context_menu(|ui| {
                        group_label(ui, s.columns_word);
                        for (candidate, _) in Columns::ALL {
                            let mut on = self.controller.state.settings.columns.on(candidate);
                            if ui.checkbox(&mut on, Columns::label(candidate, s)).changed() {
                                self.controller.dispatch(AppAction::ToggleColumn(candidate));
                            }
                        }
                    });
                    edges.push((slot, *column, rect.right()));
                }
                // The grips go down after every header so that they sit on
                // top of them; a grip laid under the next header lost to it.
                for (slot, column, edge) in edges {
                    let handle = Rect::from_min_max(
                        egui::pos2(edge - 4.0, header.top()),
                        egui::pos2(edge + 4.0, header.bottom()),
                    );
                    let drag = ui
                        .interact(handle, ui.id().with(("resize", slot)), Sense::drag())
                        .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
                    let lit = drag.hovered() || drag.dragged();
                    ui.painter().vline(
                        edge - 0.5,
                        header.y_range(),
                        egui::Stroke::new(
                            1.0,
                            if lit {
                                hex(p.focus)
                            } else {
                                super::theme::alpha(p.border, 0.9)
                            },
                        ),
                    );
                    if drag.dragged() {
                        resized = Some((column, widths[slot] + drag.drag_delta().x, false));
                    }
                    if drag.drag_stopped() {
                        resized = Some((column, widths[slot], true));
                    }
                }
                if let Some((column, width, done)) = resized {
                    let least = if column == SortColumn::Name {
                        NAME_LEAST
                    } else {
                        CELL_LEAST
                    };
                    let slot = column_slot(column);
                    let settings = &mut self.controller.state.settings;
                    if let Some(stored) = settings.widths.get_mut(slot) {
                        *stored = width.clamp(least, 900.0);
                    }
                    if done {
                        settings.save();
                    }
                }
                if rows.is_empty() {
                    self.empty_state(ui);
                    self.empty_space(ui, None);
                    return;
                }
                let area_salt = "details-rows";
                let mut area = egui::ScrollArea::vertical()
                    .id_salt(area_salt)
                    .auto_shrink(false);
                if std::mem::take(&mut self.scroll_to_cursor) {
                    if let Some(cursor) = self.controller.state.cursor {
                        let id = ui.make_persistent_id(area_salt);
                        let offset = egui::scroll_area::State::load(ui.ctx(), id)
                            .map_or(0.0, |state| state.offset.y);
                        let height = ui.available_height() - 40.0;
                        area = area.vertical_scroll_offset(scroll_target(
                            offset,
                            height,
                            cursor,
                            self.row_h(),
                        ));
                    }
                }
                let shown = area.show_rows(ui, self.row_h(), rows.len(), |ui, range| {
                    self.autoscroll(ui);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for index in range {
                        let row = &rows[index];
                        let (rect, response) = ui.allocate_exact_size(
                            Vec2::new(total, self.row_h()),
                            Sense::click_and_drag(),
                        );
                        let look = RowLook {
                            checked: self.controller.is_checked(row),
                            cursor: self.controller.state.cursor == Some(index),
                            open: false,
                            cut: self.controller.state.cut_names.contains(&row.path),
                        };
                        paint_row_back(ui, rect, &response, &look, p);
                        let mut x = rect.left();
                        for (slot, column) in shown.iter().enumerate() {
                            let cell = Rect::from_min_size(
                                egui::pos2(x, rect.top()),
                                Vec2::new(widths[slot], self.row_h()),
                            );
                            x += widths[slot];
                            if *column == SortColumn::Name {
                                if self.renaming_here(row) {
                                    self.rename_field(ui, cell.shrink2(Vec2::new(24.0, 1.0)));
                                } else {
                                    paint_name(ui, cell, row, &look, p);
                                }
                                continue;
                            }
                            let text = column_text(row, *column, s, &current_dir);
                            let (anchor, at) = if right_aligned(*column) {
                                (
                                    Align2::RIGHT_CENTER,
                                    cell.right_center() - Vec2::new(10.0, 0.0),
                                )
                            } else {
                                (
                                    Align2::LEFT_CENTER,
                                    cell.left_center() + Vec2::new(8.0, 0.0),
                                )
                            };
                            ui.painter_at(cell).text(
                                at,
                                anchor,
                                text,
                                FontId::proportional(13.0),
                                hex(p.muted),
                            );
                        }
                        self.row_events(ui, &response, row, index, None, look.checked);
                    }
                });
                self.blank_below(ui, &shown, None);
            });
    }

    fn columns_view(&mut self, ui: &mut egui::Ui) {
        let panes = self.controller.state.browser.panes.len();
        let active = self.controller.state.browser.active;
        let wanted = self.controller.state.settings.directory_width;
        let gap = 1.0;
        let room = ui.available_width();
        let (first, width) = column_slots(panes, active, wanted, gap, room);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for pane in first..panes {
                let height = ui.available_height();
                let directory = &self.controller.state.browser.panes[pane].directory;
                let shown = super::theme::appear(ui.ctx(), egui::Id::new(("arca-pane", directory)));
                ui.allocate_ui_with_layout(
                    Vec2::new(width, height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.multiply_opacity(shown);
                        ui.set_width(width);
                        ui.set_height(height);
                        self.pane(ui, pane, width);
                    },
                );
                ui.add(egui::Separator::default().spacing(1.0));
            }
        });
    }

    fn pane(&mut self, ui: &mut egui::Ui, pane: usize, width: f32) {
        let s = self.controller.s();
        let p = palette_of(ui.ctx());
        let rows = self.controller.pane_rows(pane);
        let browser = &self.controller.state.browser;
        let active = browser.active == pane;
        let data = &browser.panes[pane];
        let cursor = if active {
            self.controller.state.cursor
        } else {
            data.cursor
        };
        let next = self.controller.opened_descendant(pane).map(str::to_string);
        let remembered = data.scroll_offset;
        let directory = data.directory.clone();
        let order = data.order;
        let header = ui.horizontal(|ui| {
            let title = directory
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .filter(|name| !name.is_empty())
                .unwrap_or(self.controller.root_label())
                .to_string();
            let color = if active { p.text } else { p.muted };
            ui.add_space(8.0);
            ui.allocate_ui_with_layout(
                egui::vec2(width - 48.0, 26.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.add(
                        egui::Label::new(RichText::new(title).strong().color(hex(color)))
                            .truncate(),
                    )
                },
            );
            ui.menu_button(
                super::theme::icon_text(icon::SORT_ASCENDING).color(hex(p.muted)),
                |ui| {
                    for column in [
                        SortColumn::Name,
                        SortColumn::Size,
                        SortColumn::Modified,
                        SortColumn::Type,
                    ] {
                        let label = if column == SortColumn::Name {
                            s.col_name
                        } else {
                            Columns::label(column, s)
                        };
                        let on = order.0 == column;
                        let text = if on {
                            format!(
                                "{label} {}",
                                if order.1 { s.ascending } else { s.descending }
                            )
                        } else {
                            label.to_string()
                        };
                        if ui.selectable_label(on, text).clicked() {
                            let ascending = if on { !order.1 } else { true };
                            self.controller.set_pane_order(pane, (column, ascending));
                        }
                    }
                },
            )
            .response
            .on_hover_text(s.sort_word);
        });
        let rect = header.response.rect;
        let span = egui::Rangef::new(rect.left(), rect.left() + width);
        ui.painter().hline(
            span,
            rect.bottom() + 1.0,
            egui::Stroke::new(1.0, super::theme::alpha(p.border, 0.6)),
        );
        if active {
            ui.painter()
                .hline(span, rect.top() + 1.0, egui::Stroke::new(2.0, hex(p.focus)));
        }
        ui.add_space(2.0);
        if rows.is_empty() {
            ui.label(RichText::new(s.empty_folder).color(hex(p.muted)));
            self.empty_space(ui, Some(pane));
            return;
        }
        let salt = ("pane-rows", pane);
        let mut area = egui::ScrollArea::vertical()
            .id_salt(salt)
            .auto_shrink(false);
        if egui::scroll_area::State::load(ui.ctx(), ui.make_persistent_id(salt)).is_none() {
            area = area.vertical_scroll_offset(remembered);
        }
        if active && std::mem::take(&mut self.scroll_to_cursor) {
            if let Some(cursor) = cursor {
                let id = ui.make_persistent_id(salt);
                let offset = egui::scroll_area::State::load(ui.ctx(), id)
                    .map_or(0.0, |state| state.offset.y);
                let height = ui.available_height() - 20.0;
                area = area.vertical_scroll_offset(scroll_target(
                    offset,
                    height,
                    cursor,
                    self.row_h(),
                ));
            }
        }
        ui.spacing_mut().item_spacing.y = 0.0;
        let shown = area.show_rows(ui, self.row_h(), rows.len(), |ui, range| {
            self.autoscroll(ui);
            for index in range {
                let row = &rows[index];
                let (rect, response) = ui.allocate_exact_size(
                    Vec2::new(width - 8.0, self.row_h()),
                    Sense::click_and_drag(),
                );
                let checked = self.controller.pane_is_checked(pane, row);
                let look = RowLook {
                    checked,
                    cursor: active && cursor == Some(index),
                    open: next.as_deref() == Some(row.path.as_str()),
                    cut: self.controller.state.cut_names.contains(&row.path),
                };
                paint_row_back(ui, rect, &response, &look, p);
                if self.renaming_here(row) && active {
                    self.rename_field(ui, rect.shrink2(Vec2::new(24.0, 1.0)));
                } else {
                    paint_name(ui, rect.shrink2(Vec2::new(0.0, 0.0)), row, &look, p);
                }
                if row.is_dir && !row.up {
                    ui.painter().text(
                        rect.right_center() - Vec2::new(8.0, 0.0),
                        Align2::RIGHT_CENTER,
                        icon::CARET_RIGHT,
                        super::theme::icon_font(12.0),
                        hex(p.muted),
                    );
                }
                self.row_events(ui, &response, row, index, Some(pane), checked);
            }
        });
        if (shown.state.offset.y - remembered).abs() > 0.5 {
            self.controller.set_pane_scroll(pane, shown.state.offset.y);
        }
        self.blank_below(ui, &shown, Some(pane));
    }

    fn autoscroll(&mut self, ui: &mut egui::Ui) {
        let id = ui.id();
        let (pressed, released, other, pos, dt) = ui.input(|i| {
            (
                i.pointer.button_pressed(egui::PointerButton::Middle),
                i.pointer.button_released(egui::PointerButton::Middle),
                i.pointer.primary_pressed() || i.pointer.secondary_pressed(),
                i.pointer.latest_pos(),
                i.stable_dt.min(0.1),
            )
        });
        match self.anchor {
            Some((owner, anchor)) if owner == id => {
                let far = pos.is_some_and(|pos| (pos.y - anchor.y).abs() > 12.0);
                if pressed || other || (released && far) {
                    self.anchor = None;
                    return;
                }
                if let Some(pos) = pos {
                    let speed = wheel_speed(pos.y - anchor.y);
                    if speed != 0.0 {
                        ui.scroll_with_delta(Vec2::new(0.0, -speed * dt));
                    }
                }
                let p = palette_of(ui.ctx());
                let painter = ui.ctx().layer_painter(egui::LayerId::new(
                    egui::Order::Foreground,
                    egui::Id::new("arca-anchor"),
                ));
                painter.circle_filled(anchor, 11.0, alpha(p.raised, 0.92));
                painter.text(
                    anchor,
                    Align2::CENTER_CENTER,
                    icon::ARROWS_DOWN_UP,
                    super::theme::icon_font(14.0),
                    hex(p.text),
                );
                ui.ctx().set_cursor_icon(egui::CursorIcon::AllScroll);
                ui.ctx().request_repaint();
            }
            _ => {
                let over = pos.is_some_and(|pos| ui.clip_rect().contains(pos));
                if pressed && over {
                    self.anchor = pos.map(|pos| (id, pos));
                }
            }
        }
    }

    fn renaming_here(&self, row: &Row) -> bool {
        !self.renaming_in_tree
            && self
                .controller
                .state
                .renaming
                .as_ref()
                .is_some_and(|(path, _)| *path == row.path)
    }

    fn rename_field(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let response = ui.put(
            rect,
            super::widgets::field(ui, &mut self.rename_value).desired_width(rect.width()),
        );
        if self.focus_rename {
            response.request_focus();
            self.focus_rename = false;
        }
        if response.lost_focus() {
            if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.commit_rename();
            } else {
                self.cancel_rename();
            }
        }
    }

    fn row_events(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        row: &Row,
        index: usize,
        pane: Option<usize>,
        checked: bool,
    ) {
        let s = self.controller.s();
        let spoken = format!(
            "{}, {}",
            row.label,
            if checked { s.checked } else { s.not_checked }
        );
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, checked, &spoken)
        });
        let idle = self.background_idle();
        if let Some(marquee) = self.marquee.as_ref().filter(|m| m.pane == pane) {
            let swept = response.rect.intersects(marquee.rect());
            let want = marquee.base.get(index).copied().unwrap_or(false) || swept;
            if want != checked && !row.up && idle {
                self.controller.dispatch(AppAction::SetChecked {
                    row: row.clone(),
                    value: want,
                });
            }
        }
        let modifiers = ui.input(|i| i.modifiers);
        let activate = |shell: &mut Shell| {
            if let Some(pane) = pane {
                if shell.controller.state.browser.active != pane {
                    shell.controller.activate_pane(pane);
                }
            }
        };
        if response.clicked() && idle {
            activate(self);
            if let (Some(pane), true, false) = (
                pane,
                row.is_dir && !row.up,
                modifiers.shift || modifiers.command,
            ) {
                self.controller.clear_picked();
                self.controller.set_pane_cursor(pane, Some(index));
                self.controller.enter_folder_from_pane(pane, &row.path);
            } else {
                self.select_row(index, modifiers);
            }
        }
        if response.double_clicked() && idle {
            activate(self);
            if row.up {
                self.controller.dispatch(AppAction::Up);
            } else {
                self.row_action(RowAction::Open, index);
            }
        }
        if response.secondary_clicked() && idle {
            activate(self);
            if !checked && !row.up {
                self.controller.state.checked.fill(false);
                self.controller.dispatch(AppAction::SetChecked {
                    row: row.clone(),
                    value: true,
                });
            }
            let active = self.controller.state.browser.active;
            self.controller.set_pane_cursor(active, Some(index));
        }
        if !row.up {
            response.context_menu(|ui| self.row_menu(ui, Some(row), index));
        }
        if response.drag_started() && idle && !row.up {
            let roots = if checked {
                match pane {
                    Some(pane) => self.controller.pane_selected_roots(pane),
                    None => self.controller.selected_roots(),
                }
            } else {
                vec![row.path.clone()]
            };
            response.dnd_set_drag_payload(DraggedRows(roots));
            self.carrying = true;
        }
        if row.is_dir && !row.up {
            if let Some(payload) = response.dnd_release_payload::<DraggedRows>() {
                if idle && self.controller.can_drop() && !payload.0.contains(&row.path) {
                    self.controller.move_into(&payload.0, &row.path);
                }
            }
        }
    }

    fn row_h(&self) -> f32 {
        self.controller.state.settings.row_height()
    }

    fn row_menu(&mut self, ui: &mut egui::Ui, row: Option<&Row>, index: usize) {
        let s = self.controller.s();
        let idle = self.background_idle();
        let writable = self.controller.writable();
        let on_disk = self.controller.on_disk();
        let current_dir = self.controller.state.current_dir.clone();
        let actions: &[RowAction] = if row.is_some() {
            &RowAction::ALL
        } else {
            &RowAction::EMPTY_SPACE
        };
        let mut first = true;
        for action in actions.iter().copied() {
            if !action.offered()
                || !action.shown(on_disk)
                || (action.writable_only() && !writable)
                || row.is_some_and(|row| {
                    (row.is_dir && action == RowAction::View)
                        || (!row.is_dir && action == RowAction::Pin)
                })
            {
                continue;
            }
            if action.starts_group() && !first {
                ui.separator();
            }
            first = false;
            let (label, keys) = action.label(s);
            let label = if row.is_none() && action == RowAction::Compress {
                s.compress
            } else {
                label
            };
            let pin_target = (action == RowAction::Pin).then(|| {
                row.map_or_else(
                    || crate::controller::disk::disk_path(&current_dir),
                    |row| crate::controller::disk::disk_path(&row.path),
                )
            });
            let pinned = pin_target
                .as_ref()
                .is_some_and(|path| self.controller.is_pinned(path));
            let label = if pinned { s.unpin_word } else { label };
            let glyph = if pinned {
                icon::PUSH_PIN_SLASH
            } else {
                action.glyph()
            };
            let label = action.destination_label(label, row, &current_dir, s);
            let enabled = idle
                && match action {
                    RowAction::Paste => self.can_paste_files(),
                    RowAction::Copy | RowAction::Cut => self.can_copy_files() || row.is_some(),
                    _ => true,
                };
            if menu_entry(ui, glyph, &label, keys, enabled).clicked() {
                if row.is_none() {
                    self.controller.dispatch(AppAction::ClearSelection);
                }
                self.row_action(action, index);
            }
        }
    }

    fn empty_space(&mut self, ui: &mut egui::Ui, pane: Option<usize>) {
        let size = ui.available_size().max(Vec2::new(1.0, 1.0));
        let response = ui.allocate_response(size, Sense::click_and_drag());
        let painter = ui.painter_at(ui.clip_rect());
        self.blank_events(response, pane, painter);
    }

    /// The part of a scroll area under its last row. The area keeps the
    /// whole height, so nothing is left to allocate after it; the blank is
    /// worked out from what it shows and gets the same menu as empty space.
    fn blank_below(
        &mut self,
        ui: &mut egui::Ui,
        shown: &egui::scroll_area::ScrollAreaOutput<()>,
        pane: Option<usize>,
    ) {
        let inner = shown.inner_rect;
        let used = (shown.content_size.y - shown.state.offset.y).max(0.0);
        let blank =
            egui::Rect::from_min_max(egui::pos2(inner.left(), inner.top() + used), inner.max);
        if blank.height() < 1.0 {
            return;
        }
        let response = ui.interact(
            blank,
            ui.id().with(("blank", pane)),
            Sense::click_and_drag(),
        );
        self.blank_events(response, pane, ui.painter_at(inner));
    }

    fn blank_events(
        &mut self,
        response: egui::Response,
        pane: Option<usize>,
        painter: egui::Painter,
    ) {
        let clip = painter.clip_rect();
        if response.clicked() && self.background_idle() {
            if let Some(pane) = pane {
                if self.controller.state.browser.active != pane {
                    self.controller.activate_pane(pane);
                }
            }
            self.controller.dispatch(AppAction::ClearSelection);
        }
        if response.drag_started() && self.background_idle() {
            // A drag is only a drag once the pointer has moved, so the
            // corner is where the button went down, not where it is now.
            let origin = response
                .ctx
                .input(|i| i.pointer.press_origin())
                .or_else(|| response.interact_pointer_pos());
            if let Some(from) = origin {
                if let Some(pane) = pane {
                    if self.controller.state.browser.active != pane {
                        self.controller.activate_pane(pane);
                    }
                }
                let keep = response.ctx.input(|i| i.modifiers.command);
                if !keep {
                    self.controller.dispatch(AppAction::ClearSelection);
                }
                let base = if keep {
                    self.controller.state.checked.clone()
                } else {
                    vec![false; self.controller.state.checked.len()]
                };
                self.marquee = Some(Marquee {
                    from,
                    to: from,
                    pane,
                    base,
                });
            }
        }
        if response.dragged() {
            if let (Some(marquee), Some(to)) =
                (self.marquee.as_mut(), response.interact_pointer_pos())
            {
                marquee.to = to.clamp(clip.min, clip.max);
            }
        }
        if response.drag_stopped() {
            self.marquee = None;
        }
        if let Some(marquee) = &self.marquee {
            let p = palette_of(&response.ctx);
            let rect = marquee.rect();
            painter.rect_filled(rect, 2, alpha(p.focus, 0.18));
            painter.rect_stroke(rect, 2, Stroke::new(1.0, hex(p.focus)), StrokeKind::Inside);
            response.ctx.request_repaint();
        }
        response.context_menu(|ui| self.row_menu(ui, None, usize::MAX));
    }
}

pub(crate) fn column_slots(
    panes: usize,
    active: usize,
    wanted: f32,
    gap: f32,
    room: f32,
) -> (usize, f32) {
    let fit = (((room + gap) / (wanted + gap)).floor() as usize).max(1);
    if panes <= fit {
        return (0, wanted);
    }
    let first = (panes - fit).min(active);
    (
        first,
        ((room + gap) / fit as f32 - gap).max(wanted.min(room)),
    )
}
