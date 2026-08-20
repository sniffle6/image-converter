use eframe::egui::{self, RichText, Shape, Stroke};
use rfd::FileDialog;

use crate::app::{ConvertalotApp, Phase, RowState};

pub(crate) fn show(app: &mut ConvertalotApp, root: &mut egui::Ui, context: &egui::Context) {
    let tokens = app.tokens();
    let background_fill = app.backdrop_fill(tokens.background);
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(background_fill).inner_margin(20.0))
        .show(root, |ui| match &app.phase {
            Phase::Empty => empty(app, context, ui),
            Phase::Planning => planning(ui, &tokens),
            Phase::Failed(error) => failed(app, context, ui, error.clone()),
            Phase::Ready | Phase::Running | Phase::Complete | Phase::BatchFailed(_) => {
                queue(app, context, ui)
            }
        });
}

pub(crate) fn show_preview(app: &mut ConvertalotApp, root: &mut egui::Ui, context: &egui::Context) {
    if context.input(|input| input.key_pressed(egui::Key::Escape)) {
        app.close_preview();
        return;
    }

    let Some(preview) = app.preview.as_ref() else {
        app.close_preview();
        return;
    };
    let row_index = preview.row_index;
    let input = preview.input.clone();
    let byte_size = preview.byte_size;
    let dimensions = preview.dimensions;
    let texture = preview.texture.clone();
    let error = preview.error.clone();
    let row_count = app.rows.rows.len();
    let tokens = app.tokens();
    let background_fill = app.backdrop_fill(tokens.background);
    let mut close = false;
    let mut navigate_to = None;

    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(background_fill).inner_margin(20.0))
        .show(root, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .button(RichText::new("←  Back to queue").size(12.0))
                    .on_hover_text("Back to queue (Esc)")
                    .clicked()
                {
                    close = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_enabled(row_index + 1 < row_count, egui::Button::new("Next  →"))
                        .on_hover_text("Preview next image (Right arrow)")
                        .clicked()
                    {
                        navigate_to = Some(row_index + 1);
                    }
                    if ui
                        .add_enabled(row_index > 0, egui::Button::new("←  Previous"))
                        .on_hover_text("Preview previous image (Left arrow)")
                        .clicked()
                    {
                        navigate_to = Some(row_index - 1);
                    }
                });
            });

            ui.add(
                egui::Label::new(
                    RichText::new(
                        input
                            .file_name()
                            .map(|name| name.to_string_lossy())
                            .unwrap_or_default(),
                    )
                    .strong()
                    .size(15.0)
                    .color(tokens.text.egui()),
                )
                .truncate(),
            );

            ui.horizontal(|ui| {
                let dimensions = dimensions
                    .map(|(width, height)| format!("{width} × {height} px"))
                    .unwrap_or_else(|| "dimensions unavailable".to_owned());
                ui.label(
                    RichText::new(format!(
                        "{}  ·  {}  ·  {} of {}",
                        dimensions,
                        human_size(byte_size),
                        row_index + 1,
                        row_count
                    ))
                    .monospace()
                    .size(10.5)
                    .color(tokens.muted.egui()),
                );
            });
            ui.label(
                RichText::new(input.display().to_string())
                    .monospace()
                    .size(10.0)
                    .color(tokens.muted.egui()),
            );
            ui.add_space(4.0);

            let available = ui.available_size();
            let (canvas, _) = ui.allocate_exact_size(
                egui::vec2(available.x, available.y.max(160.0)),
                egui::Sense::hover(),
            );
            ui.painter().rect_filled(canvas, 8.0, tokens.panel.egui());
            ui.painter().rect_stroke(
                canvas,
                8.0,
                Stroke::new(1.0, tokens.border.egui()),
                egui::StrokeKind::Inside,
            );

            if let Some(texture) = texture {
                paint_checkerboard(ui.painter(), canvas.shrink(1.0), &tokens);
                let natural = texture.size_vec2();
                let bounds = canvas.shrink(18.0).size();
                let scale = (bounds.x / natural.x).min(bounds.y / natural.y).min(1.0);
                let display_size = natural * scale;
                let image_rect = egui::Rect::from_center_size(canvas.center(), display_size);
                ui.put(
                    image_rect,
                    egui::Image::new(&texture).fit_to_exact_size(display_size),
                );
            } else if let Some(error) = error {
                ui.scope_builder(egui::UiBuilder::new().max_rect(canvas), |ui| {
                    ui.centered_and_justified(|ui| {
                        ui.label(RichText::new(error).size(12.0).color(tokens.danger.egui()));
                    });
                });
            }
        });

    if context.input(|input| input.key_pressed(egui::Key::ArrowLeft)) && row_index > 0 {
        navigate_to = Some(row_index - 1);
    }
    if context.input(|input| input.key_pressed(egui::Key::ArrowRight)) && row_index + 1 < row_count
    {
        navigate_to = Some(row_index + 1);
    }
    if close {
        app.close_preview();
    } else if let Some(row_index) = navigate_to {
        app.open_preview(row_index, context);
    }
}

fn paint_checkerboard(
    painter: &egui::Painter,
    rect: egui::Rect,
    tokens: &crate::theme::ThemeTokens,
) {
    let cell = 16.0;
    let mut row = 0usize;
    let mut y = rect.top();
    while y < rect.bottom() {
        let mut column = 0usize;
        let mut x = rect.left();
        while x < rect.right() {
            let tile = egui::Rect::from_min_max(
                egui::pos2(x, y),
                egui::pos2((x + cell).min(rect.right()), (y + cell).min(rect.bottom())),
            );
            let color = if (row + column).is_multiple_of(2) {
                tokens.panel.egui()
            } else {
                tokens.control.egui()
            };
            painter.rect_filled(tile, 0.0, color);
            column += 1;
            x += cell;
        }
        row += 1;
        y += cell;
    }
}

fn empty(app: &mut ConvertalotApp, context: &egui::Context, ui: &mut egui::Ui) {
    let tokens = app.tokens();
    let panel_fill = app.backdrop_fill(tokens.panel);
    let available = ui.available_size();
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(available.x, (available.y - 56.0).max(280.0)),
        egui::Sense::click(),
    );
    ui.painter().rect_filled(rect, 14.0, panel_fill);
    dashed_rect(ui.painter(), rect.shrink(1.0), tokens.border.egui());
    let mut browse_clicked = false;
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space((rect.height() / 2.0 - 79.0).max(24.0));
            let (icon, _) = ui.allocate_exact_size(egui::vec2(58.0, 58.0), egui::Sense::hover());
            ui.painter().rect_stroke(
                icon,
                13.0,
                Stroke::new(2.0, tokens.accent.egui()),
                egui::StrokeKind::Inside,
            );
            let center = icon.center();
            ui.painter().line_segment(
                [
                    center + egui::vec2(0.0, -9.0),
                    center + egui::vec2(0.0, 11.0),
                ],
                Stroke::new(1.5, tokens.accent.egui()),
            );
            ui.painter().line_segment(
                [
                    center + egui::vec2(-4.0, 7.0),
                    center + egui::vec2(0.0, 11.0),
                ],
                Stroke::new(1.5, tokens.accent.egui()),
            );
            ui.painter().line_segment(
                [
                    center + egui::vec2(4.0, 7.0),
                    center + egui::vec2(0.0, 11.0),
                ],
                Stroke::new(1.5, tokens.accent.egui()),
            );
            ui.add_space(10.0);
            ui.label(
                RichText::new("Drop images here")
                    .size(27.0)
                    .strong()
                    .family(egui::FontFamily::Name("Arial Bold".into()))
                    .color(tokens.text.egui()),
            );
            ui.add_space(1.0);
            ui.allocate_ui_with_layout(
                egui::vec2(360.0, 20.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    browse_clicked |= browse_prompt(ui, &tokens);
                },
            );
            ui.label(
                RichText::new("PNG · JPEG · WEBP · BMP · TIFF · GIF")
                    .monospace()
                    .size(10.5)
                    .color(tokens.muted.egui()),
            );
        });
    });
    ui.advance_cursor_after_rect(rect);
    if browse_clicked || response.clicked() {
        browse_files(app, context);
    }
    ui.add_space(14.0);
    ui.horizontal(|ui| {
        ui.add_enabled(
            false,
            egui::Button::new(
                RichText::new("CONVERT QUEUE")
                    .strong()
                    .size(13.5)
                    .color(tokens.muted.egui()),
            )
            .min_size(egui::vec2(185.0, 41.0)),
        );
        ui.add_space(4.0);
        ui.label(
            RichText::new("Add images to start")
                .size(12.0)
                .color(tokens.muted.egui()),
        );
    });
}

fn browse_prompt(ui: &mut egui::Ui, tokens: &crate::theme::ThemeTokens) -> bool {
    let (row, _) = ui.allocate_exact_size(egui::vec2(360.0, 20.0), egui::Sense::hover());
    let font = egui::FontId::proportional(13.0);
    let before = ui
        .painter()
        .layout_no_wrap("or".to_owned(), font.clone(), tokens.muted.egui());
    let link = ui.painter().layout_no_wrap(
        "browse files".to_owned(),
        font.clone(),
        tokens.accent.egui(),
    );
    let after = ui.painter().layout_no_wrap(
        "· folders are scanned for you".to_owned(),
        font,
        tokens.muted.egui(),
    );
    let gap = 8.0;
    let total_width = before.size().x + link.size().x + after.size().x + gap * 2.0;
    let top = row.center().y - before.size().y / 2.0;
    let before_pos = egui::pos2(row.center().x - total_width / 2.0, top);
    let link_pos = egui::pos2(before_pos.x + before.size().x + gap, top);
    let after_pos = egui::pos2(link_pos.x + link.size().x + gap, top);
    let link_rect = egui::Rect::from_min_size(link_pos, link.size()).expand2(egui::vec2(2.0, 1.0));
    let response = ui.interact(
        link_rect,
        ui.id().with("browse-files-link"),
        egui::Sense::click(),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Link, ui.is_enabled(), "browse files")
    });
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    ui.painter().galley(before_pos, before, tokens.muted.egui());
    ui.painter().galley(link_pos, link, tokens.accent.egui());
    ui.painter().galley(after_pos, after, tokens.muted.egui());
    if response.hovered() || response.has_focus() {
        ui.painter().line_segment(
            [
                egui::pos2(link_rect.left() + 2.0, link_rect.bottom()),
                egui::pos2(link_rect.right() - 2.0, link_rect.bottom()),
            ],
            Stroke::new(1.0, tokens.accent.egui()),
        );
    }

    response.clicked()
        || (response.has_focus()
            && ui.input(|input| {
                input.key_pressed(egui::Key::Enter) || input.key_pressed(egui::Key::Space)
            }))
}

fn dashed_rect(painter: &egui::Painter, rect: egui::Rect, color: egui::Color32) {
    let radius = 13.0;
    let stroke = Stroke::new(1.5, color);
    let lines = [
        [
            rect.left_top() + egui::vec2(radius, 0.0),
            rect.right_top() + egui::vec2(-radius, 0.0),
        ],
        [
            rect.right_top() + egui::vec2(0.0, radius),
            rect.right_bottom() + egui::vec2(0.0, -radius),
        ],
        [
            rect.right_bottom() + egui::vec2(-radius, 0.0),
            rect.left_bottom() + egui::vec2(radius, 0.0),
        ],
        [
            rect.left_bottom() + egui::vec2(0.0, -radius),
            rect.left_top() + egui::vec2(0.0, radius),
        ],
    ];
    for line in lines {
        painter.extend(Shape::dashed_line(&line, stroke, 6.0, 6.0));
    }
}

fn planning(ui: &mut egui::Ui, tokens: &crate::theme::ThemeTokens) {
    ui.vertical_centered(|ui| {
        ui.add_space(160.0);
        ui.spinner();
        ui.add_space(8.0);
        ui.label(
            RichText::new("Scanning folders and reserving output names…")
                .strong()
                .size(16.0)
                .color(tokens.text.egui()),
        );
        ui.label(
            RichText::new("Image pixels are decoded only when conversion starts.")
                .size(11.5)
                .color(tokens.muted.egui()),
        );
    });
}

fn failed(app: &mut ConvertalotApp, context: &egui::Context, ui: &mut egui::Ui, error: String) {
    let tokens = app.tokens();
    ui.vertical_centered(|ui| {
        ui.add_space(120.0);
        ui.label(
            RichText::new("Queue could not be prepared")
                .strong()
                .size(20.0)
                .color(tokens.danger.egui()),
        );
        ui.label(error);
        for failure in &app.planning_failures {
            ui.label(
                RichText::new(failure)
                    .monospace()
                    .size(10.5)
                    .color(tokens.muted.egui()),
            );
        }
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if ui.button("Add images").clicked() {
                browse_files(app, context);
            }
            if ui.button("Clear").clicked() {
                app.clear();
            }
        });
    });
}

fn queue(app: &mut ConvertalotApp, context: &egui::Context, ui: &mut egui::Ui) {
    let tokens = app.tokens();
    let running = app.is_running();
    if running {
        running_queue(app, ui, &tokens);
        return;
    }
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("Queue / {}", app.rows.rows.len()))
                .strong()
                .size(15.0)
                .color(tokens.text.egui()),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add_enabled(
                    !running,
                    egui::Button::new("Clear").min_size(egui::vec2(56.0, 28.0)),
                )
                .clicked()
            {
                app.clear();
            }
            if ui
                .add_enabled(
                    !running,
                    egui::Button::new("Add folder").min_size(egui::vec2(86.0, 28.0)),
                )
                .clicked()
                && let Some(path) = FileDialog::new().pick_folder()
            {
                app.add_paths([path], context);
            }
            if ui
                .add_enabled(
                    !running,
                    egui::Button::new("Add images").min_size(egui::vec2(94.0, 28.0)),
                )
                .clicked()
            {
                browse_files(app, context);
            }
        });
    });
    ui.add_space(4.0);
    if let Some(row_index) = queue_table(app, ui, &tokens) {
        app.open_preview(row_index, context);
        return;
    }
    for failure in &app.planning_failures {
        ui.label(
            RichText::new(failure)
                .monospace()
                .size(10.0)
                .color(tokens.danger.egui()),
        );
    }
    if app.phase == Phase::Ready {
        // The global 8px item gap combines with this to match the 11px table-to-drop spacing.
        ui.add_space(3.0);
        let (drop_rect, drop_response) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 42.0), egui::Sense::click());
        dashed_rect(ui.painter(), drop_rect.shrink(1.0), tokens.border.egui());
        ui.put(
            drop_rect,
            egui::Label::new(
                RichText::new("Drop more images or folders here")
                    .size(12.0)
                    .color(tokens.muted.egui()),
            )
            .sense(egui::Sense::click()),
        );
        if drop_response
            .on_hover_text("Click to add images, or drop images and folders anywhere in the window")
            .clicked()
        {
            browse_files(app, context);
        }
        ui.allocate_ui_with_layout(
            ui.available_size(),
            egui::Layout::bottom_up(egui::Align::LEFT),
            |ui| ready_footer(app, context, ui, &tokens),
        );
        return;
    }
    if matches!(app.phase, Phase::Complete | Phase::BatchFailed(_)) {
        let completed = app.rows.completed();
        let total = app.rows.rows.len();
        ui.add_space(8.0);
        ui.add(
            egui::ProgressBar::new(if total == 0 {
                0.0
            } else {
                completed as f32 / total as f32
            })
            .fill(tokens.accent.egui()),
        );
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!(
                    "{completed} / {total} · {} failed",
                    app.rows.failed()
                ))
                .monospace()
                .size(11.5)
                .color(tokens.muted.egui()),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{:.1}s elapsed", app.elapsed().as_secs_f32()))
                        .monospace()
                        .size(11.5)
                        .color(tokens.muted.egui()),
                );
            });
        });
    }
    ui.add_space(10.0);
    match app.phase {
        Phase::Ready => unreachable!("ready queue returns after drawing its footer"),
        Phase::Running => unreachable!("running queue returns after drawing its footer"),
        Phase::Complete | Phase::BatchFailed(_) => {
            if let Phase::BatchFailed(error) = &app.phase {
                ui.label(RichText::new(error).size(11.5).color(tokens.danger.egui()));
                ui.add_space(4.0);
            }
            ui.horizontal(|ui| {
                if ui
                    .add(egui::Button::new("RUN AGAIN").fill(tokens.accent.egui()))
                    .clicked()
                {
                    app.start_conversion(context);
                }
                if ui.button("Clear queue").clicked() {
                    app.clear();
                }
                let cancelled = app
                    .rows
                    .rows
                    .iter()
                    .filter(|r| matches!(r.state, RowState::Cancelled))
                    .count();
                ui.label(
                    RichText::new(format!(
                        "{} done · {} failed · {cancelled} cancelled",
                        app.rows.rows.len() - app.rows.failed() - cancelled,
                        app.rows.failed()
                    ))
                    .size(11.5)
                    .color(tokens.muted.egui()),
                );
            });
        }
        Phase::Empty | Phase::Planning | Phase::Failed(_) => {}
    }
}

fn running_queue(app: &mut ConvertalotApp, ui: &mut egui::Ui, tokens: &crate::theme::ThemeTokens) {
    if let Some(row_index) = queue_table(app, ui, tokens) {
        let context = ui.ctx().clone();
        app.open_preview(row_index, &context);
        return;
    }

    let completed = app.rows.completed();
    let total = app.rows.rows.len();
    let progress = if total == 0 {
        0.0
    } else {
        completed as f32 / total as f32
    };
    ui.add_space(8.0);
    let (track, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 10.0), egui::Sense::hover());
    ui.painter().rect_filled(track, 5.0, tokens.control.egui());
    if progress > 0.0 {
        let fill = egui::Rect::from_min_max(
            track.min,
            egui::pos2(track.left() + track.width() * progress, track.bottom()),
        );
        ui.painter().rect_filled(fill, 5.0, tokens.accent.egui());
    }
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!(
                "{completed} / {total} · {} failed",
                app.rows.failed()
            ))
            .monospace()
            .size(11.5)
            .color(tokens.muted.egui()),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new(format!("{:.1}s elapsed", app.elapsed().as_secs_f32()))
                    .monospace()
                    .size(11.5)
                    .color(tokens.muted.egui()),
            );
        });
    });

    ui.allocate_ui_with_layout(
        ui.available_size(),
        egui::Layout::bottom_up(egui::Align::LEFT),
        |ui| {
            ui.horizontal(|ui| {
                if ui
                    .add(egui::Button::new("Cancel remaining").min_size(egui::vec2(150.0, 40.0)))
                    .clicked()
                {
                    app.cancel();
                }
                ui.add_space(2.0);
                ui.label(
                    RichText::new("Finished files are already saved")
                        .size(11.5)
                        .color(tokens.muted.egui()),
                );
            });
        },
    );
}

fn queue_table(
    app: &ConvertalotApp,
    ui: &mut egui::Ui,
    tokens: &crate::theme::ThemeTokens,
) -> Option<usize> {
    let width = ui.available_width();
    let panel_fill = tokens.panel.egui();
    let mut clicked_row = None;
    egui::Frame::new()
        .fill(panel_fill)
        .stroke(Stroke::new(1.0, tokens.border.egui()))
        .corner_radius(8.0)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            queue_header(ui, tokens, width);
            egui::ScrollArea::vertical()
                .max_height(300.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for (index, row) in app.rows.rows.iter().enumerate() {
                        if queue_row(ui, tokens, row, index, width) {
                            clicked_row = Some(index);
                        }
                    }
                });
        });
    clicked_row
}

fn queue_header(ui: &mut egui::Ui, tokens: &crate::theme::ThemeTokens, width: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 28.0), egui::Sense::hover());
    let line_y = rect.bottom() - 0.5;
    ui.painter().line_segment(
        [
            egui::pos2(rect.left(), line_y),
            egui::pos2(rect.right(), line_y),
        ],
        Stroke::new(1.0, tokens.border.egui()),
    );
    let columns = queue_column_rects(rect.shrink2(egui::vec2(12.0, 0.0)));
    for (column, text) in columns
        .into_iter()
        .zip(["FILE · CLICK TO PREVIEW", "SIZE", "STATUS"])
    {
        ui.painter().text(
            column.left_center(),
            egui::Align2::LEFT_CENTER,
            text,
            egui::FontId::new(10.0, egui::FontFamily::Monospace),
            tokens.muted.egui(),
        );
    }
}

fn queue_row(
    ui: &mut egui::Ui,
    tokens: &crate::theme::ThemeTokens,
    row: &crate::app::QueueRow,
    index: usize,
    width: f32,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 30.0), egui::Sense::click());
    if response.hovered() || response.has_focus() {
        ui.painter().rect_filled(rect, 0.0, tokens.control.egui());
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    } else if index % 2 == 1 {
        ui.painter().rect_filled(rect, 0.0, tokens.row_alt.egui());
    }
    let [file_rect, size_rect, status_rect] =
        queue_column_rects(rect.shrink2(egui::vec2(12.0, 0.0)));
    let name = row
        .input
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_default();
    ui.painter().with_clip_rect(file_rect).text(
        file_rect.left_center(),
        egui::Align2::LEFT_CENTER,
        &name,
        egui::FontId::new(11.5, egui::FontFamily::Monospace),
        tokens.text.egui(),
    );
    ui.interact(
        file_rect,
        ui.id().with(("queue-file", row.id.0)),
        egui::Sense::hover(),
    )
    .on_hover_text(format!(
        "Input: {}\nOutput: {}",
        row.input.display(),
        row.output.display()
    ));
    ui.painter().text(
        size_rect.left_center(),
        egui::Align2::LEFT_CENTER,
        human_size(row.byte_size),
        egui::FontId::new(11.0, egui::FontFamily::Monospace),
        tokens.muted.egui(),
    );
    queue_status(ui, tokens, row, status_rect);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            ui.is_enabled(),
            format!("Preview {name}"),
        )
    });
    response.clicked()
        || (response.has_focus()
            && ui.input(|input| {
                input.key_pressed(egui::Key::Enter) || input.key_pressed(egui::Key::Space)
            }))
}

fn queue_column_rects(rect: egui::Rect) -> [egui::Rect; 3] {
    let file_width = (rect.width() - 205.0).max(120.0);
    let size_left = rect.left() + file_width + 8.0;
    let status_left = size_left + 78.0;
    [
        egui::Rect::from_min_size(rect.min, egui::vec2(file_width, rect.height())),
        egui::Rect::from_min_size(
            egui::pos2(size_left, rect.top()),
            egui::vec2(70.0, rect.height()),
        ),
        egui::Rect::from_min_max(egui::pos2(status_left, rect.top()), rect.max),
    ]
}

fn queue_status(
    ui: &mut egui::Ui,
    tokens: &crate::theme::ThemeTokens,
    row: &crate::app::QueueRow,
    rect: egui::Rect,
) {
    let (status, color) = match &row.state {
        RowState::Queued => ("QUEUED".to_owned(), tokens.muted.egui()),
        RowState::Converting => ("• CONVERTING".to_owned(), tokens.accent.egui()),
        RowState::Done { elapsed, output: _ } => (
            format!("✓ DONE · {:.1}s", elapsed.as_secs_f32()),
            tokens.text.egui(),
        ),
        RowState::Failed(error) => (
            format!("× FAILED — {}", concise(error)),
            tokens.danger.egui(),
        ),
        RowState::Cancelled => ("CANCELLED".to_owned(), tokens.muted.egui()),
    };
    ui.painter().with_clip_rect(rect).text(
        rect.left_center(),
        egui::Align2::LEFT_CENTER,
        status,
        egui::FontId::new(10.0, egui::FontFamily::Monospace),
        color,
    );
    let response = ui.interact(
        rect,
        ui.id().with(("queue-status", row.id.0)),
        egui::Sense::hover(),
    );
    match &row.state {
        RowState::Failed(error) => {
            response.on_hover_text(error);
        }
        RowState::Done { output, .. } => {
            response.on_hover_text(output.display().to_string());
        }
        _ => {}
    }
}

fn ready_footer(
    app: &mut ConvertalotApp,
    context: &egui::Context,
    ui: &mut egui::Ui,
    tokens: &crate::theme::ThemeTokens,
) {
    let (footer_rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 41.0), egui::Sense::hover());
    ui.scope_builder(egui::UiBuilder::new().max_rect(footer_rect), |ui| {
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            if ui
                .add(
                    egui::Button::new(
                        RichText::new("CONVERT QUEUE")
                            .strong()
                            .size(13.5)
                            .color(tokens.on_accent.egui()),
                    )
                    .fill(tokens.accent.egui())
                    .min_size(egui::vec2(185.0, 41.0)),
                )
                .clicked()
            {
                app.start_conversion(context);
            }
            ui.add_space(4.0);
            ui.label(
                RichText::new(summary(app))
                    .size(11.5)
                    .color(tokens.muted.egui()),
            );
        });
    });
}

fn browse_files(app: &mut ConvertalotApp, context: &egui::Context) {
    if let Some(paths) = FileDialog::new()
        .add_filter(
            "Images",
            &["png", "jpg", "jpeg", "webp", "bmp", "tif", "tiff", "gif"],
        )
        .pick_files()
    {
        app.add_paths(paths, context);
    }
}

fn human_size(bytes: u64) -> String {
    if bytes >= 1_000_000 {
        format!("{:.1} MB", bytes as f64 / 1_000_000.0)
    } else if bytes >= 1_000 {
        format!("{:.1} KB", bytes as f64 / 1_000.0)
    } else {
        format!("{bytes} B")
    }
}

fn concise(error: &str) -> &str {
    if error.contains("decode") {
        "decode"
    } else if error.contains("open") {
        "open"
    } else if error.contains("output") {
        "output"
    } else {
        "error"
    }
}

fn summary(app: &ConvertalotApp) -> String {
    let format = match app.format {
        image_converter::OutputFormat::Png => "PNG".to_owned(),
        image_converter::OutputFormat::Jpeg => format!("JPEG {}", app.quality),
        image_converter::OutputFormat::WebP => "lossless WebP".to_owned(),
    };
    let resize = match app.resize_choice {
        crate::app::ResizeChoice::Original => "original size".to_owned(),
        crate::app::ResizeChoice::Fit => format!("fit inside {} × {}", app.width, app.height),
        crate::app::ResizeChoice::Exact => format!("exactly {} × {}", app.width, app.height),
        crate::app::ResizeChoice::Percent => format!("{}%", app.percent),
    };
    format!("{} images → {format}, {resize}", app.rows.rows.len())
}
