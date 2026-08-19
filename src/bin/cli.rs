use std::{path::PathBuf, process::ExitCode, sync::atomic::AtomicBool};

use clap::{Parser, ValueEnum};
use image_converter::{
    ConversionEvent, ConversionRequest, Converter, DuplicateStyle, OutputFormat, ResizeMode,
    RgbColor,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum DuplicateStyleArg {
    Dash,
    Underscore,
    Parenthesized,
    Copy,
}

impl From<DuplicateStyleArg> for DuplicateStyle {
    fn from(value: DuplicateStyleArg) -> Self {
        match value {
            DuplicateStyleArg::Dash => Self::Dash,
            DuplicateStyleArg::Underscore => Self::Underscore,
            DuplicateStyleArg::Parenthesized => Self::Parenthesized,
            DuplicateStyleArg::Copy => Self::Copy,
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

    /// JPEG matte used for transparent pixels, in #RRGGBB form.
    #[arg(long, value_name = "#RRGGBB")]
    background: Option<RgbColor>,

    /// Collision naming: dash, underscore, parenthesized, or copy.
    #[arg(long, value_enum, default_value = "dash")]
    duplicate_style: DuplicateStyleArg,

    /// Replace existing output names instead of adding a suffix.
    #[arg(long)]
    overwrite: bool,
}

impl Cli {
    fn into_request(self) -> Result<ConversionRequest, String> {
        if self.background.is_some() && self.format != FormatArg::Jpeg {
            return Err("--background is only meaningful with --format jpeg".to_owned());
        }
        let resize = if let Some(percent) = self.percent {
            ResizeMode::Percent(percent)
        } else if self.exact {
            ResizeMode::Exact {
                width: self.width.expect("clap requires width"),
                height: self.height.expect("clap requires height"),
            }
        } else if self.width.is_some() || self.height.is_some() {
            ResizeMode::Fit {
                width: self.width.unwrap_or(0),
                height: self.height.unwrap_or(0),
            }
        } else {
            ResizeMode::Original
        };
        Ok(ConversionRequest {
            inputs: self.inputs,
            output_dir: self.output,
            format: self.format.into(),
            resize,
            jpeg_quality: self.quality,
            jpeg_background: self.background.unwrap_or_default(),
            overwrite: self.overwrite,
            duplicate_style: self.duplicate_style.into(),
        })
    }
}

fn main() -> ExitCode {
    let request = match Cli::parse().into_request() {
        Ok(request) => request,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::from(2);
        }
    };
    let plan = match Converter.plan(request) {
        Ok(plan) => plan,
        Err(error) => {
            eprintln!("error: {error}");
            for failure in error.failures {
                eprintln!("{}: {}", failure.path.display(), failure.error);
            }
            return ExitCode::FAILURE;
        }
    };
    for failure in &plan.failures {
        eprintln!("{}: {}", failure.path.display(), failure.error);
    }
    let report = Converter.run(plan, &AtomicBool::new(false), |event| {
        if let ConversionEvent::ItemFinished(result) = event {
            if let Some(output) = result.output {
                println!("{} -> {}", result.input.display(), output.display());
            } else if let Some(error) = result.error {
                eprintln!("{}: {error}", result.input.display());
            }
        }
    });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_jpeg_background_and_duplicate_style() {
        let cli = Cli::try_parse_from([
            "image-converter",
            "input.png",
            "--format",
            "jpeg",
            "--background",
            "#102030",
            "--duplicate-style",
            "copy",
        ])
        .unwrap();
        let request = cli.into_request().unwrap();
        assert_eq!(request.jpeg_background, RgbColor::new(0x10, 0x20, 0x30));
        assert_eq!(request.duplicate_style, DuplicateStyle::Copy);
    }

    #[test]
    fn rejects_background_for_non_jpeg_output() {
        let cli = Cli::try_parse_from([
            "image-converter",
            "input.png",
            "--format",
            "png",
            "--background",
            "#FFFFFF",
        ])
        .unwrap();
        assert!(cli.into_request().unwrap_err().contains("only meaningful"));
    }

    #[test]
    fn rejects_invalid_background_and_naming_style() {
        assert!(
            Cli::try_parse_from([
                "image-converter",
                "input.png",
                "--format",
                "jpeg",
                "--background",
                "white",
            ])
            .is_err()
        );
        assert!(
            Cli::try_parse_from([
                "image-converter",
                "input.png",
                "--duplicate-style",
                "numbers",
            ])
            .is_err()
        );
    }
}
