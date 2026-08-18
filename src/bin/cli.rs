use std::{path::PathBuf, process::ExitCode, sync::atomic::AtomicBool};

use clap::{Parser, ValueEnum};
use image_converter::{ConversionRequest, Converter, OutputFormat, ResizeMode};

#[derive(Clone, Copy, Debug, ValueEnum)]
enum FormatArg {
    Png,
    Jpeg,
    Webp,
}

impl From<FormatArg> for OutputFormat {
    fn from(value: FormatArg) -> Self {
        match value {
            FormatArg::Png => Self::Png,
            FormatArg::Jpeg => Self::Jpeg,
            FormatArg::Webp => Self::WebP,
        }
    }
}

#[derive(Debug, Parser)]
#[command(
    name = "image-converter",
    version,
    about = "Convert images quickly using all available CPU cores"
)]
struct Cli {
    /// Image files or folders. Folders are scanned recursively.
    #[arg(required = true)]
    inputs: Vec<PathBuf>,

    /// Output image format.
    #[arg(short, long, value_enum, default_value = "png")]
    format: FormatArg,

    /// Place every output in this folder. Defaults to a converted folder beside each input.
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Target or maximum width in pixels.
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
    width: Option<u32>,

    /// Target or maximum height in pixels.
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
    height: Option<u32>,

    /// Stretch to the exact width and height instead of preserving aspect ratio.
    #[arg(long, requires_all = ["width", "height"], conflicts_with = "percent")]
    exact: bool,

    /// Resize by a percentage.
    #[arg(long, conflicts_with_all = ["width", "height"], value_parser = clap::value_parser!(u16).range(1..))]
    percent: Option<u16>,

    /// JPEG quality from 1 to 100. WebP output is lossless.
    #[arg(long, default_value_t = 90, value_parser = clap::value_parser!(u8).range(1..=100))]
    quality: u8,

    /// Replace existing output names instead of adding a numeric suffix.
    #[arg(long)]
    overwrite: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let resize = if let Some(percent) = cli.percent {
        ResizeMode::Percent(percent)
    } else if cli.exact {
        ResizeMode::Exact {
            width: cli.width.expect("clap requires width"),
            height: cli.height.expect("clap requires height"),
        }
    } else if cli.width.is_some() || cli.height.is_some() {
        ResizeMode::Fit {
            width: cli.width.unwrap_or(0),
            height: cli.height.unwrap_or(0),
        }
    } else {
        ResizeMode::Original
    };

    let report = Converter.run(
        ConversionRequest {
            inputs: cli.inputs,
            output_dir: cli.output,
            format: cli.format.into(),
            resize,
            jpeg_quality: cli.quality,
            overwrite: cli.overwrite,
        },
        &AtomicBool::new(false),
        |event| {
            if let image_converter::ConversionEvent::ItemFinished(result) = event {
                if let Some(output) = result.output {
                    println!("{} -> {}", result.input.display(), output.display());
                } else if let Some(error) = result.error {
                    eprintln!("{}: {error}", result.input.display());
                }
            }
        },
    );

    println!(
        "{} converted, {} failed in {:.2?}",
        report.succeeded(),
        report.failed(),
        report.elapsed
    );
    if report.failed() == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
