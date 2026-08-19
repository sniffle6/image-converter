use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    time::{Duration, Instant},
};

use aero_glass::{AeroGlass, GlassConfig, GlassStatus, InactiveBehavior};
use eframe::egui;
use image_converter::{
    BatchReport, ConversionEvent, ConversionPlan, ConversionRequest, Converter, DuplicateStyle,
    ItemId, OutputFormat, PlanError, ResizeMode, RgbColor,
};

use crate::{
    settings,
    theme::{Preferences, ThemeMode},
    windows, workbench,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ResizeChoice {
    Original,
    Fit,
    Exact,
    Percent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Screen {
    Workbench,
    Appearance,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Phase {
    Empty,
    Planning,
    Ready,
    Running,
    Complete,
    Failed(String),
    BatchFailed(String),
}

#[derive(Clone, Debug)]
pub(crate) enum RowState {
    Queued,
    Converting,
    Done { elapsed: Duration, output: PathBuf },
    Failed(String),
    Cancelled,
}

#[derive(Clone, Debug)]
pub(crate) struct QueueRow {
    pub id: ItemId,
    pub input: PathBuf,
    pub byte_size: u64,
    pub output: PathBuf,
    pub state: RowState,
}

#[derive(Default)]
pub(crate) struct QueueModel {
    pub rows: Vec<QueueRow>,
}

impl QueueModel {
    fn from_plan(plan: &ConversionPlan) -> Self {
        Self {
            rows: plan
                .items
                .iter()
                .map(|item| QueueRow {
                    id: item.id,
                    input: item.input.clone(),
                    byte_size: item.byte_size,
                    output: item.output.clone(),
                    state: RowState::Queued,
                })
                .collect(),
        }
    }

    pub fn apply_event(&mut self, event: ConversionEvent) {
        match event {
            ConversionEvent::BatchStarted { .. } => {}
            ConversionEvent::ItemStarted { id } => {
                if let Some(row) = self.rows.iter_mut().find(|row| row.id == id) {
                    row.state = RowState::Converting;
                }
            }
            ConversionEvent::ItemFinished(result) => {
                if let Some(row) = self.rows.iter_mut().find(|row| row.id == result.id) {
                    row.state = if result.cancelled {
                        RowState::Cancelled
                    } else if let Some(error) = result.error {
                        RowState::Failed(error)
                    } else if let Some(output) = result.output {
                        RowState::Done {
                            elapsed: result.elapsed,
                            output,
                        }
                    } else {
                        RowState::Failed("conversion produced no output".to_owned())
                    };
                }
            }
        }
    }

    fn apply_report(&mut self, report: &BatchReport) {
        for result in &report.results {
            self.apply_event(ConversionEvent::ItemFinished(result.clone()));
        }
    }

    pub fn completed(&self) -> usize {
        self.rows
            .iter()
            .filter(|r| {
                matches!(
                    r.state,
                    RowState::Done { .. } | RowState::Failed(_) | RowState::Cancelled
                )
            })
            .count()
    }
    pub fn failed(&self) -> usize {
        self.rows
            .iter()
            .filter(|r| matches!(r.state, RowState::Failed(_)))
            .count()
    }
}

enum PlannerMessage {
    Planned(Result<ConversionPlan, PlanError>),
}
enum WorkerMessage {
    Event(ConversionEvent),
    Finished(BatchReport),
}

pub(crate) struct ConvertalotApp {
    pub phase: Phase,
    pub screen: Screen,
    pub rows: QueueModel,
    pub preferences: Preferences,
    pub format: OutputFormat,
    pub resize_choice: ResizeChoice,
    pub width: u32,
    pub height: u32,
    pub percent: u16,
    pub quality: u8,
    pub overwrite: bool,
    pub duplicate_style: DuplicateStyle,
    pub output_dir: Option<PathBuf>,
    pub jpeg_hex: String,
    pub theme_status: String,
    pub theme_hex: Vec<String>,
    pub planning_failures: Vec<String>,
    source_inputs: Vec<PathBuf>,
    plan: Option<ConversionPlan>,
    planner: Option<Receiver<PlannerMessage>>,
    worker: Option<Receiver<WorkerMessage>>,
    cancellation: Option<Arc<AtomicBool>>,
    report: Option<BatchReport>,
    run_started: Option<Instant>,
    aero_glass: Option<AeroGlass>,
    glass_failed: bool,
    rounded_corner_viewport: Option<(u32, u32, bool)>,
}

impl ConvertalotApp {
    pub fn new(context: &eframe::CreationContext<'_>) -> Self {
        let preferences = Preferences::load();
        preferences.apply(&context.egui_ctx);
        let theme_hex = token_strings(&preferences.custom_tokens);
        Self {
            phase: Phase::Empty,
            screen: Screen::Workbench,
            rows: QueueModel::default(),
            jpeg_hex: preferences.jpeg_background.to_string(),
            preferences,
            format: OutputFormat::Jpeg,
            resize_choice: ResizeChoice::Fit,
            width: 1920,
            height: 1080,
            percent: 50,
            quality: 90,
            overwrite: false,
            duplicate_style: DuplicateStyle::Dash,
            output_dir: None,
            theme_status: String::new(),
            theme_hex,
            planning_failures: Vec::new(),
            source_inputs: Vec::new(),
            plan: None,
            planner: None,
            worker: None,
            cancellation: None,
            report: None,
            run_started: None,
            aero_glass: None,
            glass_failed: false,
            rounded_corner_viewport: None,
        }
    }

    pub fn is_running(&self) -> bool {
        self.phase == Phase::Running
    }

    pub(crate) fn backdrop_fill(&self, fallback: crate::theme::HexColor) -> egui::Color32 {
        if self.glass_native_active() {
            egui::Color32::TRANSPARENT
        } else {
            fallback.egui()
        }
    }

    pub(crate) fn glass_native_active(&self) -> bool {
        self.preferences.active_theme == ThemeMode::Glass
            && self
                .aero_glass
                .as_ref()
                .is_some_and(|glass| glass.status() == GlassStatus::Active)
    }

    fn glass_config(&self) -> GlassConfig {
        let tint = crate::theme::ThemeTokens::glass().background;
        GlassConfig {
            tint: aero_glass::RgbColor::new(tint.0[0], tint.0[1], tint.0[2]),
            translucency: self.preferences.glass_translucency,
            blur_radius: f32::from(self.preferences.glass_blur),
            inactive_behavior: if self.preferences.solid_when_inactive {
                InactiveBehavior::Solid
            } else {
                InactiveBehavior::KeepGlass
            },
        }
    }

    fn sync_glass(&mut self, frame: &eframe::Frame, context: &egui::Context, focused: bool) {
        let corner_viewport = context.input(|input| {
            let viewport = input.viewport();
            let size = viewport
                .outer_rect
                .map(|rect| rect.size())
                .unwrap_or_else(|| input.content_rect().size());
            let scale = viewport.native_pixels_per_point.unwrap_or(1.0);
            (
                (size.x * scale).round() as u32,
                (size.y * scale).round() as u32,
                viewport.maximized.unwrap_or(false),
            )
        });
        if self.rounded_corner_viewport != Some(corner_viewport) {
            // A native region explicitly clips the borderless swapchain; DWM preference alone
            // only rounds the shadow on some borderless-window configurations.
            let _ = aero_glass::set_rounded_corners(frame);
            self.rounded_corner_viewport = Some(corner_viewport);
        }
        if self.preferences.active_theme != ThemeMode::Glass {
            self.aero_glass = None;
            self.glass_failed = false;
            context.send_viewport_cmd(egui::ViewportCommand::Transparent(false));
            return;
        }

        let config = self.glass_config();
        if self.aero_glass.is_none() && !self.glass_failed {
            match AeroGlass::attach(frame, config) {
                Ok(glass) => self.aero_glass = Some(glass),
                Err(error) => {
                    self.glass_failed = true;
                    self.theme_status = format!("Glass unavailable: {error}");
                }
            }
        }
        if let Some(glass) = &mut self.aero_glass
            && glass
                .update(config)
                .and_then(|_| glass.set_active(focused))
                .is_err()
        {
            self.aero_glass = None;
            self.glass_failed = true;
            self.theme_status = "Glass unavailable; using the solid fallback".into();
        }
        context.send_viewport_cmd(egui::ViewportCommand::Transparent(
            self.glass_native_active(),
        ));
    }

    pub fn add_paths(&mut self, paths: impl IntoIterator<Item = PathBuf>, context: &egui::Context) {
        if self.is_running() {
            return;
        }
        for path in paths {
            if !self.source_inputs.contains(&path) {
                self.source_inputs.push(path);
            }
        }
        self.start_planning(context);
    }

    pub fn clear(&mut self) {
        if self.is_running() {
            return;
        }
        self.source_inputs.clear();
        self.plan = None;
        self.rows.rows.clear();
        self.report = None;
        self.planner = None;
        self.phase = Phase::Empty;
        self.planning_failures.clear();
    }

    pub fn request(&self) -> ConversionRequest {
        ConversionRequest {
            inputs: self.source_inputs.clone(),
            output_dir: self.output_dir.clone(),
            format: self.format,
            resize: match self.resize_choice {
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
            },
            jpeg_quality: self.quality,
            jpeg_background: self.jpeg_hex.parse().unwrap_or(RgbColor::WHITE),
            overwrite: self.overwrite,
            duplicate_style: self.duplicate_style,
        }
    }

    pub fn settings_changed(&mut self, context: &egui::Context) {
        if !self.source_inputs.is_empty() && !self.is_running() {
            self.start_planning(context);
        }
    }

    fn start_planning(&mut self, context: &egui::Context) {
        if self.source_inputs.is_empty() {
            self.phase = Phase::Empty;
            return;
        }
        self.phase = Phase::Planning;
        self.report = None;
        self.planning_failures.clear();
        let request = self.request();
        let (sender, receiver) = mpsc::channel();
        let context = context.clone();
        std::thread::spawn(move || {
            let _ = sender.send(PlannerMessage::Planned(Converter.plan(request)));
            context.request_repaint();
        });
        self.planner = Some(receiver);
    }

    pub fn start_conversion(&mut self, context: &egui::Context) {
        let Some(plan) = self.plan.clone() else {
            return;
        };
        self.rows = QueueModel::from_plan(&plan);
        let cancellation = Arc::new(AtomicBool::new(false));
        let worker_cancellation = Arc::clone(&cancellation);
        let (sender, receiver) = mpsc::channel();
        let context = context.clone();
        std::thread::spawn(move || {
            let event_sender = sender.clone();
            let report = Converter.run(plan, &worker_cancellation, |event| {
                let _ = event_sender.send(WorkerMessage::Event(event));
                context.request_repaint();
            });
            let _ = sender.send(WorkerMessage::Finished(report));
            context.request_repaint();
        });
        self.phase = Phase::Running;
        self.worker = Some(receiver);
        self.cancellation = Some(cancellation);
        self.report = None;
        self.run_started = Some(Instant::now());
    }

    pub fn cancel(&self) {
        if let Some(flag) = &self.cancellation {
            flag.store(true, Ordering::Relaxed);
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.report
            .as_ref()
            .map(|r| r.elapsed)
            .or_else(|| self.run_started.map(|s| s.elapsed()))
            .unwrap_or_default()
    }

    pub fn concrete_output_dir(&self) -> Option<PathBuf> {
        if let Some(path) = &self.output_dir {
            return Some(path.clone());
        }
        let mut parents = self.rows.rows.iter().filter_map(|row| row.output.parent());
        let first = parents.next()?.to_path_buf();
        parents.all(|parent| parent == first).then_some(first)
    }

    fn poll(&mut self) {
        if let Some(receiver) = self.planner.take() {
            match receiver.try_recv() {
                Ok(PlannerMessage::Planned(Ok(plan))) => {
                    self.planning_failures = plan
                        .failures
                        .iter()
                        .map(|f| format!("{} — {}", f.path.display(), f.error))
                        .collect();
                    self.rows = QueueModel::from_plan(&plan);
                    self.plan = Some(plan);
                    self.phase = Phase::Ready;
                }
                Ok(PlannerMessage::Planned(Err(error))) => {
                    self.planning_failures = error
                        .failures
                        .iter()
                        .map(|f| format!("{} — {}", f.path.display(), f.error))
                        .collect();
                    self.plan = None;
                    self.rows.rows.clear();
                    self.phase = Phase::Failed(error.message);
                }
                Err(mpsc::TryRecvError::Empty) => self.planner = Some(receiver),
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.phase = Phase::Failed("queue planner stopped unexpectedly".to_owned())
                }
            }
        }
        if let Some(receiver) = self.worker.take() {
            let mut keep_receiver = true;
            loop {
                match receiver.try_recv() {
                    Ok(WorkerMessage::Event(event)) => self.rows.apply_event(event),
                    Ok(WorkerMessage::Finished(report)) => {
                        self.rows.apply_report(&report);
                        self.report = Some(report);
                        self.phase = Phase::Complete;
                        self.cancellation = None;
                        self.run_started = None;
                        keep_receiver = false;
                        break;
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        self.phase = Phase::BatchFailed(
                            "conversion worker stopped unexpectedly; finished files were preserved"
                                .to_owned(),
                        );
                        self.cancellation = None;
                        self.run_started = None;
                        keep_receiver = false;
                        break;
                    }
                }
            }
            if keep_receiver {
                self.worker = Some(receiver);
            }
        }
    }

    pub fn select_theme(&mut self, mode: ThemeMode, context: &egui::Context) {
        self.preferences.active_theme = mode;
        self.preferences.apply(context);
        self.theme_status = match self.preferences.save() {
            Ok(()) => format!(
                "Using the built-in {} theme",
                match mode {
                    ThemeMode::Light => "light",
                    ThemeMode::Dark => "dark",
                    ThemeMode::Glass => "glass",
                    ThemeMode::Custom => "custom",
                }
            ),
            Err(e) => format!("Could not save: {e}"),
        };
    }
}

fn token_strings(tokens: &crate::theme::ThemeTokens) -> Vec<String> {
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
    .map(|color| color.to_string())
    .collect()
}

impl eframe::App for ConvertalotApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let context = ui.ctx().clone();
        self.poll();
        let focused = context.input(|input| input.viewport().focused.unwrap_or(input.focused));
        self.sync_glass(frame, &context, focused);
        let dropped = context.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect::<Vec<_>>()
        });
        if !dropped.is_empty() {
            self.add_paths(dropped, &context);
        }
        windows::title_bar(self, ui);
        match self.screen {
            Screen::Workbench => {
                settings::conversion_sidebar(self, ui, &context);
                workbench::show(self, ui, &context);
            }
            Screen::Appearance => settings::appearance(self, ui, &context),
        }
        windows::resize_handles(&context);
        if self.is_running() {
            context.request_repaint_after(Duration::from_millis(100));
        }
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        if self.glass_native_active() {
            [0.0, 0.0, 0.0, 0.0]
        } else {
            self.preferences
                .tokens()
                .background
                .egui()
                .to_normalized_gamma_f32()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: usize) -> QueueRow {
        QueueRow {
            id: ItemId(id),
            input: "in".into(),
            output: "out".into(),
            byte_size: 1,
            state: RowState::Queued,
        }
    }

    #[test]
    fn out_of_order_events_update_by_identity() {
        let mut model = QueueModel {
            rows: vec![row(0), row(1)],
        };
        model.apply_event(ConversionEvent::ItemStarted { id: ItemId(1) });
        model.apply_event(ConversionEvent::ItemFinished(
            image_converter::ConversionResult {
                id: ItemId(1),
                input: "in".into(),
                output: Some("out".into()),
                elapsed: Duration::from_millis(2),
                error: None,
                cancelled: false,
            },
        ));
        model.apply_event(ConversionEvent::ItemStarted { id: ItemId(0) });
        assert!(matches!(model.rows[0].state, RowState::Converting));
        assert!(matches!(model.rows[1].state, RowState::Done { .. }));
    }

    #[test]
    fn cancellation_state_is_distinct_from_failure() {
        let mut model = QueueModel { rows: vec![row(0)] };
        model.apply_event(ConversionEvent::ItemFinished(
            image_converter::ConversionResult {
                id: ItemId(0),
                input: "in".into(),
                output: None,
                elapsed: Duration::ZERO,
                error: None,
                cancelled: true,
            },
        ));
        assert!(matches!(model.rows[0].state, RowState::Cancelled));
        assert_eq!(model.failed(), 0);
    }
}
