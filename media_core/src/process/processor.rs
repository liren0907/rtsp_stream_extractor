//! Core processor functionality for file processing operations

use std::path::Path;
use std::fs;
use std::time::Duration;

use crate::process::types::{ProcessError, ProcessingMode, FileFormat, VideoFormat, AudioFormat, ImageFormat, DocumentFormat};
use crate::process::config::ProcessConfig;
use crate::process::stats::ProcessingStats;
use crate::process::video::VideoProcessor;

/// Main processor struct for handling process operations
pub struct Processor {
    config: ProcessConfig,
    stats: ProcessingStats,
}

impl Processor {
    /// Create a new processor with the given configuration
    pub fn new(config: ProcessConfig) -> Result<Self, ProcessError> {
        // Basic validation (only if validation is enabled)
        if config.processing_options.enable_validation {
            if config.input_path.is_empty() {
                return Err(ProcessError::InvalidInput("Input path cannot be empty".to_string()));
            }
            
            if config.output_path.is_empty() {
                return Err(ProcessError::InvalidInput("Output path cannot be empty".to_string()));
            }
        }

        // Validate processing mode compatibility
        Self::validate_processing_mode(&config)?;

        Ok(Self { 
            config,
            stats: ProcessingStats::new(),
        })
    }

    /// Validate processing mode and configuration compatibility
    fn validate_processing_mode(config: &ProcessConfig) -> Result<(), ProcessError> {
        // Skip validation if validation is disabled (useful for tests)
        if !config.processing_options.enable_validation {
            return Ok(());
        }

        match config.processing_mode {
            ProcessingMode::DirectoryProcess => {
                let path = Path::new(&config.input_path);
                if path.exists() && !path.is_dir() {
                    return Err(ProcessError::ConfigurationError(
                        "Directory processing mode requires input path to be a directory".to_string()
                    ));
                }
            },
            ProcessingMode::SingleFile => {
                let path = Path::new(&config.input_path);
                if path.exists() && !path.is_file() {
                    return Err(ProcessError::ConfigurationError(
                        "Single file processing mode requires input path to be a file".to_string()
                    ));
                }
            },
            _ => {} // Other modes are flexible
        }
        Ok(())
    }

    /// Process from source to destination
    pub fn process_from_source(&mut self, input_path: &str, output_path: &str) -> Result<(), ProcessError> {
        if self.config.processing_options.verbose_logging {
            println!("Starting process from {} to {}", input_path, output_path);
        }

        // Reset stats for new processing session
        self.stats = ProcessingStats::new();

        // Basic validation
        self.validate_input(input_path)?;
        self.validate_output_path(output_path)?;

        // Create output directory if needed
        if self.config.processing_options.create_output_directory {
            self.ensure_output_directory(output_path)?;
        }

        // Process based on mode
        match self.config.processing_mode {
            ProcessingMode::SingleFile => self.process_single_file(input_path, output_path)?,
            ProcessingMode::BatchFiles => self.process_batch_files(input_path, output_path)?,
            ProcessingMode::DirectoryProcess => self.process_directory(input_path, output_path)?,
            ProcessingMode::StreamProcess => self.process_stream_data(input_path, output_path)?,
        }

        // Finalize stats
        self.stats.finalize();

        if self.config.processing_options.verbose_logging {
            println!("Process completed successfully");
            println!("Files processed: {}", self.stats.files_processed);
            println!("Files failed: {}", self.stats.files_failed);
            println!("Success rate: {:.2}%", self.stats.success_rate());
            println!("Processing time: {:?}", self.stats.processing_time);
        }

        Ok(())
    }

    /// Process a single file
    fn process_single_file(&mut self, input_path: &str, output_path: &str) -> Result<(), ProcessError> {
        let input_file = Path::new(input_path);
        let output_file = Path::new(output_path);

        // Check file size limits
        if let Some(max_size_mb) = self.config.processing_options.max_file_size_mb {
            let file_size = fs::metadata(input_file)
                .map_err(|e| ProcessError::IoError(format!("Failed to get file metadata: {}", e)))?
                .len();
            
            let max_size_bytes = max_size_mb * 1024 * 1024;
            if file_size > max_size_bytes {
                return Err(ProcessError::ValidationError(
                    format!("File size ({} bytes) exceeds maximum allowed size ({} MB)", 
                           file_size, max_size_mb)
                ));
            }
        }

        // Backup original if requested
        if self.config.processing_options.backup_original {
            self.backup_file(input_file)?;
        }

        // Determine file format and process accordingly
        let file_format = self.detect_file_format(input_file)?;
        self.process_file_by_format(input_file, output_file, &file_format)?;

        // Update stats
        let file_size = fs::metadata(input_file)
            .map_err(|e| ProcessError::IoError(format!("Failed to get file size: {}", e)))?
            .len();
        self.stats.add_processed_file(file_size);

        Ok(())
    }

    /// Process multiple files in batch
    fn process_batch_files(&mut self, input_path: &str, output_path: &str) -> Result<(), ProcessError> {
        // For batch processing, input_path should contain file patterns or list
        // This is a simplified implementation
        let input_dir = Path::new(input_path);
        let output_dir = Path::new(output_path);

        if !input_dir.is_dir() {
            return Err(ProcessError::InvalidInput("Batch processing requires input directory".to_string()));
        }

        let entries = fs::read_dir(input_dir)
            .map_err(|e| ProcessError::IoError(format!("Failed to read directory: {}", e)))?;

        for entry in entries {
            let entry = entry.map_err(|e| ProcessError::IoError(format!("Failed to read entry: {}", e)))?;
            let path = entry.path();

            if path.is_file() {
                let file_name = path.file_name()
                    .ok_or_else(|| ProcessError::ProcessingFailed("Invalid file name".to_string()))?;
                let output_file = output_dir.join(file_name);

                match self.process_single_file(
                    path.to_str().unwrap_or(""),
                    output_file.to_str().unwrap_or("")
                ) {
                    Ok(_) => {
                        if self.config.processing_options.verbose_logging {
                            println!("Successfully processed: {:?}", path);
                        }
                    },
                    Err(e) => {
                        let error_msg = format!("Failed to process {:?}: {}", path, e);
                        self.stats.add_failed_file(error_msg.clone());
                        if self.config.processing_options.verbose_logging {
                            eprintln!("{}", error_msg);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Process entire directory recursively
    fn process_directory(&mut self, input_path: &str, output_path: &str) -> Result<(), ProcessError> {
        let input_dir = Path::new(input_path);
        let output_dir = Path::new(output_path);

        self.process_directory_recursive(input_dir, output_dir, input_dir)?;
        Ok(())
    }

    /// Recursive directory processing helper
    fn process_directory_recursive(&mut self, current_dir: &Path, output_base: &Path, input_base: &Path) -> Result<(), ProcessError> {
        let entries = fs::read_dir(current_dir)
            .map_err(|e| ProcessError::IoError(format!("Failed to read directory: {}", e)))?;

        for entry in entries {
            let entry = entry.map_err(|e| ProcessError::IoError(format!("Failed to read entry: {}", e)))?;
            let path = entry.path();

            if path.is_dir() {
                // Recursively process subdirectories
                self.process_directory_recursive(&path, output_base, input_base)?;
            } else if path.is_file() {
                // Calculate relative path and create corresponding output path
                let relative_path = path.strip_prefix(input_base)
                    .map_err(|e| ProcessError::ProcessingFailed(format!("Failed to calculate relative path: {}", e)))?;
                let output_file = output_base.join(relative_path);

                // Ensure output directory exists
                if let Some(parent) = output_file.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|e| ProcessError::IoError(format!("Failed to create output directory: {}", e)))?;
                }

                // Process the file
                match self.process_single_file(
                    path.to_str().unwrap_or(""),
                    output_file.to_str().unwrap_or("")
                ) {
                    Ok(_) => {
                        if self.config.processing_options.verbose_logging {
                            println!("Successfully processed: {:?}", path);
                        }
                    },
                    Err(e) => {
                        let error_msg = format!("Failed to process {:?}: {}", path, e);
                        self.stats.add_failed_file(error_msg.clone());
                        if self.config.processing_options.verbose_logging {
                            eprintln!("{}", error_msg);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Process stream data (placeholder for stream processing)
    fn process_stream_data(&mut self, input_path: &str, output_path: &str) -> Result<(), ProcessError> {
        if self.config.processing_options.verbose_logging {
            println!("Processing stream data from {} to {}", input_path, output_path);
        }

        // Placeholder implementation for stream processing
        // This would be expanded based on specific stream processing requirements
        
        // Simulate stream processing
        std::thread::sleep(Duration::from_millis(100));
        
        self.stats.add_processed_file(0); // Stream data doesn't have traditional file size
        
        Ok(())
    }

    /// Detect file format based on extension and content
    pub fn detect_file_format(&self, file_path: &Path) -> Result<FileFormat, ProcessError> {
        let extension = file_path.extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_lowercase();

        match extension.as_str() {
            "mp4" | "avi" | "mkv" | "mov" | "webm" => {
                match extension.as_str() {
                    "mp4" => Ok(FileFormat::Video(VideoFormat::Mp4)),
                    "avi" => Ok(FileFormat::Video(VideoFormat::Avi)),
                    "mkv" => Ok(FileFormat::Video(VideoFormat::Mkv)),
                    "mov" => Ok(FileFormat::Video(VideoFormat::Mov)),
                    "webm" => Ok(FileFormat::Video(VideoFormat::Webm)),
                    _ => unreachable!(),
                }
            },
            "mp3" | "wav" | "flac" | "aac" => {
                match extension.as_str() {
                    "mp3" => Ok(FileFormat::Audio(AudioFormat::Mp3)),
                    "wav" => Ok(FileFormat::Audio(AudioFormat::Wav)),
                    "flac" => Ok(FileFormat::Audio(AudioFormat::Flac)),
                    "aac" => Ok(FileFormat::Audio(AudioFormat::Aac)),
                    _ => unreachable!(),
                }
            },
            "jpg" | "jpeg" | "png" | "gif" | "bmp" => {
                match extension.as_str() {
                    "jpg" | "jpeg" => Ok(FileFormat::Image(ImageFormat::Jpg)),
                    "png" => Ok(FileFormat::Image(ImageFormat::Png)),
                    "gif" => Ok(FileFormat::Image(ImageFormat::Gif)),
                    "bmp" => Ok(FileFormat::Image(ImageFormat::Bmp)),
                    _ => unreachable!(),
                }
            },
            "txt" | "json" | "xml" | "csv" => {
                match extension.as_str() {
                    "txt" => Ok(FileFormat::Document(DocumentFormat::Txt)),
                    "json" => Ok(FileFormat::Document(DocumentFormat::Json)),
                    "xml" => Ok(FileFormat::Document(DocumentFormat::Xml)),
                    "csv" => Ok(FileFormat::Document(DocumentFormat::Csv)),
                    _ => unreachable!(),
                }
            },
            _ => Err(ProcessError::ProcessingFailed(format!("Unsupported file format: {}", extension))),
        }
    }

    /// Process file based on its detected format
    fn process_file_by_format(&mut self, input_file: &Path, output_file: &Path, format: &FileFormat) -> Result<(), ProcessError> {
        match format {
            FileFormat::Video(_) => self.process_video_file(input_file, output_file),
            FileFormat::Audio(_) => self.process_audio_file(input_file, output_file),
            FileFormat::Image(_) => self.process_image_file(input_file, output_file),
            FileFormat::Document(_) => self.process_document_file(input_file, output_file),
        }
    }

    /// Process video files
    fn process_video_file(&self, input_file: &Path, output_file: &Path) -> Result<(), ProcessError> {
        if self.config.processing_options.verbose_logging {
            println!("Processing video file: {:?} -> {:?}", input_file, output_file);
        }

        // For now, this is a simple copy operation
        // In a real implementation, this could involve video transcoding, compression, etc.
        fs::copy(input_file, output_file)
            .map_err(|e| ProcessError::IoError(format!("Failed to copy video file: {}", e)))?;

        Ok(())
    }

    /// Process audio files
    fn process_audio_file(&self, input_file: &Path, output_file: &Path) -> Result<(), ProcessError> {
        if self.config.processing_options.verbose_logging {
            println!("Processing audio file: {:?} -> {:?}", input_file, output_file);
        }

        // Simple copy operation - could be enhanced with audio processing
        fs::copy(input_file, output_file)
            .map_err(|e| ProcessError::IoError(format!("Failed to copy audio file: {}", e)))?;

        Ok(())
    }

    /// Process image files
    fn process_image_file(&self, input_file: &Path, output_file: &Path) -> Result<(), ProcessError> {
        if self.config.processing_options.verbose_logging {
            println!("Processing image file: {:?} -> {:?}", input_file, output_file);
        }

        // Simple copy operation - could be enhanced with image processing
        fs::copy(input_file, output_file)
            .map_err(|e| ProcessError::IoError(format!("Failed to copy image file: {}", e)))?;

        Ok(())
    }

    /// Process document files
    fn process_document_file(&self, input_file: &Path, output_file: &Path) -> Result<(), ProcessError> {
        if self.config.processing_options.verbose_logging {
            println!("Processing document file: {:?} -> {:?}", input_file, output_file);
        }

        // Simple copy operation - could be enhanced with document processing
        fs::copy(input_file, output_file)
            .map_err(|e| ProcessError::IoError(format!("Failed to copy document file: {}", e)))?;

        Ok(())
    }

    /// Backup original file
    fn backup_file(&self, file_path: &Path) -> Result<(), ProcessError> {
        let backup_path = file_path.with_extension(
            format!("{}.backup", 
                   file_path.extension()
                           .and_then(|ext| ext.to_str())
                           .unwrap_or(""))
        );

        fs::copy(file_path, backup_path)
            .map_err(|e| ProcessError::IoError(format!("Failed to create backup: {}", e)))?;

        Ok(())
    }

    /// Ensure output directory exists
    fn ensure_output_directory(&self, output_path: &str) -> Result<(), ProcessError> {
        let path = Path::new(output_path);
        let dir = if path.is_dir() {
            path
        } else {
            path.parent().unwrap_or(Path::new("."))
        };

        fs::create_dir_all(dir)
            .map_err(|e| ProcessError::IoError(format!("Failed to create output directory: {}", e)))?;

        Ok(())
    }

    /// Validate input path
    fn validate_input(&self, input_path: &str) -> Result<(), ProcessError> {
        if !self.config.processing_options.enable_validation {
            return Ok(());
        }

        if input_path.is_empty() {
            return Err(ProcessError::InvalidInput("Input path is empty".to_string()));
        }

        let path = Path::new(input_path);
        if !path.exists() {
            return Err(ProcessError::InvalidInput(format!("Input path does not exist: {}", input_path)));
        }

        Ok(())
    }

    /// Validate output path
    fn validate_output_path(&self, output_path: &str) -> Result<(), ProcessError> {
        if !self.config.processing_options.enable_validation {
            return Ok(());
        }

        if output_path.is_empty() {
            return Err(ProcessError::InvalidInput("Output path is empty".to_string()));
        }

        let path = Path::new(output_path);
        if let Some(parent) = path.parent() {
            if !parent.exists() && !self.config.processing_options.create_output_directory {
                return Err(ProcessError::InvalidInput(format!("Output directory does not exist: {}", parent.display())));
            }
        }

        // Check if output file exists and overwrite is not allowed
        if path.exists() && path.is_file() && !self.config.processing_options.overwrite_existing {
            return Err(ProcessError::ValidationError(format!("Output file already exists: {}", output_path)));
        }

        Ok(())
    }

    /// Get current configuration
    pub fn get_config(&self) -> &ProcessConfig {
        &self.config
    }

    /// Get processing statistics
    pub fn get_stats(&self) -> &ProcessingStats {
        &self.stats
    }

    /// Get supported file formats
    pub fn get_supported_formats(&self) -> &Vec<FileFormat> {
        &self.config.supported_formats
    }

    /// Check if a file format is supported
    pub fn is_format_supported(&self, format: &FileFormat) -> bool {
        self.config.supported_formats.contains(format)
    }

    /// Run video extraction processing (matching extraction/processing.rs::run)
    pub fn run_video_extraction(&mut self, config_path: &str) -> Result<(), ProcessError> {
        VideoProcessor::run_video_extraction(config_path, &mut self.stats)
    }
} 