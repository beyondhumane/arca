use crate::theme::{
    self, color, BLUE, BLUE_2, BLUE_PRESSED, INK, LINE, MUTED, PALE, PALE_HOVER, SLATE, SWITCH_OFF,
    TRACK, WHITE,
};
use gpui::{
    div, linear_color_stop, linear_gradient, percentage, prelude::*, px, relative, svg, Animation,
    AnimationExt as _, BoxShadow, Div, ElementId, FontWeight, Hsla, Stateful, Svg, Transformation,
    WindowControlArea,
};
use std::time::Duration;

const BLUE_HOVER: u32 = 0x1A78FF;

pub fn icon(path: &'static str, size: f32, hex: u32) -> Svg {
    svg()
        .path(path)
        .size(px(size))
        .flex_none()
        .text_color(color(hex))
}

fn glow(hex: u32, alpha: f32) -> Hsla {
    let mut tint: Hsla = color(hex).into();
    tint.a = alpha;
    tint
}

pub fn primary_button(
    id: &'static str,
    label: &'static str,
    width: Option<f32>,
    arrow: bool,
) -> Stateful<Div> {
    let button = div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .gap(px(10.))
        .h(px(46.))
        .px(px(24.))
        .rounded(px(12.))
        .bg(linear_gradient(
            180.,
            linear_color_stop(color(BLUE_HOVER), 0.),
            linear_color_stop(color(BLUE), 1.),
        ))
        .shadow(vec![
            BoxShadow::new(px(0.), px(8.), glow(BLUE, 0.30)).blur_radius(px(20.)),
            BoxShadow::new(px(0.), px(1.), glow(BLUE, 0.35)).blur_radius(px(2.)),
        ])
        .text_color(color(WHITE))
        .font_family(theme::display_font())
        .font_weight(FontWeight::SEMIBOLD)
        .text_size(px(15.))
        .cursor_pointer()
        .hover(|style| style.bg(color(BLUE_HOVER)))
        .active(|style| style.bg(color(BLUE_PRESSED)))
        .child(label)
        .when(arrow, |button| {
            button.child(icon("icons/arrow-right.svg", 17., WHITE))
        });
    match width {
        Some(width) => button.w(px(width)),
        None => button,
    }
}

pub fn secondary_button(
    id: &'static str,
    label: &'static str,
    leading: Option<&'static str>,
) -> Stateful<Div> {
    let mut button = div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .gap(px(8.))
        .h(px(46.))
        .px(px(24.))
        .rounded(px(12.))
        .bg(color(PALE))
        .border_1()
        .border_color(color(LINE))
        .text_color(color(BLUE))
        .font_family(theme::display_font())
        .font_weight(FontWeight::SEMIBOLD)
        .text_size(px(15.))
        .cursor_pointer()
        .hover(|style| style.bg(color(PALE_HOVER)))
        .active(|style| style.bg(color(LINE)));
    if let Some(path) = leading {
        button = button.child(icon(path, 16., BLUE));
    }
    button.child(label)
}

pub fn link(id: &'static str, label: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .text_color(color(BLUE))
        .text_size(px(13.5))
        .underline()
        .cursor_pointer()
        .hover(|style| style.text_color(color(BLUE_PRESSED)))
        .child(label)
}

pub fn switch(id: &'static str, on: bool) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .w(px(42.))
        .h(px(24.))
        .px(px(3.))
        .rounded_full()
        .cursor_pointer()
        .bg(color(if on { BLUE } else { SWITCH_OFF }))
        .when(on, |track| track.justify_end())
        .child(
            div()
                .size(px(18.))
                .rounded_full()
                .bg(color(WHITE))
                .shadow(vec![
                    BoxShadow::new(px(0.), px(1.), glow(INK, 0.28)).blur_radius(px(3.))
                ]),
        )
}

pub fn card() -> Div {
    div()
        .rounded(px(16.))
        .bg(glow(WHITE, 0.82))
        .border_1()
        .border_color(glow(LINE, 0.9))
        .shadow(vec![
            BoxShadow::new(px(0.), px(10.), glow(INK, 0.06)).blur_radius(px(28.))
        ])
}

pub fn progress_bar(percent: f32) -> Div {
    div()
        .flex_1()
        .h(px(12.))
        .rounded_full()
        .bg(color(TRACK))
        .overflow_hidden()
        .child(
            div()
                .h_full()
                .w(relative((percent / 100.).clamp(0., 1.)))
                .rounded_full()
                .bg(linear_gradient(
                    90.,
                    linear_color_stop(color(BLUE), 0.),
                    linear_color_stop(color(BLUE_2), 1.),
                )),
        )
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StepState {
    Done,
    Active,
    Pending,
}

fn step_mark(state: StepState) -> Div {
    let slot = div()
        .relative()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(20.));
    match state {
        StepState::Done => slot.rounded_full().bg(color(BLUE)).child(
            svg()
                .path("icons/check.svg")
                .size(px(12.))
                .text_color(color(WHITE)),
        ),
        StepState::Active => slot
            .child(
                div()
                    .absolute()
                    .size_full()
                    .rounded_full()
                    .border_2()
                    .border_color(color(TRACK)),
            )
            .child(icon("icons/loader.svg", 20., BLUE).with_animation(
                "spinner",
                Animation::new(Duration::from_millis(900)).repeat(),
                |mark, delta| mark.with_transformation(Transformation::rotate(percentage(delta))),
            )),
        StepState::Pending => slot.rounded_full().border_2().border_color(color(MUTED)),
    }
}

pub fn step_row(state: StepState, label: &'static str) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(12.))
        .child(step_mark(state))
        .child(
            div()
                .text_size(px(14.))
                .text_color(color(SLATE))
                .child(label),
        )
}

pub fn window_control(
    id: &'static str,
    glyph: &'static str,
    area: WindowControlArea,
    closes: bool,
) -> Stateful<Div> {
    let hover_bg = if closes {
        color(0xE81123)
    } else {
        glow(INK, 0.07).into()
    };
    let mark = svg().path(glyph).size(px(15.)).text_color(color(INK));
    let mark = if closes {
        mark.group_hover(id, |style| style.text_color(color(WHITE)))
    } else {
        mark
    };
    let control = div()
        .id(id)
        .group(id)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .w(px(46.))
        .h_full()
        .hover(move |style| style.bg(hover_bg))
        .child(mark);
    if cfg!(target_os = "windows") {
        control.window_control_area(area)
    } else {
        control.on_click(move |_, window, _| match area {
            WindowControlArea::Min => window.minimize_window(),
            WindowControlArea::Close => window.remove_window(),
            _ => {}
        })
    }
}

pub fn drag_area(id: impl Into<ElementId>) -> Stateful<Div> {
    let area = div().id(id).flex_1().h_full();
    if cfg!(target_os = "windows") {
        area.window_control_area(WindowControlArea::Drag)
    } else {
        area.on_mouse_move(|event, window, _| {
            if event.dragging() {
                window.start_window_move();
            }
        })
    }
}
