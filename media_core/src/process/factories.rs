//! Convenience functions for creating processors with common configurations

use crate::process::types::{ProcessError, ProcessingMode, FileFormat, VideoFormat, get_default_supported_formats};
use crate::process::config::{ProcessConfig, ProcessingOptions};
use crate::process::processor::Processor;

/// Convenience function to create a processor with default options
pub fn create_processor(input_path: String, output_path: String) -> Result<Processor, ProcessError> {
    let config = ProcessConfig {
        input_path,
        output_path,
        processing_options: ProcessingOptions::default(),
        processing_mode: ProcessingMode::SingleFile,
        supported_formats: get_default_supported_formats(),
        video_config: None,
    };
    
    Processor::new(config)
}

/// Convenience function to create a processor with custom options
pub fn create_processor_with_options(
    input_path: String, 
    output_path: String, 
    options: ProcessingOptions
) -> Result<Processor, ProcessError> {
    let config = ProcessConfig {
        input_path,
        output_path,
        processing_options: options,
        processing_mode: ProcessingMode::SingleFile,
        supported_formats: get_default_supported_formats(),
        video_config: None,
    };
    
    Processor::new(config)
}

/// Convenience function to create a processor with custom mode
pub fn create_processor_with_mode(
    input_path: String,
    output_path: String,
    mode: ProcessingMode,
) -> Result<Processor, ProcessError> {
    let config = ProcessConfig {
        input_path,
        output_path,
        processing_options: ProcessingOptions::default(),
        processing_mode: mode,
        supported_formats: get_default_supported_formats(),
        video_config: None,
    };
    
    Processor::new(config)
}

/// Convenience function to create a processor for video extraction
pub fn create_video_processor() -> Result<Processor, ProcessError> {
    // Disable validation for video processors since they get paths from video config
    let mut options = ProcessingOptions::default();
    options.enable_validation = false;
    
    let config = ProcessConfig {
        input_path: "".to_string(), // Will be set by video config
        output_path: "".to_string(), // Will be set by video config
        processing_options: options,
        processing_mode: ProcessingMode::DirectoryProcess,
        supported_formats: vec![
            FileFormat::Video(VideoFormat::Mp4),
            FileFormat::Video(VideoFormat::Avi),
            FileFormat::Video(VideoFormat::Mkv),
            FileFormat::Video(VideoFormat::Mov),
        ],
        video_config: None,
    };
    
    Processor::new(config)
} 