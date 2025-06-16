use std::time::Duration;
use serde::{Deserialize, Serialize};
use crate::process::types::{ProcessingMode, FileFormat};

/// Video extraction configuration matching extraction/config.rs
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VideoExtractionConfig {
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

/// Basic process configuration
#[derive(Debug, Clone)]
pub struct ProcessConfig {
    pub input_path: String,
    pub output_path: String,
    pub processing_options: ProcessingOptions,
    pub processing_mode: ProcessingMode,
    pub supported_formats: Vec<FileFormat>,
    pub video_config: Option<VideoExtractionConfig>,
}

/// Processing options for the process module
#[derive(Debug, Clone)]
pub struct ProcessingOptions {
    pub enable_validation: bool,
    pub verbose_logging: bool,
    pub create_output_directory: bool,
    pub overwrite_existing: bool,
    pub max_file_size_mb: Option<u64>,
    pub timeout_seconds: Option<u64>,
    pub parallel_processing: bool,
    pub backup_original: bool,
}

impl Default for ProcessingOptions {
    fn default() -> Self {
        Self {
            enable_validation: true,
            verbose_logging: false,
            create_output_directory: true,
            overwrite_existing: false,
            max_file_size_mb: Some(1024), // 1GB default limit
            timeout_seconds: Some(300),   // 5 minutes default timeout
            parallel_processing: false,
            backup_original: false,
        }
    }
} 