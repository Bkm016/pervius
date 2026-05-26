//! File 子菜单
//!
//! @author sky

use super::MenuState;
use crate::app::App;
use crate::appearance::theme;
use eframe::egui;
use egui_shell::components::{
    menu_item, menu_item_if, menu_item_raw, menu_item_raw_if, menu_submenu, SettingsFile,
};
use rust_i18n::t;
use std::path::PathBuf;

pub fn render(ui: &mut egui::Ui, app: &mut App) {
    let mt = &theme::menu_theme();
    let state = MenuState::from_app(app);
    if menu_item(
        ui,
        mt,
        &t!("menu.open_jar"),
        Some(&app.settings.keymap.open_jar),
    ) {
        app.request_open_jar_dialog();
    }
    // Open Recent submenu
    let recent = app.settings.recent.clone();
    let mut open_path: Option<PathBuf> = None;
    let mut clear = false;
    menu_submenu(ui, mt, &t!("menu.open_recent"), |ui| {
        if recent.is_empty() {
            ui.add_enabled(
                false,
                egui::Label::new(
                    egui::RichText::new(t!("menu.no_recent"))
                        .size(12.0)
                        .color(egui::Color32::from_rgb(100, 100, 100)),
                ),
            );
        } else {
            let mt = &theme::menu_theme();
            for entry in &recent {
                let path = std::path::Path::new(&entry.path);
                let exists = path.exists();
                let dir = path
                    .parent()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if menu_item_raw_if(ui, mt, &entry.name, &dir, exists) {
                    open_path = Some(PathBuf::from(&entry.path));
                }
            }
            ui.separator();
            if menu_item_raw(ui, mt, &t!("menu.clear_recent"), "") {
                clear = true;
            }
        }
    });
    if let Some(path) = open_path {
        app.request_open_jar(&path);
    }
    if clear {
        app.settings.clear_recent();
        let _ = app.settings.save();
    }
    ui.separator();
    if menu_item_if(
        ui,
        mt,
        &t!("menu.export_jar"),
        Some(&app.settings.keymap.export_jar),
        state.has_jar && state.is_idle,
    ) {
        app.export_jar();
    }
    if menu_item_if(
        ui,
        mt,
        &t!("menu.save_overwrite_source"),
        None,
        state.has_jar && state.is_idle && state.has_jar_changes,
    ) {
        app.save_jar_overwrite_source();
    }
    if menu_item_if(
        ui,
        mt,
        &t!("menu.export_decompiled"),
        Some(&app.settings.keymap.export_decompiled),
        state.has_jar && state.is_decompiled && state.is_idle,
    ) {
        app.export_decompiled();
    }
    if menu_item_if(
        ui,
        mt,
        &t!("menu.re_decompile"),
        None,
        state.has_jar && state.is_idle,
    ) {
        app.redecompile_jar();
    }
    ui.separator();
    if menu_item(
        ui,
        mt,
        &t!("menu.settings"),
        Some(&app.settings.keymap.open_settings),
    ) {
        app.open_settings();
    }
    ui.separator();
    if menu_item_raw(ui, mt, &t!("menu.exit"), "Alt+F4") {
        app.request_close(ui.ctx());
    }
}
