use std::{
    collections::HashSet,
    fmt,
    fs::{self, File},
    io::BufWriter,
    path::{Path, PathBuf},
    str::FromStr,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use image::{
    DynamicImage, GenericImageView, ImageDecoder, ImageEncoder, ImageReader, RgbaImage,
    codecs::{jpeg::JpegEncoder, png::PngEncoder, webp::WebPEncoder},
};
use rayon::prelude::*;
use thiserror::Error;
use walkdir::WalkDir;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum OutputFormat {
    #[default]
    Png,
    Jpeg,
    WebP,
}

impl OutputFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::WebP => "webp",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ResizeMode {
    #[default]
    Original,
    Fit {
        width: u32,
        height: u32,
    },
    Exact {
        width: u32,
        height: u32,
    },
    Percent(u16),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DuplicateStyle {
    #[default]
    Dash,
    Underscore,
    Parenthesized,
    Copy,
}

impl DuplicateStyle {
    fn name(self, stem: &str, number: usize) -> String {
        match self {
            Self::Dash => format!("{stem}-{number}"),
            Self::Underscore => format!("{stem}_{number}"),
            Self::Parenthesized => format!("{stem} ({number})"),
            Self::Copy if number == 2 => format!("{stem}-copy"),
            Self::Copy => format!("{stem}-copy-{}", number - 1),
        }
    }
}

impl fmt::Display for DuplicateStyle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Dash => "dash",
            Self::Underscore => "underscore",
            Self::Parenthesized => "parenthesized",
            Self::Copy => "copy",
        })
    }
}

impl FromStr for DuplicateStyle {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "dash" => Ok(Self::Dash),
            "underscore" => Ok(Self::Underscore),
            "parenthesized" => Ok(Self::Parenthesized),
            "copy" => Ok(Self::Copy),
            _ => Err("expected dash, underscore, parenthesized, or copy".to_owned()),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RgbColor {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl RgbColor {
    pub const BLACK: Self = Self::new(0, 0, 0);
    pub const WHITE: Self = Self::new(255, 255, 255);

    pub const fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }

    pub const fn components(self) -> [u8; 3] {
        [self.red, self.green, self.blue]
    }
}

impl Default for RgbColor {
    fn default() -> Self {
        Self::WHITE
    }
}

impl fmt::Display for RgbColor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "#{:02X}{:02X}{:02X}",
            self.red, self.green, self.blue
        )
    }
}

impl FromStr for RgbColor {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let digits = value
            .strip_prefix('#')
            .filter(|digits| digits.len() == 6)
            .ok_or_else(|| "color must use #RRGGBB".to_owned())?;
        let parse = |range| {
            u8::from_str_radix(&digits[range], 16)
                .map_err(|_| "color must use hexadecimal #RRGGBB".to_owned())
        };
        Ok(Self::new(parse(0..2)?, parse(2..4)?, parse(4..6)?))
    }
}

#[derive(Clone, Debug)]
pub struct ConversionRequest {
    pub inputs: Vec<PathBuf>,
    pub output_dir: Option<PathBuf>,
    pub format: OutputFormat,
    pub resize: ResizeMode,
    pub jpeg_quality: u8,
    pub jpeg_background: RgbColor,
    pub overwrite: bool,
    pub duplicate_style: DuplicateStyle,
}

impl Default for ConversionRequest {
    fn default() -> Self {
        Self {
            inputs: Vec::new(),
            output_dir: None,
            format: OutputFormat::Png,
            resize: ResizeMode::Original,
            jpeg_quality: 90,
            jpeg_background: RgbColor::WHITE,
            overwrite: false,
            duplicate_style: DuplicateStyle::Dash,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ItemId(pub usize);

#[derive(Clone, Debug)]
pub struct PlannedItem {
    pub id: ItemId,
    pub input: PathBuf,
    pub byte_size: u64,
    pub output: PathBuf,
}

#[derive(Clone, Debug)]
pub struct PlanningFailure {
    pub path: PathBuf,
    pub error: String,
}

#[derive(Clone, Debug)]
pub struct ConversionPlan {
    pub request: ConversionRequest,
    pub items: Vec<PlannedItem>,
    pub failures: Vec<PlanningFailure>,
}

#[derive(Clone, Debug, Error)]
#[error("{message}")]
pub struct PlanError {
    pub message: String,
    pub failures: Vec<PlanningFailure>,
}

#[derive(Clone, Debug)]
pub enum ConversionEvent {
    BatchStarted { total: usize },
    ItemStarted { id: ItemId },
    ItemFinished(ConversionResult),
}

#[derive(Clone, Debug)]
pub struct ConversionResult {
    pub id: ItemId,
    pub input: PathBuf,
    pub output: Option<PathBuf>,
    pub elapsed: Duration,
    pub error: Option<String>,
    pub cancelled: bool,
}

impl ConversionResult {
    pub fn succeeded(&self) -> bool {
        self.error.is_none() && !self.cancelled
    }
}

#[derive(Clone, Debug, Default)]
pub struct BatchReport {
    pub results: Vec<ConversionResult>,
    pub elapsed: Duration,
    pub cancelled: bool,
}

impl BatchReport {
    pub fn succeeded(&self) -> usize {
        self.results
            .iter()
            .filter(|result| result.succeeded())
            .count()
    }

    pub fn failed(&self) -> usize {
        self.results
            .iter()
            .filter(|result| result.error.is_some())
            .count()
    }

    pub fn cancelled_count(&self) -> usize {
        self.results
            .iter()
            .filter(|result| result.cancelled)
            .count()
    }
}

#[derive(Debug, Error)]
enum ConvertError {
    #[error("invalid resize dimensions")]
    InvalidResize,
    #[error("could not create output folder {path}: {source}")]
    CreateOutput {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("could not decode image: {0}")]
    Decode(#[from] image::ImageError),
    #[error("could not open image: {0}")]
    Open(#[from] std::io::Error),
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Converter;

impl Converter {
    /// Expands and deduplicates inputs, reads inexpensive metadata, and reserves every output.
    pub fn plan(&self, mut request: ConversionRequest) -> Result<ConversionPlan, PlanError> {
        let (inputs, failures) = discover_inputs(&request.inputs);
        if inputs.is_empty() {
            return Err(PlanError {
                message: "no supported images were found".to_owned(),
                failures,
            });
        }
        request.inputs = inputs.iter().map(|(path, _)| path.clone()).collect();
        let items = plan_jobs(&inputs, &request);
        Ok(ConversionPlan {
            request,
            items,
            failures,
        })
    }

    /// Converts a pre-reserved plan in parallel. The callback may run concurrently.
    pub fn run<F>(&self, plan: ConversionPlan, cancelled: &AtomicBool, on_event: F) -> BatchReport
    where
        F: Fn(ConversionEvent) + Sync,
    {
        let batch_started = Instant::now();
        on_event(ConversionEvent::BatchStarted {
            total: plan.items.len(),
        });
        let request = &plan.request;
        let mut results: Vec<_> = plan
            .items
            .par_iter()
            .map(|item| {
                if cancelled.load(Ordering::Relaxed) {
                    return cancelled_result(item);
                }
                on_event(ConversionEvent::ItemStarted { id: item.id });
                let result = convert_one(item, request);
                on_event(ConversionEvent::ItemFinished(result.clone()));
                result
            })
            .collect();
        results.sort_by_key(|result| result.id);
        BatchReport {
            results,
            elapsed: batch_started.elapsed(),
            cancelled: cancelled.load(Ordering::Relaxed),
        }
    }
}

fn discover_inputs(paths: &[PathBuf]) -> (Vec<(PathBuf, u64)>, Vec<PlanningFailure>) {
    let mut inputs = Vec::new();
    let mut failures = Vec::new();
    let mut seen = HashSet::new();
    for supplied in paths {
        let path = match fs::canonicalize(supplied) {
            Ok(path) => path,
            Err(error) => {
                failures.push(PlanningFailure {
                    path: supplied.clone(),
                    error: format!("input is unavailable: {error}"),
                });
                continue;
            }
        };
        if path.is_file() {
            add_candidate(path, &mut seen, &mut inputs, &mut failures);
            continue;
        }
        if !path.is_dir() {
            failures.push(PlanningFailure {
                path,
                error: "input is not a file or folder".to_owned(),
            });
            continue;
        }
        for entry in WalkDir::new(&path).follow_links(false) {
            match entry {
                Ok(entry) if entry.file_type().is_file() => {
                    let candidate = entry.into_path();
                    if is_supported_input(&candidate) {
                        add_candidate(candidate, &mut seen, &mut inputs, &mut failures);
                    }
                }
                Ok(_) => {}
                Err(error) => failures.push(PlanningFailure {
                    path: error.path().unwrap_or(&path).to_path_buf(),
                    error: format!("could not scan folder: {error}"),
                }),
            }
        }
    }
    (inputs, failures)
}

fn add_candidate(
    path: PathBuf,
    seen: &mut HashSet<PathBuf>,
    inputs: &mut Vec<(PathBuf, u64)>,
    failures: &mut Vec<PlanningFailure>,
) {
    if !is_supported_input(&path) || !seen.insert(path.clone()) {
        return;
    }
    match fs::metadata(&path) {
        Ok(metadata) => inputs.push((path, metadata.len())),
        Err(error) => failures.push(PlanningFailure {
            path,
            error: format!("could not read file metadata: {error}"),
        }),
    }
}

fn is_supported_input(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "webp" | "bmp" | "tif" | "tiff" | "gif"
            )
        })
}

fn plan_jobs(inputs: &[(PathBuf, u64)], request: &ConversionRequest) -> Vec<PlannedItem> {
    let mut reserved = HashSet::new();
    inputs
        .iter()
        .enumerate()
        .map(|(index, (input, byte_size))| {
            let output_dir = request.output_dir.clone().unwrap_or_else(|| {
                input
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .join("converted")
            });
            let stem = input
                .file_stem()
                .and_then(|stem| stem.to_str())
                .filter(|stem| !stem.is_empty())
                .unwrap_or("image");
            let extension = request.format.extension();
            let mut output = output_dir.join(format!("{stem}.{extension}"));
            if output == *input {
                output = output_dir.join(format!("{stem}-converted.{extension}"));
            }
            let mut number = 2;
            while reserved.contains(&output) || (!request.overwrite && output.exists()) {
                let name = request.duplicate_style.name(stem, number);
                output = output_dir.join(format!("{name}.{extension}"));
                number += 1;
            }
            reserved.insert(output.clone());
            PlannedItem {
                id: ItemId(index),
                input: input.clone(),
                byte_size: *byte_size,
                output,
            }
        })
        .collect()
}

fn cancelled_result(item: &PlannedItem) -> ConversionResult {
    ConversionResult {
        id: item.id,
        input: item.input.clone(),
        output: None,
        elapsed: Duration::ZERO,
        error: None,
        cancelled: true,
    }
}

fn convert_one(item: &PlannedItem, request: &ConversionRequest) -> ConversionResult {
    let started = Instant::now();
    let outcome = (|| -> Result<(), ConvertError> {
        let mut decoder = ImageReader::open(&item.input)?
            .with_guessed_format()?
            .into_decoder()?;
        let orientation = decoder.orientation()?;
        let mut image = DynamicImage::from_decoder(decoder)?;
        image.apply_orientation(orientation);
        let image = resize(image, request.resize)?;
        let output_dir = item.output.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(output_dir).map_err(|source| ConvertError::CreateOutput {
            path: output_dir.to_path_buf(),
            source,
        })?;
        encode(&image, &item.output, request)?;
        Ok(())
    })();
    ConversionResult {
        id: item.id,
        input: item.input.clone(),
        output: outcome.as_ref().ok().map(|_| item.output.clone()),
        elapsed: started.elapsed(),
        error: outcome.err().map(|error| error.to_string()),
        cancelled: false,
    }
}

fn resize(image: DynamicImage, mode: ResizeMode) -> Result<DynamicImage, ConvertError> {
    let (width, height) = image.dimensions();
    let dimensions = match mode {
        ResizeMode::Original => return Ok(image),
        ResizeMode::Exact { width, height } if width > 0 && height > 0 => (width, height),
        ResizeMode::Fit {
            width: 0,
            height: 0,
        } => return Err(ConvertError::InvalidResize),
        ResizeMode::Fit {
            width: max_width,
            height: max_height,
        } => {
            let max_width = if max_width == 0 { u32::MAX } else { max_width };
            let max_height = if max_height == 0 {
                u32::MAX
            } else {
                max_height
            };
            let scale = (max_width as f64 / width as f64).min(max_height as f64 / height as f64);
            (
                (width as f64 * scale).round().clamp(1.0, u32::MAX as f64) as u32,
                (height as f64 * scale).round().clamp(1.0, u32::MAX as f64) as u32,
            )
        }
        ResizeMode::Percent(percent) if percent > 0 => (
            ((width as u64 * percent as u64) / 100)
                .max(1)
                .min(u32::MAX as u64) as u32,
            ((height as u64 * percent as u64) / 100)
                .max(1)
                .min(u32::MAX as u64) as u32,
        ),
        _ => return Err(ConvertError::InvalidResize),
    };
    if dimensions == (width, height) {
        Ok(image)
    } else {
        Ok(image.resize_exact(
            dimensions.0,
            dimensions.1,
            image::imageops::FilterType::Triangle,
        ))
    }
}

fn composite_for_jpeg(image: &DynamicImage, background: RgbColor) -> image::RgbImage {
    let rgba: RgbaImage = image.to_rgba8();
    let bg = background.components();
    image::RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
        let pixel = rgba.get_pixel(x, y).0;
        let alpha = u16::from(pixel[3]);
        image::Rgb([
            ((u16::from(pixel[0]) * alpha + u16::from(bg[0]) * (255 - alpha) + 127) / 255) as u8,
            ((u16::from(pixel[1]) * alpha + u16::from(bg[1]) * (255 - alpha) + 127) / 255) as u8,
            ((u16::from(pixel[2]) * alpha + u16::from(bg[2]) * (255 - alpha) + 127) / 255) as u8,
        ])
    })
}

fn encode(
    image: &DynamicImage,
    output: &Path,
    request: &ConversionRequest,
) -> Result<(), ConvertError> {
    let writer = BufWriter::new(File::create(output)?);
    match request.format {
        OutputFormat::Png => {
            let pixels = image.to_rgba8();
            PngEncoder::new(writer).write_image(
                &pixels,
                pixels.width(),
                pixels.height(),
                image::ExtendedColorType::Rgba8,
            )?;
        }
        OutputFormat::Jpeg => {
            let pixels = composite_for_jpeg(image, request.jpeg_background);
            JpegEncoder::new_with_quality(writer, request.jpeg_quality.clamp(1, 100)).write_image(
                &pixels,
                pixels.width(),
                pixels.height(),
                image::ExtendedColorType::Rgb8,
            )?;
        }
        OutputFormat::WebP => {
            let pixels = image.to_rgba8();
            WebPEncoder::new_lossless(writer).write_image(
                &pixels,
                pixels.width(),
                pixels.height(),
                image::ExtendedColorType::Rgba8,
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgb, Rgba};
    use std::sync::{Mutex, atomic::AtomicBool};

    fn sample(path: &Path, width: u32, height: u32) {
        ImageBuffer::from_fn(width, height, |x, y| {
            Rgba([(x % 255) as u8, (y % 255) as u8, 100, 180])
        })
        .save(path)
        .unwrap();
    }

    fn plan_for(request: ConversionRequest) -> ConversionPlan {
        Converter.plan(request).unwrap()
    }

    #[test]
    fn planning_expands_folders_records_sizes_and_deduplicates() {
        let temp = tempfile::tempdir().unwrap();
        let nested = temp.path().join("nested");
        fs::create_dir(&nested).unwrap();
        let input = nested.join("source.png");
        sample(&input, 3, 2);
        let plan = plan_for(ConversionRequest {
            inputs: vec![temp.path().to_path_buf(), input.clone()],
            ..Default::default()
        });
        assert_eq!(plan.items.len(), 1);
        assert_eq!(plan.items[0].byte_size, fs::metadata(input).unwrap().len());
        assert!(plan.items[0].input.is_absolute());
    }

    #[test]
    fn converts_and_fits_an_image() {
        let temp = tempfile::tempdir().unwrap();
        let input = temp.path().join("source.png");
        let output = temp.path().join("out");
        sample(&input, 400, 200);
        let report = Converter.run(
            plan_for(ConversionRequest {
                inputs: vec![input],
                output_dir: Some(output.clone()),
                format: OutputFormat::Jpeg,
                resize: ResizeMode::Fit {
                    width: 100,
                    height: 100,
                },
                ..Default::default()
            }),
            &AtomicBool::new(false),
            |_| {},
        );
        assert_eq!(report.succeeded(), 1);
        let result = ImageReader::open(output.join("source.jpg"))
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(result.dimensions(), (100, 50));
    }

    #[test]
    fn every_collision_style_and_duplicate_stem_is_reserved() {
        let temp = tempfile::tempdir().unwrap();
        let a = temp.path().join("a");
        let b = temp.path().join("b");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        let first = a.join("same.png");
        let second = b.join("same.png");
        sample(&first, 2, 2);
        sample(&second, 2, 2);
        for (style, expected) in [
            (DuplicateStyle::Dash, "same-2.png"),
            (DuplicateStyle::Underscore, "same_2.png"),
            (DuplicateStyle::Parenthesized, "same (2).png"),
            (DuplicateStyle::Copy, "same-copy.png"),
        ] {
            let output = temp.path().join(style.to_string());
            let plan = plan_for(ConversionRequest {
                inputs: vec![first.clone(), second.clone()],
                output_dir: Some(output.clone()),
                duplicate_style: style,
                ..Default::default()
            });
            assert_eq!(plan.items[0].output, output.join("same.png"));
            assert_eq!(plan.items[1].output, output.join(expected));
        }
    }

    #[test]
    fn jpeg_matte_composites_transparent_partial_opaque_and_rgb_pixels() {
        let custom = RgbColor::new(20, 40, 60);
        let rgba = DynamicImage::ImageRgba8(
            ImageBuffer::from_vec(3, 1, vec![255, 0, 0, 0, 200, 100, 0, 128, 9, 8, 7, 255])
                .unwrap(),
        );
        let pixels = composite_for_jpeg(&rgba, custom);
        assert_eq!(pixels.get_pixel(0, 0), &Rgb([20, 40, 60]));
        assert_eq!(pixels.get_pixel(1, 0), &Rgb([110, 70, 30]));
        assert_eq!(pixels.get_pixel(2, 0), &Rgb([9, 8, 7]));
        let rgb = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(1, 1, Rgb([1, 2, 3])));
        assert_eq!(
            composite_for_jpeg(&rgb, RgbColor::BLACK).get_pixel(0, 0),
            &Rgb([1, 2, 3])
        );
        assert_eq!(
            composite_for_jpeg(&rgba, RgbColor::WHITE).get_pixel(0, 0),
            &Rgb([255, 255, 255])
        );
        assert_eq!(
            composite_for_jpeg(&rgba, RgbColor::BLACK).get_pixel(0, 0),
            &Rgb([0, 0, 0])
        );
    }

    #[test]
    fn png_and_webp_preserve_alpha() {
        let temp = tempfile::tempdir().unwrap();
        let input = temp.path().join("alpha.png");
        ImageBuffer::from_pixel(1, 1, Rgba([10_u8, 20, 30, 77]))
            .save(&input)
            .unwrap();
        for format in [OutputFormat::Png, OutputFormat::WebP] {
            let output = temp.path().join(format.extension());
            let report = Converter.run(
                plan_for(ConversionRequest {
                    inputs: vec![input.clone()],
                    output_dir: Some(output.clone()),
                    format,
                    ..Default::default()
                }),
                &AtomicBool::new(false),
                |_| {},
            );
            assert_eq!(report.succeeded(), 1);
            let converted = ImageReader::open(output.join(format!("alpha.{}", format.extension())))
                .unwrap()
                .decode()
                .unwrap()
                .to_rgba8();
            assert_eq!(converted.get_pixel(0, 0).0[3], 77);
        }
    }

    #[test]
    fn emits_batch_started_item_started_and_finished_with_stable_ids() {
        let temp = tempfile::tempdir().unwrap();
        let input = temp.path().join("source.png");
        sample(&input, 2, 2);
        let events = Mutex::new(Vec::new());
        Converter.run(
            plan_for(ConversionRequest {
                inputs: vec![input],
                ..Default::default()
            }),
            &AtomicBool::new(false),
            |event| events.lock().unwrap().push(event),
        );
        let events = events.into_inner().unwrap();
        assert!(matches!(
            events[0],
            ConversionEvent::BatchStarted { total: 1 }
        ));
        assert!(matches!(
            events[1],
            ConversionEvent::ItemStarted { id: ItemId(0) }
        ));
        assert!(
            matches!(events[2], ConversionEvent::ItemFinished(ref result) if result.id == ItemId(0))
        );
    }

    #[test]
    fn cancellation_returns_every_row_and_marks_unstarted_work() {
        let temp = tempfile::tempdir().unwrap();
        let input = temp.path().join("source.png");
        sample(&input, 2, 2);
        let cancelled = AtomicBool::new(true);
        let report = Converter.run(
            plan_for(ConversionRequest {
                inputs: vec![input],
                ..Default::default()
            }),
            &cancelled,
            |_| {},
        );
        assert!(report.cancelled);
        assert_eq!(report.cancelled_count(), 1);
        assert_eq!(report.results.len(), 1);
    }

    #[test]
    fn rgb_color_requires_exact_hex_form() {
        assert_eq!(
            "#12aBcF".parse::<RgbColor>().unwrap(),
            RgbColor::new(0x12, 0xab, 0xcf)
        );
        assert!("12ABCF".parse::<RgbColor>().is_err());
        assert!("#FFF".parse::<RgbColor>().is_err());
    }
}
