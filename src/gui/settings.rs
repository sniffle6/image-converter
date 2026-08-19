use eframe::egui::{self, Color32, RichText, Stroke};
use image_converter::{DuplicateStyle, OutputFormat, RgbColor};
use rfd::FileDialog;

use crate::{
    app::{ConvertalotApp, ResizeChoice},
    theme::{HexColor, ThemeMode, ThemeTokens},
    windows,
};

pub(crate) fn conversion_sidebar(
    app: &mut ConvertalotApp,
    root: &mut egui::Ui,
    context: &egui::Context,
) {
    let tokens = app.preferences.tokens();
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
            ui.add_enabled_ui(true, |ui| {
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
                destination_field(ui, app, &tokens, true);
                ui.horizontal(|ui| {
                    if ui
                        .add_sized([163.0, 29.0], egui::Button::new("Choose folder"))
                        .clicked()
                        && let Some(folder) = FileDialog::new()
                            .set_title("Choose output folder")
                            .pick_folder()
                    {
                        app.output_dir = Some(folder);
                        changed = true;
                    }
                    if ui
                        .add_enabled(
                            app.output_dir.is_some(),
                            egui::Button::new("Reset").min_size(egui::vec2(59.0, 29.0)),
                        )
                        .clicked()
                    {
                        app.output_dir = None;
                        changed = true;
                    }
                });
                muted(
                    ui,
                    "Defaults to a converted folder beside each source.",
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
            });
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
    let mut changed = false;
    ui.horizontal(|ui| {
        if ui
            .add_sized(
                [49.0, 24.0],
                egui::Button::selectable(app.jpeg_hex.eq_ignore_ascii_case("#FFFFFF"), "White"),
            )
            .clicked()
        {
            app.jpeg_hex = "#FFFFFF".into();
            changed = true;
        }
        if ui
            .add_sized(
                [46.0, 24.0],
                egui::Button::selectable(app.jpeg_hex.eq_ignore_ascii_case("#000000"), "Black"),
            )
            .clicked()
        {
            app.jpeg_hex = "#000000".into();
            changed = true;
        }
        let custom = !app.jpeg_hex.eq_ignore_ascii_case("#FFFFFF")
            && !app.jpeg_hex.eq_ignore_ascii_case("#000000");
        if ui
            .add_sized([58.0, 24.0], egui::Button::selectable(custom, "Custom"))
            .clicked()
            && !custom
        {
            app.jpeg_hex = "#808080".into();
            changed = true;
        }
        let parsed = app.jpeg_hex.parse::<RgbColor>();
        let rgb = parsed.unwrap_or(RgbColor::WHITE).components();
        let mut picked = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
        if ui.color_edit_button_srgba(&mut picked).changed() {
            app.jpeg_hex = format!("#{:02X}{:02X}{:02X}", picked.r(), picked.g(), picked.b());
            changed = true;
        }
    });
    let response = ui.add_sized(
        [ui.available_width(), 24.0],
        egui::TextEdit::singleline(&mut app.jpeg_hex)
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
    let visuals = ui.visuals();
    ui.add_sized(
        [105.0, 26.0],
        egui::Button::new(RichText::new(label).monospace().size(10.5))
            .fill(visuals.widgets.inactive.bg_fill)
            .stroke(if selected {
                Stroke::new(2.0, visuals.selection.bg_fill)
            } else {
                visuals.widgets.inactive.bg_stroke
            }),
    )
    .clicked()
    .then(|| *current = value)
    .is_some()
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
    let tokens = app.preferences.tokens();
    let panel_fill = app.backdrop_fill(tokens.panel);
    let background_fill = app.backdrop_fill(tokens.background);
    egui::Panel::left("settings-navigation")
        .resizable(false)
        .exact_size(158.0)
        .frame(
            egui::Frame::new()
                .fill(panel_fill)
                .stroke(Stroke::new(1.0, tokens.border.egui()))
                .inner_margin(egui::Margin::symmetric(12, 16)),
        )
        .show(root, |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            ui.add_sized(
                [134.0, 31.0],
                egui::Button::new(
                    RichText::new("Appearance")
                        .strong()
                        .color(tokens.on_accent.egui()),
                )
                .fill(tokens.accent.egui())
                .stroke(Stroke::NONE)
                .corner_radius(5.0),
            );
            for name in ["Conversion", "File names", "Performance", "About"] {
                ui.add_enabled(
                    false,
                    egui::Button::new(RichText::new(name).size(12.0))
                        .frame(false)
                        .min_size(egui::vec2(134.0, 30.0)),
                );
            }
        });
    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(background_fill)
                .inner_margin(egui::Margin::symmetric(20, 14)),
        )
        .show(root, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(10.0, 5.0);
            let content_width =
                (ui.ctx().input(|input| input.content_rect().width()) - 198.0).clamp(520.0, 760.0);
            ui.set_max_width(content_width);
            ui.label(
                RichText::new("Theme")
                    .strong()
                    .size(15.0)
                    .color(tokens.text.egui()),
            );
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                let card_width = ((content_width - 24.0) / 3.0).floor();
                theme_card(
                    ui,
                    app,
                    context,
                    ThemeMode::Light,
                    "Light",
                    ThemeTokens::light(),
                    card_width,
                );
                theme_card(
                    ui,
                    app,
                    context,
                    ThemeMode::Dark,
                    "Dark",
                    ThemeTokens::dark(),
                    card_width,
                );
                theme_card(
                    ui,
                    app,
                    context,
                    ThemeMode::Glass,
                    "Glass",
                    ThemeTokens::glass(),
                    card_width,
                );
            });

            ui.add_space(5.0);
            glass_settings(app, ui, &tokens, content_width);
            ui.add_space(5.0);

            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Colors")
                        .strong()
                        .size(15.0)
                        .color(tokens.text.egui()),
                );
                ui.label(
                    RichText::new("Editing any colour starts your own theme")
                        .size(11.0)
                        .color(tokens.muted.egui()),
                );
            });

            let editor_height = (ui.available_height() - 54.0).clamp(225.0, 250.0);
            let mut custom = app.preferences.custom_tokens.clone();
            let mut changed = false;
            ui.allocate_ui_with_layout(
                egui::vec2(content_width, editor_height),
                egui::Layout::left_to_right(egui::Align::TOP),
                |ui| {
                    ui.set_min_size(egui::vec2(content_width, editor_height));
                    let column_gap = 24.0;
                    let column_width = (content_width - column_gap) / 2.0;
                    ui.allocate_ui_with_layout(
                        egui::vec2(column_width, editor_height),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            ui.set_max_width(column_width);
                            ui.spacing_mut().item_spacing.y = 0.0;
                            token_group_heading(ui, "SURFACES", &tokens);
                            changed |= token_row(
                                ui,
                                0,
                                "desktop",
                                &mut custom.canvas,
                                &mut app.theme_hex[0],
                            );
                            changed |= token_row(
                                ui,
                                1,
                                "window",
                                &mut custom.background,
                                &mut app.theme_hex[1],
                            );
                            changed |=
                                token_row(ui, 2, "panel", &mut custom.panel, &mut app.theme_hex[2]);
                            changed |= token_row(
                                ui,
                                3,
                                "control",
                                &mut custom.control,
                                &mut app.theme_hex[3],
                            );
                            changed |=
                                token_row(ui, 4, "field", &mut custom.field, &mut app.theme_hex[4]);
                            changed |= token_row(
                                ui,
                                5,
                                "row stripe",
                                &mut custom.row_alt,
                                &mut app.theme_hex[5],
                            );
                            changed |= token_row(
                                ui,
                                6,
                                "border",
                                &mut custom.border,
                                &mut app.theme_hex[6],
                            );
                        },
                    );
                    ui.add_space(column_gap);
                    ui.allocate_ui_with_layout(
                        egui::vec2(column_width, editor_height),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            ui.set_max_width(column_width);
                            ui.spacing_mut().item_spacing.y = 0.0;
                            token_group_heading(ui, "TEXT", &tokens);
                            changed |=
                                token_row(ui, 7, "text", &mut custom.text, &mut app.theme_hex[7]);
                            changed |= token_row(
                                ui,
                                8,
                                "muted text",
                                &mut custom.muted,
                                &mut app.theme_hex[8],
                            );
                            token_group_heading(ui, "ACCENT & STATUS", &tokens);
                            changed |= token_row(
                                ui,
                                9,
                                "accent",
                                &mut custom.accent,
                                &mut app.theme_hex[9],
                            );
                            changed |= token_row(
                                ui,
                                10,
                                "on accent",
                                &mut custom.on_accent,
                                &mut app.theme_hex[10],
                            );
                            changed |= token_row(
                                ui,
                                11,
                                "error",
                                &mut custom.danger,
                                &mut app.theme_hex[11],
                            );
                            token_group_heading(ui, "TITLE BAR", &tokens);
                            changed |= token_row(
                                ui,
                                12,
                                "title bar",
                                &mut custom.title,
                                &mut app.theme_hex[12],
                            );
                            changed |= token_row(
                                ui,
                                13,
                                "title text",
                                &mut custom.title_text,
                                &mut app.theme_hex[13],
                            );
                            changed |= token_row(
                                ui,
                                14,
                                "title muted",
                                &mut custom.title_muted,
                                &mut app.theme_hex[14],
                            );
                            changed |= token_row(
                                ui,
                                15,
                                "title rule",
                                &mut custom.title_rule,
                                &mut app.theme_hex[15],
                            );
                            changed |= token_row(
                                ui,
                                16,
                                "title control",
                                &mut custom.title_control,
                                &mut app.theme_hex[16],
                            );
                        },
                    );
                },
            );
            app.preferences.custom_tokens = custom;
            if changed {
                app.preferences.active_theme = ThemeMode::Custom;
                app.preferences.apply(context);
                app.theme_status = "Unsaved — click Save to keep it".into();
            }

            ui.separator();
            ui.horizontal(|ui| {
                if ui
                    .add_sized(
                        [153.0, 34.0],
                        egui::Button::new(
                            RichText::new("Save as my theme")
                                .strong()
                                .color(tokens.on_accent.egui()),
                        )
                        .fill(tokens.accent.egui())
                        .stroke(Stroke::NONE),
                    )
                    .clicked()
                {
                    app.preferences.active_theme = ThemeMode::Custom;
                    app.preferences.apply(context);
                    app.theme_status = match app.preferences.save() {
                        Ok(()) => "Saved as My theme".into(),
                        Err(e) => format!("Could not save: {e}"),
                    };
                }
                if ui
                    .add_sized([135.0, 34.0], egui::Button::new("Reset to default"))
                    .clicked()
                {
                    app.preferences = crate::theme::Preferences::default();
                    app.preferences.apply(context);
                    app.theme_hex = token_values(&app.preferences.custom_tokens);
                    let _ = app.preferences.save();
                    app.theme_status = "Reset to the built-in dark theme".into();
                }
                let default_status = match app.preferences.active_theme {
                    ThemeMode::Light => "Using the built-in light theme",
                    ThemeMode::Dark => "Using the built-in dark theme",
                    ThemeMode::Glass => "Using the built-in glass theme",
                    ThemeMode::Custom => "Using My theme",
                };
                let status = if app.theme_status.is_empty() {
                    default_status
                } else {
                    &app.theme_status
                };
                let status_width = (content_width - 318.0).max(120.0);
                let (status_rect, _) =
                    ui.allocate_exact_size(egui::vec2(status_width, 34.0), egui::Sense::hover());
                ui.painter().text(
                    status_rect.right_center(),
                    egui::Align2::RIGHT_CENTER,
                    status,
                    egui::FontId::monospace(10.0),
                    tokens.muted.egui(),
                );
            });
        });
}

fn glass_settings(app: &mut ConvertalotApp, ui: &mut egui::Ui, tokens: &ThemeTokens, width: f32) {
    let panel_fill = tokens.panel.egui();
    egui::Frame::new()
        .fill(panel_fill)
        .stroke(Stroke::new(1.0, tokens.border.egui()))
        .corner_radius(9.0)
        .inner_margin(egui::Margin::symmetric(16, 10))
        .show(ui, |ui| {
            ui.set_width(width - 32.0);
            section(ui, "Glass");
            ui.horizontal(|ui| {
                ui.add_sized(
                    [102.0, 18.0],
                    egui::Label::new(
                        RichText::new("Blur")
                            .size(11.0)
                            .color(tokens.muted.egui()),
                    )
                    .halign(egui::Align::LEFT),
                );
                let slider_width = (ui.available_width() - 42.0).max(120.0);
                if ui
                    .add_sized(
                        [slider_width, 18.0],
                        egui::Slider::new(&mut app.preferences.glass_blur, 0..=64)
                            .show_value(false),
                    )
                    .on_hover_text("Adjusts the native Gaussian blur behind the window")
                    .changed()
                {
                    let _ = app.preferences.save();
                }
                ui.label(
                    RichText::new(format!("{}px", app.preferences.glass_blur))
                        .monospace()
                        .size(10.0)
                        .color(tokens.text.egui()),
                );
            });
            ui.horizontal(|ui| {
                ui.add_sized(
                    [102.0, 18.0],
                    egui::Label::new(
                        RichText::new("Translucency")
                            .size(11.0)
                            .color(tokens.muted.egui()),
                    )
                    .halign(egui::Align::LEFT),
                );
                let slider_width = (ui.available_width() - 42.0).max(120.0);
                if ui
                    .add_sized(
                        [slider_width, 18.0],
                        egui::Slider::new(&mut app.preferences.glass_translucency, 0..=90)
                            .show_value(false),
                    )
                    .on_hover_text("Higher values reveal more of the desktop through the native tint")
                    .changed()
                {
                    let _ = app.preferences.save();
                }
                ui.label(
                    RichText::new(format!("{}%", app.preferences.glass_translucency))
                        .monospace()
                        .size(10.0),
                );
            });
            if setting_toggle(
                ui,
                &mut app.preferences.solid_when_inactive,
                "Solid when inactive",
                tokens,
            ) {
                let _ = app.preferences.save();
            }
            ui.label(
                RichText::new(
                    "Blur and tint remain active when focus moves elsewhere. Solid when inactive is an optional readability fallback.",
                )
                .size(10.0)
                .color(tokens.muted.egui()),
            );
        });
}

fn theme_card(
    ui: &mut egui::Ui,
    app: &mut ConvertalotApp,
    context: &egui::Context,
    mode: ThemeMode,
    label: &str,
    preview: ThemeTokens,
    width: f32,
) {
    let selected = app.preferences.active_theme == mode;
    let tokens = app.preferences.tokens();
    let card_fill = tokens.panel.egui();
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 66.0), egui::Sense::click());
    let border = if selected {
        Stroke::new(2.0, tokens.accent.egui())
    } else if response.hovered() || response.has_focus() {
        Stroke::new(1.0, tokens.muted.egui())
    } else {
        Stroke::new(1.0, tokens.border.egui())
    };
    ui.painter()
        .rect(rect, 9.0, card_fill, border, egui::StrokeKind::Inside);
    let preview_rect = egui::Rect::from_min_max(
        rect.min + egui::vec2(10.0, 7.0),
        egui::pos2(rect.right() - 10.0, rect.top() + 45.0),
    );
    ui.painter()
        .rect_filled(preview_rect, 4.0, preview.background.egui());
    ui.painter().rect_filled(
        egui::Rect::from_min_size(preview_rect.min, egui::vec2(preview_rect.width(), 11.0)),
        4.0,
        preview.title.egui(),
    );
    ui.painter().rect_filled(
        egui::Rect::from_min_max(
            preview_rect.min + egui::vec2(7.0, 16.0),
            egui::pos2(preview_rect.right() - 35.0, preview_rect.bottom() - 5.0),
        ),
        3.0,
        preview.panel.egui(),
    );
    ui.painter().rect_filled(
        egui::Rect::from_min_max(
            egui::pos2(preview_rect.right() - 29.0, preview_rect.top() + 16.0),
            preview_rect.max - egui::vec2(7.0, 5.0),
        ),
        3.0,
        preview.accent.egui(),
    );
    let radio_center = egui::pos2(rect.left() + 16.0, rect.bottom() - 11.0);
    ui.painter()
        .circle_stroke(radio_center, 6.0, Stroke::new(1.0, tokens.border.egui()));
    if selected {
        ui.painter()
            .circle_filled(radio_center, 5.0, tokens.accent.egui());
    }
    ui.painter().text(
        egui::pos2(rect.left() + 29.0, rect.bottom() - 11.0),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::new(12.0, egui::FontFamily::Name("Arial Bold".into())),
        tokens.text.egui(),
    );
    let keyboard_activated = response.has_focus()
        && ui.input(|input| {
            input.key_pressed(egui::Key::Enter) || input.key_pressed(egui::Key::Space)
        });
    if response.clicked() || keyboard_activated {
        app.select_theme(mode, context);
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
        let (label_rect, _) = ui.allocate_exact_size(egui::vec2(187.0, 19.0), egui::Sense::hover());
        ui.painter().text(
            label_rect.left_center(),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(10.5),
            ui.visuals().text_color(),
        );
        if ui.color_edit_button_srgba(&mut picked).changed() {
            *color = HexColor([picked.r(), picked.g(), picked.b()]);
            *text = color.to_string();
            changed = true;
        }
        let response = ui.add_sized(
            [94.0, 19.0],
            egui::TextEdit::singleline(text)
                .id_salt(("theme-token", index))
                .font(egui::TextStyle::Monospace)
                .vertical_align(egui::Align::Center),
        );
        if response.changed()
            && let Ok(parsed) = text.parse()
        {
            *color = parsed;
            changed = true;
        }
        if text.parse::<HexColor>().is_err() {
            let rect = response.rect;
            response.on_hover_text("Enter a color as #RRGGBB");
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

fn token_values(tokens: &ThemeTokens) -> Vec<String> {
    [
        tokens.canvas,
        tokens.background,
        tokens.panel,
        tokens.control,
        tokens.field,
        tokens.row_alt,
        tokens.border,
        tokens.text,
        tokens.muted,
        tokens.accent,
        tokens.on_accent,
        tokens.danger,
        tokens.title,
        tokens.title_text,
        tokens.title_muted,
        tokens.title_rule,
        tokens.title_control,
    ]
    .into_iter()
    .map(|c| c.to_string())
    .collect()
}
