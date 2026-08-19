use std::{path::Path, process::Command};

use eframe::egui::{self, Align, Layout, RichText, Sense, Stroke};

use crate::{
    app::{ConvertalotApp, Screen},
    theme::ThemeMode,
};

pub(crate) fn title_bar(app: &mut ConvertalotApp, root: &mut egui::Ui) {
    let context = root.ctx().clone();
    let tokens = app.preferences.tokens();
    let title_fill = app.backdrop_fill(tokens.title);
    egui::Panel::top("custom-title-bar")
        .exact_size(52.0)
        .frame(
            egui::Frame::new()
                .fill(title_fill)
                .inner_margin(egui::Margin::symmetric(18, 12)),
        )
        .show(root, |ui| {
            let mut drag_rect = ui.max_rect();
            drag_rect.max.x = (drag_rect.max.x - 270.0).max(drag_rect.min.x);
            let drag = ui.interact(
                drag_rect,
                ui.id().with("drag-window"),
                Sense::click_and_drag(),
            );
            if drag.double_clicked() {
                toggle_maximized(&context);
            } else if drag.drag_started() {
                context.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("CONVERTALOT")
                        .strong()
                        .size(15.0)
                        .family(egui::FontFamily::Name("Arial Bold".into()))
                        .color(tokens.title_text.egui()),
                );
                ui.add(egui::Separator::default().vertical().spacing(5.0));
                let subtitle = if app.screen == Screen::Appearance {
                    "settings"
                } else if app.is_running() {
                    "converting…"
                } else {
                    "batch image converter"
                };
                ui.label(RichText::new(subtitle).monospace().size(11.0).color(
                    if app.is_running() {
                        tokens.accent.egui()
                    } else {
                        tokens.title_muted.egui()
                    },
                ));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if title_button(ui, "×", &tokens).clicked() {
                        context.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    if title_button(ui, "▢", &tokens).clicked() {
                        toggle_maximized(&context);
                    }
                    if title_button(ui, "–", &tokens).clicked() {
                        context.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                    }
                    ui.add_space(5.0);
                    if app.is_running() {
                        // Conversion settings are intentionally unreachable until the batch ends.
                        ui.add_space(31.0);
                    } else if title_button(ui, "⚙", &tokens)
                        .on_hover_text("Appearance settings")
                        .clicked()
                    {
                        app.screen = if app.screen == Screen::Appearance {
                            Screen::Workbench
                        } else {
                            Screen::Appearance
                        };
                    }
                    egui::Frame::new()
                        .fill(tokens.title_control.egui())
                        .corner_radius(20.0)
                        .inner_margin(2.0)
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            if theme_pill(
                                ui,
                                "●",
                                app.preferences.active_theme == ThemeMode::Glass,
                                &tokens,
                            )
                            .clicked()
                            {
                                app.select_theme(ThemeMode::Glass, &context);
                            }
                            if theme_pill(
                                ui,
                                "◐",
                                app.preferences.active_theme == ThemeMode::Dark,
                                &tokens,
                            )
                            .clicked()
                            {
                                app.select_theme(ThemeMode::Dark, &context);
                            }
                            if theme_pill(
                                ui,
                                "☀",
                                app.preferences.active_theme == ThemeMode::Light,
                                &tokens,
                            )
                            .clicked()
                            {
                                app.select_theme(ThemeMode::Light, &context);
                            }
                        });
                });
            });
        });
}

fn title_button(
    ui: &mut egui::Ui,
    text: &str,
    tokens: &crate::theme::ThemeTokens,
) -> egui::Response {
    ui.add(
        egui::Button::new(
            RichText::new(text)
                .size(12.0)
                .color(tokens.title_muted.egui()),
        )
        .frame(false)
        .min_size(egui::vec2(26.0, 24.0)),
    )
}

fn theme_pill(
    ui: &mut egui::Ui,
    text: &str,
    selected: bool,
    tokens: &crate::theme::ThemeTokens,
) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).strong().size(11.0).color(if selected {
            tokens.on_accent.egui()
        } else {
            tokens.title_muted.egui()
        }))
        .fill(if selected {
            tokens.accent.egui()
        } else {
            egui::Color32::TRANSPARENT
        })
        .stroke(Stroke::NONE)
        .corner_radius(20.0)
        .min_size(egui::vec2(28.0, 20.0)),
    )
}

fn toggle_maximized(context: &egui::Context) {
    let maximized = context.input(|input| input.viewport().maximized.unwrap_or(false));
    context.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
}

pub(crate) fn resize_handles(context: &egui::Context) {
    if context.input(|input| input.viewport().maximized.unwrap_or(false)) {
        return;
    }
    let rect = context.input(|input| input.content_rect());
    let thickness = 5.0;
    let handles = [
        (
            egui::Rect::from_min_max(rect.min, egui::pos2(rect.max.x, rect.min.y + thickness)),
            egui::ResizeDirection::North,
        ),
        (
            egui::Rect::from_min_max(egui::pos2(rect.min.x, rect.max.y - thickness), rect.max),
            egui::ResizeDirection::South,
        ),
        (
            egui::Rect::from_min_max(rect.min, egui::pos2(rect.min.x + thickness, rect.max.y)),
            egui::ResizeDirection::West,
        ),
        (
            egui::Rect::from_min_max(egui::pos2(rect.max.x - thickness, rect.min.y), rect.max),
            egui::ResizeDirection::East,
        ),
    ];
    egui::Area::new("resize-handles".into())
        .order(egui::Order::Foreground)
        .fixed_pos(rect.min)
        .show(context, |ui| {
            for (index, (handle, direction)) in handles.into_iter().enumerate() {
                let local = handle.translate(-rect.min.to_vec2());
                let response = ui.interact(local, ui.id().with(index), Sense::drag());
                if response.drag_started() {
                    context.send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
                }
            }
        });
}

pub(crate) fn open_folder(path: &Path) {
    let _ = Command::new("explorer.exe").arg(path).spawn();
}
