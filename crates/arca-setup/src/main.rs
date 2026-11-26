#![forbid(unsafe_code)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod assets;
mod cli;
#[cfg_attr(not(windows), allow(dead_code))]
mod engine;
mod i18n;
mod screens;
mod theme;
mod ui;

fn main() {
    app::run();
}
