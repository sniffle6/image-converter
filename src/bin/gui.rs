#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
};

use eframe::egui::{self, Color32, RichText, Stroke};
use image_converter::{
    BatchReport, ConversionEvent, ConversionRequest, Converter, OutputFormat, ResizeMode,
};
use rfd::FileDialog;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([860.0, 620.0])
            .with_min_inner_size([700.0, 500.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        "Image Converter",
        options,
        Box::new(|context| Ok(Box::new(ConverterApp::new(context)))),
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ResizeChoice {
    Original,
    Fit,
    Exact,
    Percent,
}

enum WorkerMessage {
    Event(ConversionEvent),
    Finished(BatchReport),
}

struct ConverterApp {
    files: Vec<PathBuf>,
    output_dir: Option<PathBuf>,
    format: OutputFormat,
    resize_choice: ResizeChoice,
    width: u32,
    height: u32,
    percent: u16,
    quality: u8,
    overwrite: bool,
    total: usize,
    completed: usize,
    failed: usize,
    recent: Vec<String>,
    report: Option<BatchReport>,
    receiver: Option<Receiver<WorkerMessage>>,
    cancellation: Option<Arc<AtomicBool>>,
}

impl ConverterApp {
    fn new(context: &eframe::CreationContext<'_>) -> Self {
        context.egui_ctx.set_theme(egui::ThemePreference::Light);
        let mut style = (*context.egui_ctx.style_of(egui::Theme::Light)).clone();
        style.visuals = egui::Visuals::light();
        style.visuals.panel_fill = Color32::from_rgb(244, 241, 233);
        style.visuals.window_fill = Color32::from_rgb(250, 248, 242);
        style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(231, 227, 217);
        style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(220, 214, 201);
        style.visuals.widgets.active.bg_fill = Color32::from_rgb(210, 91, 48);
        style.visuals.selection.bg_fill = Color32::from_rgb(210, 91, 48);
        style.visuals.hyperlink_color = Color32::from_rgb(154, 58, 30);
        style.spacing.item_spacing = egui::vec2(10.0, 10.0);
        context.egui_ctx.set_style_of(egui::Theme::Light, style);

        Self {
            files: Vec::new(),
            output_dir: None,
            format: OutputFormat::Png,
            resize_choice: ResizeChoice::Original,
            width: 1920,
            height: 1080,
            percent: 50,
            quality: 90,
            overwrite: false,
            total: 0,
            completed: 0,
            failed: 0,
            recent: Vec::new(),
            report: None,
            receiver: None,
            cancellation: None,
        }
    }

    fn is_running(&self) -> bool {
        self.receiver.is_some()
    }

    fn add_paths(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        for path in paths {
            if !self.files.contains(&path) {
                self.files.push(path);
            }
        }
    }

    fn start(&mut self, context: egui::Context) {
        let resize = match self.resize_choice {
            ResizeChoice::Original => ResizeMode::Original,
            ResizeChoice::Fit => ResizeMode::Fit {
                width: self.width,
                height: self.height,
            },
            ResizeChoice::Exact => ResizeMode::Exact {
                width: self.width,
                height: self.height,
            },
            ResizeChoice::Percent => ResizeMode::Percent(self.percent),
        };
        let request = ConversionRequest {
            inputs: self.files.clone(),
            output_dir: self.output_dir.clone(),
            format: self.format,
            resize,
            jpeg_quality: self.quality,
            overwrite: self.overwrite,
        };
        let cancellation = Arc::new(AtomicBool::new(false));
        let worker_cancellation = Arc::clone(&cancellation);
        let (sender, receiver) = mpsc::channel();

        std::thread::spawn(move || {
            let event_sender = sender.clone();
            let report = Converter.run(request, &worker_cancellation, |event| {
                let _ = event_sender.send(WorkerMessage::Event(event));
                context.request_repaint();
            });
            let _ = sender.send(WorkerMessage::Finished(report));
            context.request_repaint();
        });

        self.total = 0;
        self.completed = 0;
        self.failed = 0;
        self.recent.clear();
        self.report = None;
        self.receiver = Some(receiver);
        self.cancellation = Some(cancellation);
    }

    fn poll_worker(&mut self) {
        let Some(receiver) = self.receiver.take() else {
            return;
        };
        let mut finished = false;
        while let Ok(message) = receiver.try_recv() {
            match message {
                WorkerMessage::Event(ConversionEvent::Started { total }) => self.total = total,
                WorkerMessage::Event(ConversionEvent::ItemFinished(result)) => {
                    self.completed += 1;
                    if let Some(error) = result.error {
                        self.failed += 1;
                        self.recent
                            .push(format!("FAILED  {} — {error}", result.input.display()));
                    } else if let Some(output) = result.output {
                        self.recent.push(format!("DONE    {}", output.display()));
                    }
                    if self.recent.len() > 8 {
                        self.recent.remove(0);
                    }
                }
                WorkerMessage::Finished(report) => {
                    self.report = Some(report);
                    self.cancellation = None;
                    finished = true;
                }
            }
        }
        if !finished {
            self.receiver = Some(receiver);
        }
    }
}

impl eframe::App for ConverterApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let context = ui.ctx().clone();
        self.poll_worker();
        let dropped = context.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .filter_map(|file| file.path.clone())
                .collect::<Vec<_>>()
        });
        if !self.is_running() {
            self.add_paths(dropped);
        }

        egui::Panel::top("header")
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(38, 39, 36))
                    .inner_margin(18.0),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("IMAGE CONVERTER")
                            .strong()
                            .size(21.0)
                            .color(Color32::from_rgb(244, 241, 233)),
                    );
                    ui.separator();
                    ui.label(
                        RichText::new("Windows batch workbench")
                            .monospace()
                            .color(Color32::from_rgb(181, 178, 168)),
                    );
                });
            });

        egui::Panel::right("settings")
            .resizable(false)
            .exact_size(260.0)
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(250, 248, 242))
                    .inner_margin(18.0),
            )
            .show(ui, |ui| {
                ui.add_enabled_ui(!self.is_running(), |ui| {
                    ui.heading("Output");
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.format, OutputFormat::Png, "PNG");
                        ui.selectable_value(&mut self.format, OutputFormat::Jpeg, "JPEG");
                        ui.selectable_value(&mut self.format, OutputFormat::WebP, "WebP");
                    });
                    if self.format == OutputFormat::Jpeg {
                        ui.add(egui::Slider::new(&mut self.quality, 1..=100).text("quality"));
                    } else if self.format == OutputFormat::WebP {
                        ui.small("Lossless encoding");
                    }

                    ui.add_space(16.0);
                    ui.heading("Size");
                    egui::ComboBox::from_id_salt("resize")
                        .selected_text(match self.resize_choice {
                            ResizeChoice::Original => "Keep original",
                            ResizeChoice::Fit => "Fit inside",
                            ResizeChoice::Exact => "Exact dimensions",
                            ResizeChoice::Percent => "Percentage",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.resize_choice,
                                ResizeChoice::Original,
                                "Keep original",
                            );
                            ui.selectable_value(
                                &mut self.resize_choice,
                                ResizeChoice::Fit,
                                "Fit inside",
                            );
                            ui.selectable_value(
                                &mut self.resize_choice,
                                ResizeChoice::Exact,
                                "Exact dimensions",
                            );
                            ui.selectable_value(
                                &mut self.resize_choice,
                                ResizeChoice::Percent,
                                "Percentage",
                            );
                        });
                    match self.resize_choice {
                        ResizeChoice::Fit | ResizeChoice::Exact => {
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::DragValue::new(&mut self.width)
                                        .range(1..=100_000)
                                        .suffix(" w"),
                                );
                                ui.add(
                                    egui::DragValue::new(&mut self.height)
                                        .range(1..=100_000)
                                        .suffix(" h"),
                                );
                            });
                        }
                        ResizeChoice::Percent => {
                            ui.add(egui::Slider::new(&mut self.percent, 1..=1000).suffix("%"));
                        }
                        ResizeChoice::Original => {}
                    }

                    ui.add_space(16.0);
                    ui.heading("Destination");
                    ui.small(
                        self.output_dir
                            .as_ref()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| "A converted folder beside each source".to_owned()),
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Choose folder").clicked() {
                            self.output_dir = FileDialog::new()
                                .set_title("Choose output folder")
                                .pick_folder();
                        }
                        if self.output_dir.is_some() && ui.button("Reset").clicked() {
                            self.output_dir = None;
                        }
                    });
                    ui.checkbox(&mut self.overwrite, "Replace existing names");
                });
            });

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(244, 241, 233))
                    .inner_margin(20.0),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading(format!("Queue / {}", self.files.len()));
                    ui.add_space(8.0);
                    ui.add_enabled_ui(!self.is_running(), |ui| {
                        if ui.button("Add images").clicked()
                            && let Some(paths) = FileDialog::new()
                                .add_filter(
                                    "Images",
                                    &["png", "jpg", "jpeg", "webp", "bmp", "tif", "tiff", "gif"],
                                )
                                .pick_files()
                        {
                            self.add_paths(paths);
                        }
                        if ui.button("Add folder").clicked()
                            && let Some(path) = FileDialog::new().pick_folder()
                        {
                            self.add_paths([path]);
                        }
                        if !self.files.is_empty() && ui.button("Clear").clicked() {
                            self.files.clear();
                        }
                    });
                });

                ui.add_space(8.0);
                let queue_height = if self.is_running() || self.report.is_some() {
                    225.0
                } else {
                    350.0
                };
                egui::Frame::new()
                    .fill(Color32::from_rgb(250, 248, 242))
                    .stroke(Stroke::new(1.0, Color32::from_rgb(194, 189, 177)))
                    .inner_margin(12.0)
                    .show(ui, |ui| {
                        ui.set_min_height(queue_height);
                        if self.files.is_empty() {
                            ui.vertical_centered(|ui| {
                                ui.add_space(queue_height / 3.0);
                                ui.label(
                                    RichText::new("Drop images or folders here")
                                        .size(18.0)
                                        .strong(),
                                );
                                ui.small("PNG · JPEG · WebP · BMP · TIFF · GIF");
                            });
                        } else {
                            egui::ScrollArea::vertical()
                                .max_height(queue_height)
                                .show(ui, |ui| {
                                    for path in &self.files {
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                RichText::new("→")
                                                    .color(Color32::from_rgb(210, 91, 48)),
                                            );
                                            ui.label(path.display().to_string());
                                        });
                                    }
                                });
                        }
                    });

                ui.add_space(14.0);
                if self.is_running() {
                    let progress = if self.total == 0 {
                        0.0
                    } else {
                        self.completed as f32 / self.total as f32
                    };
                    ui.add(
                        egui::ProgressBar::new(progress)
                            .text(format!("{} / {}", self.completed, self.total)),
                    );
                    if ui.button("Cancel remaining").clicked()
                        && let Some(cancellation) = &self.cancellation
                    {
                        cancellation.store(true, Ordering::Relaxed);
                    }
                } else {
                    let convert = egui::Button::new(
                        RichText::new("CONVERT QUEUE")
                            .strong()
                            .color(Color32::WHITE),
                    )
                    .fill(Color32::from_rgb(210, 91, 48));
                    if ui.add_enabled(!self.files.is_empty(), convert).clicked() {
                        self.start(context.clone());
                    }
                }

                if let Some(report) = &self.report {
                    ui.label(
                        RichText::new(format!(
                            "{} converted · {} failed · {:.2?}",
                            report.succeeded(),
                            report.failed(),
                            report.elapsed
                        ))
                        .strong(),
                    );
                }
                for line in self.recent.iter().rev() {
                    ui.small(RichText::new(line).monospace());
                }
            });
    }
}
