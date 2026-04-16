#![allow(dead_code)]

pub mod command_builder;
pub mod encoder;
pub mod ffmpeg;

use std::path::PathBuf;
use std::process::Child;

pub struct ConversionState {
    pub ffmpeg_path: PathBuf,
    pub encoder: String,
    pub encoder_info: String,
    pub is_processing: bool,
    pub stop_requested: bool,
    pub current_process: Option<Child>,
}

impl ConversionState {
    pub fn cancel(&mut self) {
        if let Some(mut child) = self.current_process.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
