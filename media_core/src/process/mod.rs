//! Process Module
//! 
//! This module provides process-based functionality for media processing.
//! It operates independently from the RTSP module and follows a modular
//! approach for better maintainability and organization.

// Module declarations
pub mod types;
pub mod config;
pub mod stats;
pub mod processor;
pub mod video;
pub mod factories;
pub mod hw_accel;

#[cfg(test)]
mod tests;

// Re-export commonly used items for convenience
pub use types::{
    ProcessError, ProcessingMode, FileFormat, VideoFormat, AudioFormat, 
    ImageFormat, DocumentFormat, get_default_supported_formats
};
pub use config::{ProcessConfig, ProcessingOptions, VideoExtractionConfig};
pub use stats::ProcessingStats;
pub use processor::Processor;
pub use video::VideoProcessor;
pub use factories::{
    create_processor, create_processor_with_options, 
    create_processor_with_mode, create_video_processor
}; 
pub use hw_accel::{HardwareAccelConfig, HardwareAcceleratedCapture}; 