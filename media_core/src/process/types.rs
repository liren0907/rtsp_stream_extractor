use std::fmt;
use std::error::Error;
use serde::{Deserialize, Serialize};

/// Process module error types
#[derive(Debug)]
pub enum ProcessError {
    InvalidInput(String),
    ProcessingFailed(String),
    IoError(String),
    ConfigurationError(String),
    ValidationError(String),
}

impl fmt::Display for ProcessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProcessError::InvalidInput(msg) => write!(f, "Invalid input: {}", msg),
            ProcessError::ProcessingFailed(msg) => write!(f, "Processing failed: {}", msg),
            ProcessError::IoError(msg) => write!(f, "IO error: {}", msg),
            ProcessError::ConfigurationError(msg) => write!(f, "Configuration error: {}", msg),
            ProcessError::ValidationError(msg) => write!(f, "Validation error: {}", msg),
        }
    }
}

impl Error for ProcessError {}

/// Processing modes available in the process module
#[derive(Debug, Clone, PartialEq)]
pub enum ProcessingMode {
    SingleFile,
    BatchFiles,
    DirectoryProcess,
    StreamProcess,
}

/// File format types supported by the processor
#[derive(Debug, Clone, PartialEq)]
pub enum FileFormat {
    Video(VideoFormat),
    Audio(AudioFormat),
    Image(ImageFormat),
    Document(DocumentFormat),
}

#[derive(Debug, Clone, PartialEq)]
pub enum VideoFormat {
    Mp4,
    Avi,
    Mkv,
    Mov,
    Webm,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AudioFormat {
    Mp3,
    Wav,
    Flac,
    Aac,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ImageFormat {
    Jpg,
    Png,
    Gif,
    Bmp,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DocumentFormat {
    Txt,
    Json,
    Xml,
    Csv,
}

/// Get default supported file formats
pub fn get_default_supported_formats() -> Vec<FileFormat> {
    vec![
        // Video formats
        FileFormat::Video(VideoFormat::Mp4),
        FileFormat::Video(VideoFormat::Avi),
        FileFormat::Video(VideoFormat::Mkv),
        FileFormat::Video(VideoFormat::Mov),
        FileFormat::Video(VideoFormat::Webm),
        // Audio formats
        FileFormat::Audio(AudioFormat::Mp3),
        FileFormat::Audio(AudioFormat::Wav),
        FileFormat::Audio(AudioFormat::Flac),
        FileFormat::Audio(AudioFormat::Aac),
        // Image formats
        FileFormat::Image(ImageFormat::Jpg),
        FileFormat::Image(ImageFormat::Png),
        FileFormat::Image(ImageFormat::Gif),
        FileFormat::Image(ImageFormat::Bmp),
        // Document formats
        FileFormat::Document(DocumentFormat::Txt),
        FileFormat::Document(DocumentFormat::Json),
        FileFormat::Document(DocumentFormat::Xml),
        FileFormat::Document(DocumentFormat::Csv),
    ]
} 