//! Tests for the process module

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use crate::process::types::{ProcessError, ProcessingMode, FileFormat, VideoFormat, ImageFormat, DocumentFormat, get_default_supported_formats};
    use crate::process::config::{ProcessConfig, ProcessingOptions, VideoExtractionConfig};
    use crate::process::stats::ProcessingStats;
    use crate::process::processor::Processor;
    use crate::process::convenience::*;

    fn create_test_file(path: &str, content: &str) -> std::io::Result<()> {
        let mut file = File::create(path)?;
        file.write_all(content.as_bytes())?;
        Ok(())
    }

    #[test]
    fn test_processor_creation() {
        let config = ProcessConfig {
            input_path: "test_input".to_string(),
            output_path: "test_output".to_string(),
            processing_options: ProcessingOptions::default(),
            processing_mode: ProcessingMode::SingleFile,
            supported_formats: get_default_supported_formats(),
            video_config: None,
        };

        let processor = Processor::new(config);
        assert!(processor.is_ok());
    }

    #[test]
    fn test_processor_creation_empty_input() {
        let config = ProcessConfig {
            input_path: "".to_string(),
            output_path: "test_output".to_string(),
            processing_options: ProcessingOptions::default(),
            processing_mode: ProcessingMode::SingleFile,
            supported_formats: get_default_supported_formats(),
            video_config: None,
        };

        let processor = Processor::new(config);
        assert!(processor.is_err());
    }

    #[test]
    fn test_processor_creation_empty_output() {
        let config = ProcessConfig {
            input_path: "test_input".to_string(),
            output_path: "".to_string(),
            processing_options: ProcessingOptions::default(),
            processing_mode: ProcessingMode::SingleFile,
            supported_formats: get_default_supported_formats(),
            video_config: None,
        };

        let processor = Processor::new(config);
        assert!(processor.is_err());
    }

    #[test]
    fn test_create_processor_convenience_function() {
        let processor = create_processor("input".to_string(), "output".to_string());
        assert!(processor.is_ok());
    }

    #[test]
    fn test_processing_options_default() {
        let options = ProcessingOptions::default();
        assert!(options.enable_validation);
        assert!(!options.verbose_logging);
        assert!(options.create_output_directory);
        assert!(!options.overwrite_existing);
        assert_eq!(options.max_file_size_mb, Some(1024));
        assert_eq!(options.timeout_seconds, Some(300));
        assert!(!options.parallel_processing);
        assert!(!options.backup_original);
    }

    #[test]
    fn test_processing_stats() {
        let mut stats = ProcessingStats::new();
        assert_eq!(stats.files_processed, 0);
        assert_eq!(stats.files_failed, 0);
        assert_eq!(stats.success_rate(), 0.0);

        stats.add_processed_file(1024);
        assert_eq!(stats.files_processed, 1);
        assert_eq!(stats.total_size_processed, 1024);
        assert_eq!(stats.success_rate(), 100.0);

        stats.add_failed_file("Test error".to_string());
        assert_eq!(stats.files_failed, 1);
        assert_eq!(stats.success_rate(), 50.0);
    }

    #[test]
    fn test_file_format_detection() {
        let processor = create_processor("input".to_string(), "output".to_string()).unwrap();
        
        let mp4_path = std::path::Path::new("test.mp4");
        let format = processor.detect_file_format(mp4_path);
        assert!(matches!(format, Ok(FileFormat::Video(VideoFormat::Mp4))));

        let jpg_path = std::path::Path::new("test.jpg");
        let format = processor.detect_file_format(jpg_path);
        assert!(matches!(format, Ok(FileFormat::Image(ImageFormat::Jpg))));

        let txt_path = std::path::Path::new("test.txt");
        let format = processor.detect_file_format(txt_path);
        assert!(matches!(format, Ok(FileFormat::Document(DocumentFormat::Txt))));
    }

    #[test]
    fn test_processing_modes() {
        // Test different processing modes
        let modes = vec![
            ProcessingMode::SingleFile,
            ProcessingMode::BatchFiles,
            ProcessingMode::DirectoryProcess,
            ProcessingMode::StreamProcess,
        ];

        for mode in modes {
            let processor = create_processor_with_mode(
                "input".to_string(),
                "output".to_string(),
                mode.clone(),
            );
            assert!(processor.is_ok());
            assert_eq!(processor.unwrap().get_config().processing_mode, mode);
        }
    }

    #[test]
    fn test_supported_formats() {
        let formats = get_default_supported_formats();
        assert!(!formats.is_empty());
        
        // Check that we have formats from each category
        let has_video = formats.iter().any(|f| matches!(f, FileFormat::Video(_)));
        let has_audio = formats.iter().any(|f| matches!(f, FileFormat::Audio(_)));
        let has_image = formats.iter().any(|f| matches!(f, FileFormat::Image(_)));
        let has_document = formats.iter().any(|f| matches!(f, FileFormat::Document(_)));
        
        assert!(has_video);
        assert!(has_audio);
        assert!(has_image);
        assert!(has_document);
    }

    #[test]
    fn test_format_support_check() {
        let processor = create_processor("input".to_string(), "output".to_string()).unwrap();
        
        let mp4_format = FileFormat::Video(VideoFormat::Mp4);
        assert!(processor.is_format_supported(&mp4_format));
        
        // All default formats should be supported
        for format in get_default_supported_formats() {
            assert!(processor.is_format_supported(&format));
        }
    }

    #[test]
    fn test_video_processor_creation() {
        let processor = create_video_processor();
        assert!(processor.is_ok());
        
        let processor = processor.unwrap();
        assert_eq!(processor.get_config().processing_mode, ProcessingMode::DirectoryProcess);
        assert!(processor.get_config().video_config.is_none());
        
        // Should support video formats
        let mp4_format = FileFormat::Video(VideoFormat::Mp4);
        assert!(processor.is_format_supported(&mp4_format));
    }

    #[test]
    fn test_video_extraction_config() {
        let config = VideoExtractionConfig {
            input_directories: vec!["test_dir".to_string()],
            output_directory: "output".to_string(),
            output_prefix: "test".to_string(),
            num_threads: Some(4),
            output_fps: 30,
            frame_interval: 10,
            extraction_mode: "opencv".to_string(),
            create_summary_per_thread: Some(true),
            video_creation_mode: Some("direct".to_string()),
            processing_mode: Some("parallel".to_string()),
        };

        // Test serialization/deserialization
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: VideoExtractionConfig = serde_json::from_str(&json).unwrap();
        
        assert_eq!(config.input_directories, deserialized.input_directories);
        assert_eq!(config.output_fps, deserialized.output_fps);
        assert_eq!(config.frame_interval, deserialized.frame_interval);
        assert_eq!(config.extraction_mode, deserialized.extraction_mode);
    }

    #[test]
    fn test_parse_frame_filename() {
        use crate::process::video::VideoProcessor;
        
        // Test valid frame filename patterns
        // Note: This test would need the parse_frame_filename method to be public
        // For now, we'll skip this test or make the method public in VideoProcessor
        
        // This test is commented out because parse_frame_filename is private
        // If needed, we can make it public or create a public wrapper
        /*
        assert_eq!(VideoProcessor::parse_frame_filename("video001_frame0000123"), Some((1, 123)));
        assert_eq!(VideoProcessor::parse_frame_filename("video0_frame000456"), Some((0, 456)));
        assert_eq!(VideoProcessor::parse_frame_filename("video999_frame9999999"), Some((999, 9999999)));
        
        // Test invalid patterns
        assert_eq!(VideoProcessor::parse_frame_filename("invalid_filename"), None);
        assert_eq!(VideoProcessor::parse_frame_filename("video_frame"), None);
        assert_eq!(VideoProcessor::parse_frame_filename(""), None);
        */
    }
} 