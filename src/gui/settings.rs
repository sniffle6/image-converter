use eframe::egui::{self, Color32, RichText, Stroke};
use image_converter::{DuplicateStyle, OutputFormat, RgbColor};
use rfd::FileDialog;

use crate::{
    app::{ConvertalotApp, ResizeChoice},
    theme::{HexColor, ThemeTokens},
    theme_catalog::{DirtyDecision, ThemeId, WindowMaterial},
    windows,
};

pub(crate) fn conversion_sidebar(
    app: &mut ConvertalotApp,
    root: &mut egui::Ui,
    context: &egui::Context,
) {
    let tokens = app.tokens();
    let panel_fill = app.backdrop_fill(tokens.panel);
    egui::Panel::right("conversion-settings")
        .resizable(false)
        .exact_size(268.0)
        .frame(
            egui::Frame::new()
                .fill(panel_fill)
                .stroke(Stroke::new(1.0, tokens.border.egui()))
                .inner_margin(egui::Margin::symmetric(18, 16)),
        )
        .show(root, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);
            if app.is_running() {
                running_conversion_sidebar(app, ui, &tokens);
                return;
            }
            {
                let mut changed = false;
                section(ui, "Convert to");
                changed |= format_selector(ui, &mut app.format, &tokens);
                match app.format {
                    OutputFormat::Jpeg => {
                        muted(ui, "Small files for photos.", &tokens);
                        ui.horizontal(|ui| {
                            ui.add_sized(
                                [40.0, 20.0],
                                egui::Label::new(
                                    RichText::new("Quality")
                                        .size(11.0)
                                        .color(tokens.muted.egui()),
                                ),
                            );
                            changed |= quality_slider(ui, &mut app.quality, &tokens);
                            egui::Frame::new()
                                .fill(tokens.field.egui())
                                .stroke(Stroke::new(1.0, tokens.border.egui()))
                                .corner_radius(4.0)
                                .inner_margin(egui::Margin::symmetric(6, 3))
                                .show(ui, |ui| {
                                    ui.label(
                                        RichText::new(app.quality.to_string())
                                            .monospace()
                                            .size(10.5),
                                    );
                                });
                        });
                        muted(
                            ui,
                            "JPEG doesn't support transparency. Transparent areas use this color.",
                            &tokens,
                        );
                        changed |= jpeg_background(ui, app, &tokens);
                        if let Ok(background) = app.jpeg_hex.parse::<RgbColor>() {
                            app.preferences.jpeg_background = HexColor::from(background);
                        }
                    }
                    OutputFormat::Png => {
                        muted(ui, "Lossless with transparency.", &tokens);
                    }
                    OutputFormat::WebP => {
                        muted(ui, "Lossless output with transparency.", &tokens);
                    }
                };

                ui.add_space(5.0);
                section(ui, "Size");
                changed |= egui::ComboBox::from_id_salt("resize-mode")
                    .width(ui.available_width())
                    .selected_text(match app.resize_choice {
                        ResizeChoice::Original => "Keep original",
                        ResizeChoice::Fit => "Fit inside",
                        ResizeChoice::Exact => "Exact dimensions",
                        ResizeChoice::Percent => "Percentage",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut app.resize_choice,
                            ResizeChoice::Original,
                            "Keep original",
                        );
                        ui.selectable_value(
                            &mut app.resize_choice,
                            ResizeChoice::Fit,
                            "Fit inside",
                        );
                        ui.selectable_value(
                            &mut app.resize_choice,
                            ResizeChoice::Exact,
                            "Exact dimensions",
                        );
                        ui.selectable_value(
                            &mut app.resize_choice,
                            ResizeChoice::Percent,
                            "Percentage",
                        );
                    })
                    .response
                    .changed();
                match app.resize_choice {
                    ResizeChoice::Fit | ResizeChoice::Exact => {
                        ui.horizontal(|ui| {
                            changed |= ui
                                .add_sized(
                                    [101.0, 29.0],
                                    egui::DragValue::new(&mut app.width)
                                        .range(1..=100_000)
                                        .suffix(" w"),
                                )
                                .changed();
                            ui.label(RichText::new("×").size(11.0).color(tokens.muted.egui()));
                            changed |= ui
                                .add_sized(
                                    [101.0, 29.0],
                                    egui::DragValue::new(&mut app.height)
                                        .range(1..=100_000)
                                        .suffix(" h"),
                                )
                                .changed();
                        });
                    }
                    ResizeChoice::Percent => {
                        changed |= ui
                            .add(egui::Slider::new(&mut app.percent, 1..=1000).suffix("%"))
                            .changed();
                    }
                    ResizeChoice::Original => {}
                }
                if matches!(app.resize_choice, ResizeChoice::Fit) {
                    muted(ui, "Aspect ratio kept — nothing stretches.", &tokens);
                }

                ui.add_space(5.0);
                section(ui, "Save to");
                changed |= editable_destination(ui, app);
                muted(
                    ui,
                    "Leave blank for a converted folder beside each source.",
                    &tokens,
                );
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(3.0);
                changed |=
                    setting_toggle(ui, &mut app.overwrite, "Overwrite existing files", &tokens);
                muted(ui, "Otherwise save duplicates as", &tokens);
                egui::Grid::new("duplicate-style")
                    .num_columns(2)
                    .spacing([6.0, 5.0])
                    .show(ui, |ui| {
                        changed |= duplicate(
                            ui,
                            &mut app.duplicate_style,
                            DuplicateStyle::Dash,
                            "photo-2.jpg",
                        );
                        changed |= duplicate(
                            ui,
                            &mut app.duplicate_style,
                            DuplicateStyle::Underscore,
                            "photo_2.jpg",
                        );
                        ui.end_row();
                        changed |= duplicate(
                            ui,
                            &mut app.duplicate_style,
                            DuplicateStyle::Parenthesized,
                            "photo (2).jpg",
                        );
                        changed |= duplicate(
                            ui,
                            &mut app.duplicate_style,
                            DuplicateStyle::Copy,
                            "photo-copy.jpg",
                        );
                        ui.end_row();
                    });
                if changed {
                    let _ = app.preferences.save();
                    app.settings_changed(context);
                }
            }
        });
}

fn running_conversion_sidebar(app: &mut ConvertalotApp, ui: &mut egui::Ui, tokens: &ThemeTokens) {
    ui.add_enabled_ui(false, |ui| {
        section(ui, "Convert to");
        let _ = format_selector(ui, &mut app.format, tokens);
        if app.format == OutputFormat::Jpeg {
            ui.horizontal(|ui| {
                ui.add_sized(
                    [40.0, 20.0],
                    egui::Label::new(
                        RichText::new("Quality")
                            .size(11.0)
                            .color(tokens.muted.egui()),
                    ),
                );
                let _ = quality_slider(ui, &mut app.quality, tokens);
                egui::Frame::new()
                    .fill(tokens.field.egui())
                    .stroke(Stroke::new(1.0, tokens.border.egui()))
                    .corner_radius(4.0)
                    .inner_margin(egui::Margin::symmetric(6, 3))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(app.quality.to_string())
                                .monospace()
                                .size(10.5),
                        );
                    });
            });
        }

        ui.add_space(5.0);
        section(ui, "Size");
        egui::ComboBox::from_id_salt("running-resize-mode")
            .width(ui.available_width())
            .selected_text(match app.resize_choice {
                ResizeChoice::Original => "Keep original",
                ResizeChoice::Fit => "Fit inside",
                ResizeChoice::Exact => "Exact dimensions",
                ResizeChoice::Percent => "Percentage",
            })
            .show_ui(ui, |_| {});
        match app.resize_choice {
            ResizeChoice::Fit | ResizeChoice::Exact => {
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [101.0, 29.0],
                        egui::DragValue::new(&mut app.width).suffix(" w"),
                    );
                    ui.label(RichText::new("×").size(11.0).color(tokens.muted.egui()));
                    ui.add_sized(
                        [101.0, 29.0],
                        egui::DragValue::new(&mut app.height).suffix(" h"),
                    );
                });
            }
            ResizeChoice::Percent => {
                ui.add(egui::Slider::new(&mut app.percent, 1..=1000).suffix("%"));
            }
            ResizeChoice::Original => {}
        }
    });

    ui.add_space(5.0);
    ui.add_enabled_ui(false, |ui| section(ui, "Save to"));
    destination_field(ui, app, tokens, false);
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(2.0);
    muted(
        ui,
        "Settings unlock when the batch finishes. OPEN still works while it runs.",
        tokens,
    );
}

fn format_selector(ui: &mut egui::Ui, format: &mut OutputFormat, tokens: &ThemeTokens) -> bool {
    let mut changed = false;
    let width = ui.available_width();
    egui::Frame::new()
        .fill(tokens.control.egui())
        .stroke(Stroke::new(1.0, tokens.border.egui()))
        .corner_radius(7.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let segment = width / 3.0;
                changed |= ui
                    .add_sized(
                        [segment, 34.0],
                        egui::Button::selectable(
                            *format == OutputFormat::Png,
                            format_label("PNG", *format == OutputFormat::Png),
                        ),
                    )
                    .clicked()
                    .then(|| *format = OutputFormat::Png)
                    .is_some();
                changed |= ui
                    .add_sized(
                        [segment, 34.0],
                        egui::Button::selectable(
                            *format == OutputFormat::Jpeg,
                            format_label("JPEG", *format == OutputFormat::Jpeg),
                        ),
                    )
                    .clicked()
                    .then(|| *format = OutputFormat::Jpeg)
                    .is_some();
                changed |= ui
                    .add_sized(
                        [segment, 34.0],
                        egui::Button::selectable(
                            *format == OutputFormat::WebP,
                            format_label("WebP", *format == OutputFormat::WebP),
                        ),
                    )
                    .clicked()
                    .then(|| *format = OutputFormat::WebP)
                    .is_some();
            });
        });
    changed
}

fn format_label(text: &str, selected: bool) -> RichText {
    let label = RichText::new(text);
    if selected {
        label
            .strong()
            .family(egui::FontFamily::Name("Arial Bold".into()))
    } else {
        label
    }
}

fn quality_slider(ui: &mut egui::Ui, quality: &mut u8, tokens: &ThemeTokens) -> bool {
    let (rect, mut response) =
        ui.allocate_exact_size(egui::vec2(134.0, 20.0), egui::Sense::click_and_drag());
    let mut changed = false;
    if (response.clicked() || response.dragged())
        && let Some(pointer) = response.interact_pointer_pos()
    {
        let fraction = ((pointer.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
        let next = (1.0 + fraction * 99.0).round() as u8;
        if next != *quality {
            *quality = next;
            changed = true;
        }
    }
    if response.has_focus() {
        let adjustment = ui.input(|input| {
            i8::from(input.key_pressed(egui::Key::ArrowRight))
                - i8::from(input.key_pressed(egui::Key::ArrowLeft))
        });
        if adjustment != 0 {
            let next = quality.saturating_add_signed(adjustment).clamp(1, 100);
            if next != *quality {
                *quality = next;
                changed = true;
            }
        }
    }
    if changed {
        response.mark_changed();
    }
    let track = egui::Rect::from_center_size(rect.center(), egui::vec2(rect.width(), 4.0));
    ui.painter().rect_filled(track, 2.0, tokens.border.egui());
    let fraction = (f32::from(*quality) - 1.0) / 99.0;
    let knob_x = egui::lerp(track.left()..=track.right(), fraction);
    ui.painter().rect_filled(
        egui::Rect::from_min_max(track.min, egui::pos2(knob_x, track.bottom())),
        2.0,
        tokens.accent.egui(),
    );
    ui.painter().circle_filled(
        egui::pos2(knob_x, track.center().y),
        7.0,
        tokens.accent.egui(),
    );
    if response.hovered() || response.has_focus() {
        ui.painter().circle_stroke(
            egui::pos2(knob_x, track.center().y),
            8.0,
            Stroke::new(1.0, tokens.text.egui()),
        );
    }
    response.on_hover_text(format!("JPEG quality: {quality}"));
    changed
}

fn jpeg_background(ui: &mut egui::Ui, app: &mut ConvertalotApp, tokens: &ThemeTokens) -> bool {
    const CONTROL_HEIGHT: f32 = 28.0;

    let mut changed = false;
    ui.horizontal(|ui| {
        if ui
            .add_sized(
                [49.0, CONTROL_HEIGHT],
                egui::Button::selectable(app.jpeg_hex.eq_ignore_ascii_case("#FFFFFF"), "White")
                    .frame_when_inactive(true),
            )
            .clicked()
        {
            app.jpeg_hex = "#FFFFFF".into();
            changed = true;
        }
        if ui
            .add_sized(
                [46.0, CONTROL_HEIGHT],
                egui::Button::selectable(app.jpeg_hex.eq_ignore_ascii_case("#000000"), "Black")
                    .frame_when_inactive(true),
            )
            .clicked()
        {
            app.jpeg_hex = "#000000".into();
            changed = true;
        }
        let custom = !app.jpeg_hex.eq_ignore_ascii_case("#FFFFFF")
            && !app.jpeg_hex.eq_ignore_ascii_case("#000000");
        if ui
            .add_sized(
                [58.0, CONTROL_HEIGHT],
                egui::Button::selectable(custom, "Custom").frame_when_inactive(true),
            )
            .clicked()
            && !custom
        {
            app.jpeg_hex = "#808080".into();
            changed = true;
        }
        let parsed = app.jpeg_hex.parse::<RgbColor>();
        let rgb = parsed.unwrap_or(RgbColor::WHITE).components();
        let mut picked = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
        let color_response = ui
            .scope(|ui| {
                ui.spacing_mut().interact_size.y = CONTROL_HEIGHT;
                ui.color_edit_button_srgba(&mut picked)
            })
            .inner;
        if color_response.changed() {
            let [red, green, blue, _] = picked.to_srgba_unmultiplied();
            app.jpeg_hex = format!("#{red:02X}{green:02X}{blue:02X}");
            changed = true;
        }
    });
    let response = ui.add_sized(
        [ui.available_width(), 24.0],
        centered_monospace_text_edit(&mut app.jpeg_hex)
            .font(egui::TextStyle::Monospace)
            .hint_text("#RRGGBB"),
    );
    if response.changed() && app.jpeg_hex.parse::<RgbColor>().is_ok() {
        changed = true;
    }
    if app.jpeg_hex.parse::<RgbColor>().is_err() {
        ui.label(
            RichText::new("Use #RRGGBB")
                .size(10.0)
                .color(tokens.danger.egui()),
        );
    }
    changed
}

fn destination_field(
    ui: &mut egui::Ui,
    app: &ConvertalotApp,
    tokens: &ThemeTokens,
    content_enabled: bool,
) {
    let destination = app
        .output_dir
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "Beside each source".to_owned());
    egui::Frame::new()
        .fill(tokens.field.egui())
        .stroke(Stroke::new(1.0, tokens.border.egui()))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let open_width = if app.concrete_output_dir().is_some() {
                    38.0
                } else {
                    0.0
                };
                ui.add_sized(
                    [(ui.available_width() - open_width - 6.0).max(50.0), 17.0],
                    egui::Label::new(
                        RichText::new(&destination)
                            .monospace()
                            .size(10.5)
                            .color(if content_enabled {
                                tokens.text.egui()
                            } else {
                                tokens.muted.egui()
                            }),
                    )
                        .halign(egui::Align::LEFT)
                        .truncate(),
                )
                .on_hover_text(format!(
                    "{destination}\nOutputs use a converted folder beside each source unless one folder is chosen."
                ));
                if let Some(path) = app.concrete_output_dir()
                    && ui
                        .link(
                            RichText::new("OPEN")
                                .strong()
                                .size(10.0)
                                .color(tokens.accent.egui()),
                        )
                        .clicked()
                {
                    windows::open_folder(&path);
                }
            });
        });
}

fn editable_destination(ui: &mut egui::Ui, app: &mut ConvertalotApp) -> bool {
    let mut committed = false;
    ui.horizontal(|ui| {
        let browse_width = 68.0;
        let field_width =
            (ui.available_width() - browse_width - ui.spacing().item_spacing.x).max(100.0);
        let response = ui.add_sized(
            [field_width, 29.0],
            centered_tall_monospace_text_edit(&mut app.output_dir_text)
                .font(egui::TextStyle::Monospace)
                .hint_text("Output folder path"),
        );
        if response.changed() {
            app.output_dir = if app.output_dir_text.trim().is_empty() {
                None
            } else {
                Some(app.output_dir_text.clone().into())
            };
        }
        committed |= response.lost_focus()
            || (response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)));

        if ui
            .add_sized([browse_width, 29.0], egui::Button::new("Browse…"))
            .clicked()
            && let Some(folder) = FileDialog::new()
                .set_title("Choose output folder")
                .pick_folder()
        {
            app.output_dir_text = folder.display().to_string();
            app.output_dir = Some(folder);
            committed = true;
        }
    });
    committed
}

fn setting_toggle(ui: &mut egui::Ui, value: &mut bool, label: &str, tokens: &ThemeTokens) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        let (rect, response) = ui.allocate_exact_size(egui::vec2(34.0, 18.0), egui::Sense::click());
        if response.clicked()
            || (response.has_focus()
                && ui.input(|input| {
                    input.key_pressed(egui::Key::Space) || input.key_pressed(egui::Key::Enter)
                }))
        {
            *value = !*value;
            changed = true;
        }
        let track = if *value {
            tokens.accent.egui()
        } else {
            tokens.control.egui()
        };
        ui.painter().rect_filled(rect, 9.0, track);
        ui.painter().rect_stroke(
            rect,
            9.0,
            Stroke::new(
                if response.has_focus() { 2.0 } else { 1.0 },
                if response.has_focus() {
                    tokens.accent.egui()
                } else {
                    tokens.border.egui()
                },
            ),
            egui::StrokeKind::Inside,
        );
        let knob_x = if *value {
            rect.right() - 9.0
        } else {
            rect.left() + 9.0
        };
        ui.painter().circle_filled(
            egui::pos2(knob_x, rect.center().y),
            6.0,
            if *value {
                tokens.on_accent.egui()
            } else {
                tokens.muted.egui()
            },
        );
        if ui
            .add(egui::Label::new(
                RichText::new(label)
                    .strong()
                    .size(11.5)
                    .color(tokens.text.egui()),
            ))
            .clicked()
        {
            *value = !*value;
            changed = true;
        }
    });
    changed
}

fn muted(ui: &mut egui::Ui, text: &str, tokens: &ThemeTokens) {
    ui.label(RichText::new(text).size(10.5).color(tokens.muted.egui()));
}

fn duplicate(
    ui: &mut egui::Ui,
    current: &mut DuplicateStyle,
    value: DuplicateStyle,
    label: &str,
) -> bool {
    let selected = *current == value;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(105.0, 26.0), egui::Sense::click());
    let activated = response.clicked()
        || (response.has_focus()
            && ui.input(|input| {
                input.key_pressed(egui::Key::Enter) || input.key_pressed(egui::Key::Space)
            }));
    let interactive = ui.style().interact(&response);
    let stroke = if selected || response.has_focus() {
        Stroke::new(2.0, ui.visuals().selection.bg_fill)
    } else {
        interactive.bg_stroke
    };
    ui.painter().rect(
        rect,
        6.0,
        ui.visuals().widgets.inactive.bg_fill,
        stroke,
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center() + egui::vec2(0.0, 1.0),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::monospace(10.5),
        interactive.text_color(),
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), selected, label)
    });
    if activated {
        *current = value;
    }
    activated
}

fn section(ui: &mut egui::Ui, text: &str) {
    let color = ui
        .visuals()
        .override_text_color
        .unwrap_or(ui.visuals().text_color());
    ui.label(
        RichText::new(text)
            .strong()
            .size(13.5)
            .family(egui::FontFamily::Name("Arial Bold".into()))
            .color(color),
    );
}

pub(crate) fn appearance(app: &mut ConvertalotApp, root: &mut egui::Ui, context: &egui::Context) {
    let tokens = app.tokens();
    let background_fill = app.backdrop_fill(tokens.background);
    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(background_fill)
                .inner_margin(egui::Margin::symmetric(22, 14)),
        )
        .show(root, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_min_width((ui.available_width() - 8.0).max(640.0));
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(
                                RichText::new("APPEARANCE")
                                    .monospace()
                                    .size(10.0)
                                    .color(tokens.accent.egui()),
                            );
                            ui.label(
                                RichText::new("Theme library")
                                    .strong()
                                    .size(19.0)
                                    .color(tokens.text.egui()),
                            );
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Back to converter").clicked() {
                                app.screen = crate::app::Screen::Workbench;
                            }
                        });
                    });
                    ui.add_space(8.0);
                    theme_selector(app, ui, context, &tokens);
                    ui.add_space(8.0);
                    theme_editor(app, ui, context, &tokens);
                });
        });
}

fn theme_selector(
    app: &mut ConvertalotApp,
    ui: &mut egui::Ui,
    context: &egui::Context,
    tokens: &ThemeTokens,
) {
    egui::Frame::new()
        .fill(tokens.panel.egui())
        .stroke(Stroke::new(1.0, tokens.border.egui()))
        .corner_radius(9.0)
        .inner_margin(egui::Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Theme")
                        .strong()
                        .size(12.0)
                        .color(tokens.text.egui()),
                );
                let selected = app.preferences.themes.selected_label();
                let choices = app.preferences.themes.choices();
                egui::ComboBox::from_id_salt("theme-library-selector")
                    .width((ui.available_width() - 145.0).clamp(250.0, 430.0))
                    .height(280.0)
                    .selected_text(selected)
                    .show_ui(ui, |ui| {
                        ui.label(
                            RichText::new("BUILT-IN")
                                .monospace()
                                .size(9.0)
                                .color(tokens.muted.egui()),
                        );
                        for choice in choices.iter().filter(|choice| choice.built_in) {
                            let selected = app.preferences.themes.selected() == choice.id;
                            if ui.selectable_label(selected, &choice.label).clicked() {
                                app.request_theme_selection(choice.id, context);
                                ui.close();
                            }
                        }
                        let saved = choices
                            .iter()
                            .filter(|choice| !choice.built_in)
                            .collect::<Vec<_>>();
                        if !saved.is_empty() {
                            ui.separator();
                            ui.label(
                                RichText::new("SAVED THEMES")
                                    .monospace()
                                    .size(9.0)
                                    .color(tokens.muted.egui()),
                            );
                            for choice in saved {
                                let selected = app.preferences.themes.selected() == choice.id;
                                if ui.selectable_label(selected, &choice.label).clicked() {
                                    app.request_theme_selection(choice.id, context);
                                    ui.close();
                                }
                            }
                        }
                    });
                if ui
                    .add_sized([112.0, 30.0], egui::Button::new("+ New theme"))
                    .clicked()
                {
                    if app.preferences.themes.begin_new_theme() {
                        app.preview_theme_changes(context);
                        app.theme_status = "New theme — enter a name and save".into();
                    } else {
                        app.theme_status = "Save or discard the current changes first".into();
                    }
                }
            });
            ui.horizontal(|ui| {
                let selected = app.preferences.themes.selected();
                ui.label(
                    RichText::new(match selected {
                        ThemeId::BuiltIn(_) => "Built-in · immutable",
                        ThemeId::Saved(_) => "Saved theme · editable",
                    })
                    .monospace()
                    .size(9.5)
                    .color(tokens.muted.egui()),
                );
                if app.preferences.themes.is_creating() {
                    ui.label(
                        RichText::new("CREATING")
                            .monospace()
                            .size(9.5)
                            .color(tokens.accent.egui()),
                    );
                } else if app.preferences.themes.is_dirty() {
                    ui.label(
                        RichText::new("UNSAVED CHANGES")
                            .monospace()
                            .size(9.5)
                            .color(tokens.danger.egui()),
                    );
                } else {
                    ui.label(
                        RichText::new("SAVED")
                            .monospace()
                            .size(9.5)
                            .color(tokens.accent.egui()),
                    );
                }
            });
        });
}

fn theme_editor(
    app: &mut ConvertalotApp,
    ui: &mut egui::Ui,
    context: &egui::Context,
    tokens: &ThemeTokens,
) {
    let editing_saved = app.preferences.themes.editing_saved().is_some();
    let name_editable = app.preferences.themes.is_creating()
        || (!editing_saved && app.preferences.themes.is_dirty());
    egui::Frame::new()
        .fill(tokens.panel.egui())
        .stroke(Stroke::new(1.0, tokens.border.egui()))
        .corner_radius(9.0)
        .inner_margin(egui::Margin::symmetric(14, 10))
        .show(ui, |ui| {
            let mut name = app.preferences.themes.draft_name().to_owned();
            ui.horizontal(|ui| {
                ui.add_sized([92.0, 27.0], egui::Label::new("Theme name"));
                let hint = if editing_saved {
                    "Use Rename to change this saved name"
                } else if name_editable {
                    "e.g. Warm Glass"
                } else {
                    "Edit a value to name a custom copy"
                };
                if ui
                    .add_sized(
                        [(ui.available_width() - 4.0).max(220.0), 27.0],
                        centered_text_edit(&mut name)
                            .hint_text(hint)
                            .interactive(name_editable),
                    )
                    .changed()
                {
                    app.preferences.themes.set_draft_name(name);
                }
            });
            ui.add_space(5.0);

            let is_glass = app
                .preferences
                .themes
                .resolved_appearance()
                .material
                .is_glass();
            ui.horizontal(|ui| {
                ui.add_sized([92.0, 27.0], egui::Label::new("Material"));
                if ui.selectable_label(!is_glass, "Solid").clicked() {
                    app.preferences.themes.set_solid();
                    app.preview_theme_changes(context);
                }
                if ui.selectable_label(is_glass, "Glass").clicked() {
                    app.preferences.themes.set_glass();
                    app.preview_theme_changes(context);
                }
                ui.label(
                    RichText::new(if is_glass {
                        "Background color supplies the native Glass tint"
                    } else {
                        "Native Glass is disabled"
                    })
                    .size(10.0)
                    .color(tokens.muted.egui()),
                );
            });

            if is_glass {
                glass_editor(app, ui, context, tokens);
            }
            ui.add_space(4.0);
            ui.separator();
            ui.add_space(4.0);
            section(ui, "Color tokens");
            muted(ui, "Use #RRGGBB for opaque or #RRGGBBAA for alpha.", tokens);
            token_editor(app, ui, context);
            ui.add_space(6.0);
            theme_actions(app, ui, context, tokens, editing_saved);
        });
}

fn glass_editor(
    app: &mut ConvertalotApp,
    ui: &mut egui::Ui,
    context: &egui::Context,
    tokens: &ThemeTokens,
) {
    let WindowMaterial::Glass {
        mut blur,
        mut translucency,
        mut solid_when_inactive,
    } = app
        .preferences
        .themes
        .resolved_appearance()
        .material
        .clone()
    else {
        return;
    };
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.add_sized([92.0, 20.0], egui::Label::new("Blur"));
        changed |= ui
            .add(egui::Slider::new(&mut blur, 0..=64).suffix(" px"))
            .changed();
        ui.add_space(18.0);
        ui.add_sized([92.0, 20.0], egui::Label::new("Translucency"));
        changed |= ui
            .add(egui::Slider::new(&mut translucency, 0..=90).suffix("%"))
            .changed();
    });
    changed |= setting_toggle(ui, &mut solid_when_inactive, "Solid when inactive", tokens);
    if changed {
        app.preferences
            .themes
            .set_glass_values(blur, translucency, solid_when_inactive);
        app.preview_theme_changes(context);
    }
}

fn token_editor(app: &mut ConvertalotApp, ui: &mut egui::Ui, context: &egui::Context) {
    let mut custom = app.preferences.themes.resolved_appearance().tokens.clone();
    let mut changed = false;
    ui.columns(2, |columns| {
        token_group_heading(&mut columns[0], "SURFACES", &custom);
        changed |= token_row(
            &mut columns[0],
            0,
            "canvas",
            &mut custom.canvas,
            &mut app.theme_hex[0],
        );
        changed |= token_row(
            &mut columns[0],
            1,
            "window / tint",
            &mut custom.background,
            &mut app.theme_hex[1],
        );
        changed |= token_row(
            &mut columns[0],
            2,
            "panel",
            &mut custom.panel,
            &mut app.theme_hex[2],
        );
        changed |= token_row(
            &mut columns[0],
            3,
            "control",
            &mut custom.control,
            &mut app.theme_hex[3],
        );
        changed |= token_row(
            &mut columns[0],
            4,
            "field",
            &mut custom.field,
            &mut app.theme_hex[4],
        );
        changed |= token_row(
            &mut columns[0],
            5,
            "alternate row",
            &mut custom.row_alt,
            &mut app.theme_hex[5],
        );
        changed |= token_row(
            &mut columns[0],
            6,
            "border",
            &mut custom.border,
            &mut app.theme_hex[6],
        );
        token_group_heading(&mut columns[0], "CONTENT", &custom);
        changed |= token_row(
            &mut columns[0],
            7,
            "text",
            &mut custom.text,
            &mut app.theme_hex[7],
        );
        changed |= token_row(
            &mut columns[0],
            8,
            "muted",
            &mut custom.muted,
            &mut app.theme_hex[8],
        );

        token_group_heading(&mut columns[1], "ACTIONS", &custom);
        changed |= token_row(
            &mut columns[1],
            9,
            "accent",
            &mut custom.accent,
            &mut app.theme_hex[9],
        );
        changed |= token_row(
            &mut columns[1],
            10,
            "on accent",
            &mut custom.on_accent,
            &mut app.theme_hex[10],
        );
        changed |= token_row(
            &mut columns[1],
            11,
            "danger",
            &mut custom.danger,
            &mut app.theme_hex[11],
        );
        token_group_heading(&mut columns[1], "TITLE BAR", &custom);
        changed |= token_row(
            &mut columns[1],
            12,
            "title",
            &mut custom.title,
            &mut app.theme_hex[12],
        );
        changed |= token_row(
            &mut columns[1],
            13,
            "title text",
            &mut custom.title_text,
            &mut app.theme_hex[13],
        );
        changed |= token_row(
            &mut columns[1],
            14,
            "title muted",
            &mut custom.title_muted,
            &mut app.theme_hex[14],
        );
        changed |= token_row(
            &mut columns[1],
            15,
            "title rule",
            &mut custom.title_rule,
            &mut app.theme_hex[15],
        );
        changed |= token_row(
            &mut columns[1],
            16,
            "title control",
            &mut custom.title_control,
            &mut app.theme_hex[16],
        );
    });
    if changed {
        app.preferences.themes.set_draft_tokens(custom);
        app.preview_theme_changes(context);
        app.theme_status = "Unsaved changes".into();
    }
}

fn theme_actions(
    app: &mut ConvertalotApp,
    ui: &mut egui::Ui,
    context: &egui::Context,
    tokens: &ThemeTokens,
    editing_saved: bool,
) {
    ui.horizontal_wrapped(|ui| {
        let save_label = if editing_saved {
            "Save changes"
        } else {
            "Save as new theme"
        };
        if ui
            .add_enabled(
                app.preferences.themes.is_dirty(),
                egui::Button::new(
                    RichText::new(save_label)
                        .strong()
                        .color(tokens.on_accent.egui()),
                )
                .fill(tokens.accent.egui()),
            )
            .clicked()
        {
            match app.preferences.themes.save_draft() {
                Ok(_) => {
                    app.preview_theme_changes(context);
                    app.theme_status = app.save_theme_preferences("Theme saved".into());
                }
                Err(error) => app.theme_status = error,
            }
        }
        if ui
            .add_enabled(
                app.preferences.themes.is_dirty(),
                egui::Button::new("Discard changes"),
            )
            .clicked()
        {
            app.preferences.themes.discard_draft();
            app.preview_theme_changes(context);
            app.theme_status = "Changes discarded".into();
        }
        if editing_saved {
            if ui
                .add_enabled(
                    !app.preferences.themes.is_dirty(),
                    egui::Button::new("Rename"),
                )
                .clicked()
            {
                app.rename_theme = Some(app.preferences.themes.draft_name().to_owned());
            }
            if ui
                .add_enabled(
                    !app.preferences.themes.is_dirty(),
                    egui::Button::new(RichText::new("Delete").color(tokens.danger.egui())),
                )
                .clicked()
            {
                app.confirm_delete_theme = true;
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new(&app.theme_status)
                    .monospace()
                    .size(9.5)
                    .color(
                        if app.theme_status.starts_with("Could not")
                            || app.theme_status.starts_with("Enter")
                        {
                            tokens.danger.egui()
                        } else {
                            tokens.muted.egui()
                        },
                    ),
            );
        });
    });
}

pub(crate) fn theme_dialogs(app: &mut ConvertalotApp, context: &egui::Context) {
    if app.preferences.themes.pending_selection().is_some() {
        egui::Window::new("Unsaved theme changes")
            .id(egui::Id::new("dirty-theme-navigation"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(context, |ui| {
                ui.label("Save your changes before switching themes?");
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        app.resolve_dirty_navigation(DirtyDecision::Save, context);
                    }
                    if ui.button("Discard").clicked() {
                        app.resolve_dirty_navigation(DirtyDecision::Discard, context);
                    }
                    if ui.button("Cancel").clicked() {
                        app.resolve_dirty_navigation(DirtyDecision::Cancel, context);
                    }
                });
            });
    }

    if let Some(mut name) = app.rename_theme.take() {
        let mut keep_open = true;
        egui::Window::new("Rename theme")
            .id(egui::Id::new("rename-theme"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(context, |ui| {
                ui.add_sized([300.0, 28.0], centered_text_edit(&mut name));
                ui.horizontal(|ui| {
                    if ui.button("Rename").clicked() {
                        match app.preferences.themes.rename_selected(&name) {
                            Ok(_) => {
                                app.theme_status =
                                    app.save_theme_preferences("Theme renamed".into());
                                keep_open = false;
                            }
                            Err(error) => app.theme_status = error,
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        keep_open = false;
                    }
                });
            });
        if keep_open {
            app.rename_theme = Some(name);
        }
    }

    if app.confirm_delete_theme {
        egui::Window::new("Delete theme?")
            .id(egui::Id::new("delete-theme"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(context, |ui| {
                ui.label("This removes the selected saved theme.");
                ui.horizontal(|ui| {
                    if ui.button("Delete").clicked() {
                        match app.preferences.themes.delete_selected() {
                            Ok(_) => {
                                app.preview_theme_changes(context);
                                app.theme_status =
                                    app.save_theme_preferences("Theme deleted; using Dark".into());
                            }
                            Err(error) => app.theme_status = error,
                        }
                        app.confirm_delete_theme = false;
                    }
                    if ui.button("Cancel").clicked() {
                        app.confirm_delete_theme = false;
                    }
                });
            });
    }
}

fn token_group_heading(ui: &mut egui::Ui, text: &str, tokens: &ThemeTokens) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(200.0, 13.0), egui::Sense::hover());
    ui.painter().text(
        rect.left_center(),
        egui::Align2::LEFT_CENTER,
        text,
        egui::FontId::monospace(9.0),
        tokens.muted.egui(),
    );
}

fn token_row(
    ui: &mut egui::Ui,
    index: usize,
    label: &str,
    color: &mut HexColor,
    text: &mut String,
) -> bool {
    let mut picked = color.egui();
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 7.0;
        let (label_rect, _) = ui.allocate_exact_size(egui::vec2(157.0, 19.0), egui::Sense::hover());
        ui.painter().text(
            label_rect.left_center(),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(10.5),
            ui.visuals().text_color(),
        );
        if ui.color_edit_button_srgba(&mut picked).changed() {
            *color = HexColor::from(picked);
            *text = color.to_string();
            changed = true;
        }
        let response = ui.add_sized(
            [94.0, 19.0],
            centered_monospace_text_edit(text)
                .id_salt(("theme-token", index))
                .font(egui::TextStyle::Monospace),
        );
        if response.changed()
            && let Ok(parsed) = text.parse()
        {
            *color = parsed;
            changed = true;
        }
        if text.parse::<HexColor>().is_err() {
            let rect = response.rect;
            response.on_hover_text("Enter #RRGGBB or #RRGGBBAA");
            ui.painter().text(
                rect.right_center() - egui::vec2(7.0, 0.0),
                egui::Align2::CENTER_CENTER,
                "!",
                egui::FontId::monospace(10.0),
                Color32::from_rgb(240, 135, 106),
            );
        }
    });
    changed
}

fn centered_text_edit<'a>(text: &'a mut dyn egui::TextBuffer) -> egui::TextEdit<'a> {
    egui::TextEdit::singleline(text)
        .vertical_align(egui::Align::Center)
        // Arial's visible glyphs sit slightly above the center of its line box.
        .margin(egui::Margin {
            left: 4,
            right: 4,
            top: 3,
            bottom: 1,
        })
}

fn centered_monospace_text_edit<'a>(text: &'a mut dyn egui::TextBuffer) -> egui::TextEdit<'a> {
    egui::TextEdit::singleline(text)
        .vertical_align(egui::Align::Center)
        // Consolas needs a larger optical correction than Arial because its visible glyphs sit
        // higher within the line box.
        .margin(egui::Margin {
            left: 4,
            right: 4,
            top: 5,
            bottom: -1,
        })
}

fn centered_tall_monospace_text_edit<'a>(text: &'a mut dyn egui::TextBuffer) -> egui::TextEdit<'a> {
    egui::TextEdit::singleline(text)
        .vertical_align(egui::Align::Center)
        .margin(egui::Margin::symmetric(4, 2))
}
