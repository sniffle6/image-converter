use std::{path::Path, process::Command};

use eframe::egui::{self, Align, Layout, RichText, Sense, Stroke};

use crate::{
    app::{ConvertalotApp, Screen},
    theme_catalog::{BuiltInTheme, ThemeId},
};

pub(crate) fn title_bar(app: &mut ConvertalotApp, root: &mut egui::Ui) {
    let context = root.ctx().clone();
    let tokens = app.tokens();
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
                ui.add(
                    egui::Image::new(&app.title_icon).fit_to_exact_size(egui::vec2(26.0, 26.0)),
                );
                ui.add_space(8.0);
                ui.label(
                    RichText::new("CONVERTALOT")
                        .strong()
                        .size(15.0)
                        .family(egui::FontFamily::Name("Arial Bold".into()))
                        .color(tokens.title_text.egui()),
                );
                ui.add(egui::Separator::default().vertical().spacing(5.0));
                let subtitle = match app.screen {
                    Screen::Appearance => "settings",
                    Screen::Preview => "image preview",
                    Screen::Workbench if app.is_running() => "converting…",
                    Screen::Workbench => "batch image converter",
                };
                ui.label(RichText::new(subtitle).monospace().size(11.0).color(
                    if app.is_running() {
                        tokens.accent.egui()
                    } else {
                        tokens.title_muted.egui()
                    },
                ));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if caption_button(ui, CaptionAction::Close, &tokens).clicked() {
                        context.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    let maximized =
                        context.input(|input| input.viewport().maximized.unwrap_or(false));
                    if caption_button(ui, CaptionAction::Maximize { maximized }, &tokens).clicked()
                    {
                        toggle_maximized(&context);
                    }
                    if caption_button(ui, CaptionAction::Minimize, &tokens).clicked() {
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
                                ThemeIcon::Glass,
                                "Glass",
                                app.preferences
                                    .themes
                                    .resolved_appearance()
                                    .material
                                    .is_glass(),
                                &tokens,
                            )
                            .clicked()
                            {
                                app.request_theme_selection(
                                    ThemeId::BuiltIn(BuiltInTheme::Glass),
                                    &context,
                                );
                            }
                            if theme_pill(
                                ui,
                                ThemeIcon::Dark,
                                "Dark",
                                app.preferences.themes.selected()
                                    == ThemeId::BuiltIn(BuiltInTheme::Dark),
                                &tokens,
                            )
                            .clicked()
                            {
                                app.request_theme_selection(
                                    ThemeId::BuiltIn(BuiltInTheme::Dark),
                                    &context,
                                );
                            }
                            if theme_pill(
                                ui,
                                ThemeIcon::Light,
                                "Light",
                                app.preferences.themes.selected()
                                    == ThemeId::BuiltIn(BuiltInTheme::Light),
                                &tokens,
                            )
                            .clicked()
                            {
                                app.request_theme_selection(
                                    ThemeId::BuiltIn(BuiltInTheme::Light),
                                    &context,
                                );
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

#[derive(Clone, Copy)]
enum CaptionAction {
    Minimize,
    Maximize { maximized: bool },
    Close,
}

fn caption_button(
    ui: &mut egui::Ui,
    action: CaptionAction,
    tokens: &crate::theme::ThemeTokens,
) -> egui::Response {
    let label = match action {
        CaptionAction::Minimize => "Minimize",
        CaptionAction::Maximize { maximized: true } => "Restore",
        CaptionAction::Maximize { maximized: false } => "Maximize",
        CaptionAction::Close => "Close",
    };
    let response = ui.add(
        egui::Button::new("")
            .fill(egui::Color32::TRANSPARENT)
            .stroke(Stroke::NONE)
            .corner_radius(5.0)
            .min_size(egui::vec2(34.0, 28.0)),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });

    let hovered = response.hovered() || response.has_focus();
    if hovered {
        let fill = if matches!(action, CaptionAction::Close) {
            tokens.danger.egui()
        } else {
            tokens.title_control.egui()
        };
        ui.painter().rect_filled(response.rect, 5.0, fill);
    }
    if response.has_focus() {
        ui.painter().rect_stroke(
            response.rect.shrink(1.0),
            4.0,
            Stroke::new(1.0, tokens.accent.egui()),
            egui::StrokeKind::Inside,
        );
    }

    let color = if hovered && matches!(action, CaptionAction::Close) {
        tokens.on_accent.egui()
    } else if hovered {
        tokens.title_text.egui()
    } else {
        tokens.title_muted.egui()
    };
    paint_caption_icon(ui.painter(), response.rect.center(), action, color);
    response.on_hover_text(label)
}

fn paint_caption_icon(
    painter: &egui::Painter,
    center: egui::Pos2,
    action: CaptionAction,
    color: egui::Color32,
) {
    let stroke = Stroke::new(1.25, color);
    match action {
        CaptionAction::Minimize => {
            painter.line_segment(
                [
                    center + egui::vec2(-5.0, 3.0),
                    center + egui::vec2(5.0, 3.0),
                ],
                stroke,
            );
        }
        CaptionAction::Maximize { maximized: false } => {
            painter.rect_stroke(
                egui::Rect::from_center_size(center, egui::vec2(9.0, 8.0)),
                0.5,
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        CaptionAction::Maximize { maximized: true } => {
            painter.rect_stroke(
                egui::Rect::from_center_size(center + egui::vec2(-1.7, -1.7), egui::vec2(7.5, 6.5)),
                0.5,
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.rect_stroke(
                egui::Rect::from_center_size(center + egui::vec2(1.7, 1.7), egui::vec2(7.5, 6.5)),
                0.5,
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        CaptionAction::Close => {
            painter.line_segment(
                [
                    center + egui::vec2(-4.0, -4.0),
                    center + egui::vec2(4.0, 4.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    center + egui::vec2(4.0, -4.0),
                    center + egui::vec2(-4.0, 4.0),
                ],
                stroke,
            );
        }
    }
}

fn theme_pill(
    ui: &mut egui::Ui,
    icon: ThemeIcon,
    label: &'static str,
    selected: bool,
    tokens: &crate::theme::ThemeTokens,
) -> egui::Response {
    let response = ui.add(
        egui::Button::new("")
            .selected(selected)
            .fill(if selected {
                tokens.accent.egui()
            } else {
                egui::Color32::TRANSPARENT
            })
            .stroke(Stroke::NONE)
            .corner_radius(20.0)
            .min_size(egui::vec2(28.0, 20.0)),
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), selected, label)
    });
    let color = if selected {
        tokens.on_accent.egui()
    } else if response.hovered() || response.has_focus() {
        tokens.accent.egui()
    } else {
        tokens.title_muted.egui()
    };
    let fill = if selected {
        tokens.accent.egui()
    } else {
        tokens.title_control.egui()
    };
    paint_theme_icon(ui.painter(), response.rect.center(), icon, color, fill);
    response.on_hover_text(label)
}

#[derive(Clone, Copy)]
enum ThemeIcon {
    Light,
    Dark,
    Glass,
}

fn paint_theme_icon(
    painter: &egui::Painter,
    center: egui::Pos2,
    icon: ThemeIcon,
    color: egui::Color32,
    fill: egui::Color32,
) {
    let stroke = Stroke::new(1.2, color);
    match icon {
        ThemeIcon::Light => {
            painter.circle_stroke(center, 2.8, stroke);
            for (start, end) in [
                ((0.0, -4.5), (0.0, -6.0)),
                ((0.0, 4.5), (0.0, 6.0)),
                ((-4.5, 0.0), (-6.0, 0.0)),
                ((4.5, 0.0), (6.0, 0.0)),
                ((-3.2, -3.2), (-4.3, -4.3)),
                ((3.2, 3.2), (4.3, 4.3)),
                ((3.2, -3.2), (4.3, -4.3)),
                ((-3.2, 3.2), (-4.3, 4.3)),
            ] {
                painter.line_segment(
                    [
                        center + egui::vec2(start.0, start.1),
                        center + egui::vec2(end.0, end.1),
                    ],
                    stroke,
                );
            }
        }
        ThemeIcon::Dark => {
            painter.circle_filled(center, 5.2, color);
            painter.circle_filled(center + egui::vec2(2.5, -1.7), 4.8, fill);
        }
        ThemeIcon::Glass => {
            let back =
                egui::Rect::from_center_size(center + egui::vec2(-1.8, -1.3), egui::vec2(8.0, 7.0));
            let front =
                egui::Rect::from_center_size(center + egui::vec2(1.8, 1.3), egui::vec2(8.0, 7.0));
            painter.rect_stroke(back, 1.5, stroke, egui::StrokeKind::Inside);
            painter.rect_filled(front, 1.5, fill);
            painter.rect_stroke(front, 1.5, stroke, egui::StrokeKind::Inside);
        }
    }
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
