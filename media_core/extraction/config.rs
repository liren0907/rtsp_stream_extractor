use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub input_directories: Vec<String>,
    pub output_directory: String,
    pub output_prefix: String,
    pub num_threads: Option<usize>,
    pub output_fps: i32,
    pub frame_interval: usize,
    pub extraction_mode: String,
    pub create_summary_per_thread: Option<bool>,
    pub video_creation_mode: Option<String>,
    pub processing_mode: Option<String>,
} 