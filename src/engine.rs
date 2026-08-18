use std::{
    collections::HashSet,
    fs::{self, File},
    io::BufWriter,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use image::{
    DynamicImage, GenericImageView, ImageDecoder, ImageEncoder, ImageReader,
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

#[derive(Clone, Debug)]
pub struct ConversionRequest {
    pub inputs: Vec<PathBuf>,
    pub output_dir: Option<PathBuf>,
    pub format: OutputFormat,
    pub resize: ResizeMode,
    pub jpeg_quality: u8,
    pub overwrite: bool,
}

impl Default for ConversionRequest {
    fn default() -> Self {
        Self {
            inputs: Vec::new(),
            output_dir: None,
            format: OutputFormat::Png,
            resize: ResizeMode::Original,
            jpeg_quality: 90,
            overwrite: false,
        }
    }
}

#[derive(Clone, Debug)]
pub enum ConversionEvent {
    Started { total: usize },
    ItemFinished(ConversionResult),
}

#[derive(Clone, Debug)]
pub struct ConversionResult {
    pub input: PathBuf,
    pub output: Option<PathBuf>,
    pub elapsed: Duration,
    pub error: Option<String>,
}

impl ConversionResult {
    pub fn succeeded(&self) -> bool {
        self.error.is_none()
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
        self.results.len() - self.succeeded()
    }
}

#[derive(Debug, Error)]
enum ConvertError {
    #[error("input does not exist: {0}")]
    MissingInput(PathBuf),
    #[error("no supported images were found")]
    NoImages,
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
    /// Runs one batch. Files are converted in parallel and results are returned in input order.
    /// The callback may run concurrently on worker threads and must return quickly.
    pub fn run<F>(
        &self,
        request: ConversionRequest,
        cancelled: &AtomicBool,
        on_event: F,
    ) -> BatchReport
    where
        F: Fn(ConversionEvent) + Sync,
    {
        let batch_started = Instant::now();
        let inputs = match discover_inputs(&request.inputs) {
            Ok(inputs) => inputs,
            Err(error) => {
                let result = ConversionResult {
                    input: request.inputs.first().cloned().unwrap_or_default(),
                    output: None,
                    elapsed: Duration::ZERO,
                    error: Some(error.to_string()),
                };
                on_event(ConversionEvent::Started { total: 1 });
                on_event(ConversionEvent::ItemFinished(result.clone()));
                return BatchReport {
                    results: vec![result],
                    elapsed: batch_started.elapsed(),
                    cancelled: false,
                };
            }
        };

        on_event(ConversionEvent::Started {
            total: inputs.len(),
        });
        let jobs = plan_jobs(&inputs, &request);
        let mut indexed_results: Vec<_> = jobs
            .into_par_iter()
            .enumerate()
            .filter_map(|(index, (input, output))| {
                if cancelled.load(Ordering::Relaxed) {
                    return None;
                }

                let result = convert_one(&input, &output, &request);
                on_event(ConversionEvent::ItemFinished(result.clone()));
                Some((index, result))
            })
            .collect();
        indexed_results.sort_by_key(|(index, _)| *index);

        BatchReport {
            results: indexed_results
                .into_iter()
                .map(|(_, result)| result)
                .collect(),
            elapsed: batch_started.elapsed(),
            cancelled: cancelled.load(Ordering::Relaxed),
        }
    }
}

fn discover_inputs(paths: &[PathBuf]) -> Result<Vec<PathBuf>, ConvertError> {
    let mut inputs = Vec::new();
    let mut seen = HashSet::new();

    for path in paths {
        if !path.exists() {
            return Err(ConvertError::MissingInput(path.clone()));
        }
        if path.is_file() {
            if seen.insert(path.clone()) {
                inputs.push(path.clone());
            }
            continue;
        }

        for entry in WalkDir::new(path)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
        {
            let candidate = entry.into_path();
            if is_supported_input(&candidate) && seen.insert(candidate.clone()) {
                inputs.push(candidate);
            }
        }
    }

    if inputs.is_empty() {
        Err(ConvertError::NoImages)
    } else {
        Ok(inputs)
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

fn plan_jobs(inputs: &[PathBuf], request: &ConversionRequest) -> Vec<(PathBuf, PathBuf)> {
    let mut reserved = HashSet::new();
    inputs
        .iter()
        .map(|input| {
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
            let mut output = output_dir.join(format!("{stem}.{}", request.format.extension()));
            let mut suffix = 2;

            if output == *input {
                output =
                    output_dir.join(format!("{stem}-converted.{}", request.format.extension()));
            }
            while reserved.contains(&output) || (!request.overwrite && output.exists()) {
                output = output_dir.join(format!("{stem}-{suffix}.{}", request.format.extension()));
                suffix += 1;
            }
            reserved.insert(output.clone());
            (input.clone(), output)
        })
        .collect()
}

fn convert_one(input: &Path, output: &Path, request: &ConversionRequest) -> ConversionResult {
    let started = Instant::now();
    let outcome = (|| -> Result<(), ConvertError> {
        let mut decoder = ImageReader::open(input)?
            .with_guessed_format()?
            .into_decoder()?;
        let orientation = decoder.orientation()?;
        let mut image = DynamicImage::from_decoder(decoder)?;
        image.apply_orientation(orientation);
        let image = resize(image, request.resize)?;

        let output_dir = output.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(output_dir).map_err(|source| ConvertError::CreateOutput {
            path: output_dir.to_path_buf(),
            source,
        })?;
        encode(&image, output, request.format, request.jpeg_quality)?;
        Ok(())
    })();

    ConversionResult {
        input: input.to_path_buf(),
        output: outcome.as_ref().ok().map(|_| output.to_path_buf()),
        elapsed: started.elapsed(),
        error: outcome.err().map(|error| error.to_string()),
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

fn encode(
    image: &DynamicImage,
    output: &Path,
    format: OutputFormat,
    jpeg_quality: u8,
) -> Result<(), ConvertError> {
    let writer = BufWriter::new(File::create(output)?);
    match format {
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
            let pixels = image.to_rgb8();
            JpegEncoder::new_with_quality(writer, jpeg_quality.clamp(1, 100)).write_image(
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
    use image::{ImageBuffer, Rgba};
    use std::sync::atomic::AtomicBool;

    fn sample(path: &Path, width: u32, height: u32) {
        ImageBuffer::from_fn(width, height, |x, y| {
            Rgba([(x % 255) as u8, (y % 255) as u8, 100, 180])
        })
        .save(path)
        .unwrap();
    }

    #[test]
    fn converts_and_fits_an_image() {
        let temp = tempfile::tempdir().unwrap();
        let input = temp.path().join("source.png");
        let output = temp.path().join("out");
        sample(&input, 400, 200);

        let report = Converter.run(
            ConversionRequest {
                inputs: vec![input],
                output_dir: Some(output.clone()),
                format: OutputFormat::Jpeg,
                resize: ResizeMode::Fit {
                    width: 100,
                    height: 100,
                },
                ..Default::default()
            },
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
    fn creates_collision_safe_names() {
        let temp = tempfile::tempdir().unwrap();
        let a = temp.path().join("a");
        let b = temp.path().join("b");
        let output = temp.path().join("out");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        let first = a.join("same.png");
        let second = b.join("same.png");
        sample(&first, 2, 2);
        sample(&second, 2, 2);

        let jobs = plan_jobs(
            &[first, second],
            &ConversionRequest {
                output_dir: Some(output.clone()),
                ..Default::default()
            },
        );

        assert_eq!(jobs[0].1, output.join("same.png"));
        assert_eq!(jobs[1].1, output.join("same-2.png"));
    }

    #[test]
    fn writes_every_output_format() {
        let temp = tempfile::tempdir().unwrap();
        let input = temp.path().join("source.png");
        sample(&input, 7, 5);

        for format in [OutputFormat::Png, OutputFormat::Jpeg, OutputFormat::WebP] {
            let output = temp.path().join(format.extension());
            let report = Converter.run(
                ConversionRequest {
                    inputs: vec![input.clone()],
                    output_dir: Some(output.clone()),
                    format,
                    ..Default::default()
                },
                &AtomicBool::new(false),
                |_| {},
            );

            assert_eq!(report.succeeded(), 1, "failed to write {format:?}");
            let converted =
                ImageReader::open(output.join(format!("source.{}", format.extension())))
                    .unwrap()
                    .decode()
                    .unwrap();
            assert_eq!(converted.dimensions(), (7, 5));
        }
    }

    #[test]
    fn cancellation_stops_before_work_starts() {
        let temp = tempfile::tempdir().unwrap();
        let input = temp.path().join("source.png");
        sample(&input, 2, 2);
        let cancelled = AtomicBool::new(true);

        let report = Converter.run(
            ConversionRequest {
                inputs: vec![input],
                ..Default::default()
            },
            &cancelled,
            |_| {},
        );

        assert!(report.cancelled);
        assert!(report.results.is_empty());
    }
}
