use eframe::egui;

const WINDOW_ICON_PNG: &[u8] = include_bytes!("../../assets/convertalot-256.png");
const TITLE_ICON_PNG: &[u8] = include_bytes!("../../assets/convertalot-32.png");

pub fn window_icon() -> egui::IconData {
    decode_png(WINDOW_ICON_PNG)
}

pub fn title_icon(ctx: &egui::Context) -> egui::TextureHandle {
    let image = image::load_from_memory(TITLE_ICON_PNG)
        .expect("title-bar icon")
        .into_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
    ctx.load_texture(
        "convertalot-title-icon",
        color_image,
        egui::TextureOptions::NEAREST,
    )
}

fn decode_png(bytes: &[u8]) -> egui::IconData {
    let image = image::load_from_memory(bytes)
        .expect("window icon")
        .into_rgba8();
    let (width, height) = image.dimensions();
    egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    }
}
