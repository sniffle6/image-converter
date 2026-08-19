#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod settings;
mod theme;
mod theme_catalog;
mod windows;
mod workbench;

use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Convertalot")
            .with_inner_size([900.0, 620.0])
            .with_min_inner_size([720.0, 520.0])
            .with_decorations(false)
            .with_resizable(true)
            .with_drag_and_drop(true)
            // Create an alpha-capable swapchain up front. Dark/light still paint opaque surfaces;
            // Glass uses the alpha channel to reveal the supported Windows DWM backdrop.
            .with_transparent(true),
        ..Default::default()
    };
    eframe::run_native(
        "Convertalot",
        options,
        Box::new(|context| {
            install_windows_fonts(&context.egui_ctx);
            Ok(Box::new(app::ConvertalotApp::new(context)))
        }),
    )
}

fn install_windows_fonts(context: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    if let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\arial.ttf") {
        fonts
            .font_data
            .insert("Arial".into(), egui::FontData::from_owned(bytes).into());
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "Arial".into());
    }
    if let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\arialbd.ttf") {
        fonts.font_data.insert(
            "Arial Bold".into(),
            egui::FontData::from_owned(bytes).into(),
        );
        fonts.families.insert(
            egui::FontFamily::Name("Arial Bold".into()),
            vec!["Arial Bold".into()],
        );
    }
    if let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\consola.ttf") {
        fonts
            .font_data
            .insert("Consolas".into(), egui::FontData::from_owned(bytes).into());
        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .insert(0, "Consolas".into());
    }
    context.set_fonts(fonts);
}
