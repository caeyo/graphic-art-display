use std::path::{Path, PathBuf};

use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "art-display", about = "GPU art display host")]
pub struct Args {
    /// Run borderless fullscreen (kiosk mode). Default is a resizable window.
    #[arg(long)]
    pub fullscreen: bool,

    /// Path to the compute shader source file.
    #[arg(long)]
    pub shader: Option<PathBuf>,

    /// Override render resolution (widthxheight).
    #[arg(long, default_value = "1280x720")]
    pub resolution: String,
}

impl Args {
    pub fn resolution(&self) -> (u32, u32) {
        let (w, h) = self
            .resolution
            .split_once('x')
            .unwrap_or(("1280", "720"));
        (w.parse().unwrap_or(1280), h.parse().unwrap_or(720))
    }

    pub fn compute_shader(&self) -> PathBuf {
        if let Some(path) = &self.shader {
            return path.clone();
        }

        if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
            let default = Path::new(&manifest_dir).join("../modules/mandelbrot/mandelbrot.comp");
            if default.exists() {
                return default;
            }
        }

        PathBuf::from("modules/mandelbrot/mandelbrot.comp")
    }
}
