//! Video processing functionality for the process module

use std::path::{Path, PathBuf};
use std::fs;
use std::process::Command;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::thread;
use std::collections::HashMap;
use std::time::Instant;
use opencv::{
    core::{Size, Mat, Vector},
    imgcodecs,
    prelude::*,
    videoio::{self, VideoCapture, VideoCaptureAPIs::CAP_ANY},
};
use path_clean::PathClean;
use rayon::prelude::*;

use crate::process::types::ProcessError;
use crate::process::config::VideoExtractionConfig;
use crate::process::stats::ProcessingStats;

/// Video processing functionality
pub struct VideoProcessor;

impl VideoProcessor {
    /// Run video extraction processing (matching extraction/processing.rs::run)
    pub fn run_video_extraction(
        config_path: &str,
        stats: &mut ProcessingStats,
    ) -> Result<(), ProcessError> {
        let start_time = Instant::now();

        let config_data = fs::read_to_string(config_path)
            .map_err(|e| ProcessError::IoError(format!("Unable to read config file {}: {}", config_path, e)))?;

        let deserializer = &mut serde_json::Deserializer::from_str(&config_data);
        let video_config: VideoExtractionConfig = serde_path_to_error::deserialize(deserializer)
            .map_err(|e| ProcessError::ConfigurationError(format!("Error parsing config.json at '{}': {}", e.path(), e)))?;

        let config = Arc::new(video_config);
        let temp_dirs_created = Arc::new(Mutex::new(Vec::<PathBuf>::new()));

        let mut video_files_by_dir: HashMap<String, Vec<PathBuf>> = HashMap::new();
        for dir_path_str in &config.input_directories {
            let dir_path = Path::new(dir_path_str);
            if !dir_path.is_dir() {
                eprintln!(
                    "Warning: Input path is not a directory, skipping: {}",
                    dir_path.display()
                );
                continue;
            }

            let video_files: Vec<PathBuf> = fs::read_dir(dir_path)
                .map_err(|e| ProcessError::IoError(format!("Failed to read directory {}: {}", dir_path.display(), e)))?
                .filter_map(|entry| entry.ok())
                .filter(|entry| {
                    let path = entry.path();
                    path.is_file()
                        && matches!(
                            path.extension().and_then(|s| s.to_str()),
                            Some("mp4" | "mov" | "avi" | "mkv")
                        )
                })
                .map(|entry| entry.path().clean())
                .collect();

            if !video_files.is_empty() {
                video_files_by_dir.insert(dir_path_str.to_string(), video_files);
            }
        }

        let processing_mode = config.processing_mode.as_deref().unwrap_or("parallel");

        match processing_mode {
            "sequential" => {
                println!("Running in sequential mode.");
                for (dir_path, video_list) in video_files_by_dir {
                    if let Err(e) = Self::process_video_directory(
                        dir_path.clone(),
                        video_list,
                        Arc::clone(&config),
                        Arc::clone(&temp_dirs_created),
                    ) {
                        eprintln!("Error processing directory {}: {}", dir_path, e);
                        stats.add_failed_file(format!("Directory {}: {}", dir_path, e));
                    }
                }
            }
            "parallel" | _ => {
                println!("Running in parallel mode.");
                let num_threads = config.num_threads.unwrap_or_else(num_cpus::get);
                rayon::ThreadPoolBuilder::new()
                    .num_threads(num_threads)
                    .build_global()
                    .map_err(|e| ProcessError::ProcessingFailed(format!("Failed to build thread pool: {}", e)))?;

                video_files_by_dir
                    .into_par_iter()
                    .for_each(|(dir_path, video_list)| {
                        if let Err(e) = Self::process_video_directory(
                            dir_path.clone(),
                            video_list,
                            Arc::clone(&config),
                            Arc::clone(&temp_dirs_created),
                        ) {
                            eprintln!("Error processing directory in parallel {}: {}", dir_path, e);
                        }
                    });
            }
        }

        // Cleanup temporary directories
        {
            let dirs_to_clean = temp_dirs_created.lock().unwrap();
            for dir in dirs_to_clean.iter() {
                println!("Cleaning up temporary directory: {}", dir.display());
                if let Err(e) = fs::remove_dir_all(dir) {
                    eprintln!(
                        "Warning: Failed to remove temporary directory {}: {}",
                        dir.display(),
                        e
                    );
                }
            }
        }

        let duration = start_time.elapsed();
        println!("Total execution time: {:?}", duration);
        stats.processing_time = duration;

        Ok(())
    }

    /// Process video directory (matching extraction/processing.rs::process_directory)
    fn process_video_directory(
        input_dir_path: String,
        video_list: Vec<PathBuf>,
        config: Arc<VideoExtractionConfig>,
        temp_dirs_created: Arc<Mutex<Vec<PathBuf>>>,
    ) -> Result<(), ProcessError> {
        let dir_tag = Self::get_directory_tag(&input_dir_path);
        println!(
            "Thread {:?} processing directory: {} ({} videos, tag: '{}')",
            thread::current().id(),
            input_dir_path,
            video_list.len(),
            dir_tag
        );

        let output_base = PathBuf::from(&config.output_directory);
        fs::create_dir_all(&output_base)
            .map_err(|e| ProcessError::IoError(format!("Failed to create output directory: {}", e)))?;

        let output_video_file = format!("{}_{}.mp4", config.output_prefix, dir_tag);
        let output_video_path = output_base.join(output_video_file);

        // Determine modes
        let creation_mode = config.video_creation_mode.as_deref().unwrap_or("temp_frames");
        let use_ffmpeg_extraction = config.extraction_mode == "ffmpeg";

        println!(
            "Processing with: Extraction = {}, Creation = {}",
            if use_ffmpeg_extraction { "ffmpeg" } else { "opencv" },
            creation_mode
        );

        let mut sorted_video_list = video_list;
        sorted_video_list.sort();

        // Process based on modes
        if creation_mode == "direct" && config.extraction_mode == "opencv" {
            Self::process_direct_opencv(&sorted_video_list, &output_video_path, &config)
        } else if creation_mode == "direct" && use_ffmpeg_extraction {
            Self::process_direct_ffmpeg(&sorted_video_list, &output_video_path, &config, &output_base, &dir_tag, temp_dirs_created)
        } else {
            Self::process_temp_frames(&sorted_video_list, &output_video_path, &config, &output_base, &dir_tag, temp_dirs_created)
        }
    }

    /// Get directory tag from path
    fn get_directory_tag(input_dir_path: &str) -> String {
        Path::new(input_dir_path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("default")
            .to_string()
    }

    /// Process using direct OpenCV method (memory-efficient)
    fn process_direct_opencv(
        video_list: &[PathBuf],
        output_video_path: &PathBuf,
        config: &VideoExtractionConfig,
    ) -> Result<(), ProcessError> {
        println!("Using memory-efficient direct OpenCV processing.");
        let mut output_writer: Option<videoio::VideoWriter> = None;
        let mut output_frame_size: Option<Size> = None;
        let mut videos_processed_count = 0;

        for (video_index, video_path) in video_list.iter().enumerate() {
            println!(
                "  Thread {:?} processing video {}/{}: {}",
                thread::current().id(),
                video_index + 1,
                video_list.len(),
                video_path.display()
            );

            let mut cap = match VideoCapture::from_file(video_path.to_str().unwrap(), CAP_ANY.into()) {
                Ok(cap) => cap,
                Err(e) => {
                    eprintln!(
                        "Warning: OpenCV failed to create capture for video {}, skipping: {}",
                        video_path.display(),
                        e
                    );
                    continue;
                }
            };

            if !cap.is_opened().map_err(|e| ProcessError::ProcessingFailed(format!("OpenCV error: {}", e)))? {
                eprintln!(
                    "Warning: OpenCV failed to open video {}, skipping.",
                    video_path.display()
                );
                continue;
            }

            // Initialize writer from first valid video
            if output_writer.is_none() {
                let width = cap.get(videoio::CAP_PROP_FRAME_WIDTH)
                    .map_err(|e| ProcessError::ProcessingFailed(format!("Failed to get frame width: {}", e)))? as i32;
                let height = cap.get(videoio::CAP_PROP_FRAME_HEIGHT)
                    .map_err(|e| ProcessError::ProcessingFailed(format!("Failed to get frame height: {}", e)))? as i32;

                if width > 0 && height > 0 {
                    let size = Size::new(width, height);
                    println!("Determined output frame size {:?} from video {}", size, video_path.display());
                    output_frame_size = Some(size);

                    let fourcc = videoio::VideoWriter::fourcc('a', 'v', 'c', '1')
                        .map_err(|e| ProcessError::ProcessingFailed(format!("Failed to create fourcc: {}", e)))?;
                    let writer = videoio::VideoWriter::new(
                        output_video_path.to_str().unwrap(),
                        fourcc,
                        config.output_fps as f64,
                        size,
                        true,
                    ).map_err(|e| ProcessError::ProcessingFailed(format!("Failed to create VideoWriter: {}", e)))?;

                    if !writer.is_opened().map_err(|e| ProcessError::ProcessingFailed(format!("VideoWriter error: {}", e)))? {
                        return Err(ProcessError::ProcessingFailed(format!(
                            "Failed to open VideoWriter for output file {}",
                            output_video_path.display()
                        )));
                    }
                    println!("Opened VideoWriter for {}", output_video_path.display());
                    output_writer = Some(writer);
                } else {
                    eprintln!(
                        "Warning: Could not get valid frame size from video {}, trying next video.",
                        video_path.display()
                    );
                    continue;
                }
            }

            // Process frames
            if let Some(ref mut writer) = output_writer {
                let expected_size = output_frame_size.unwrap();
                let total_frames_cv = cap.get(videoio::CAP_PROP_FRAME_COUNT)
                    .map_err(|e| ProcessError::ProcessingFailed(format!("Failed to get frame count: {}", e)))? as usize;

                for frame_number in (0..total_frames_cv).step_by(config.frame_interval) {
                    let mut frame = Mat::default();
                    if !cap.set(videoio::CAP_PROP_POS_FRAMES, frame_number as f64)
                        .map_err(|e| ProcessError::ProcessingFailed(format!("Failed to seek frame: {}", e)))? {
                        eprintln!(
                            "Warning: OpenCV failed to seek to frame {} in {}. Skipping frame.",
                            frame_number,
                            video_path.display()
                        );
                        continue;
                    }

                    if cap.read(&mut frame)
                        .map_err(|e| ProcessError::ProcessingFailed(format!("Failed to read frame: {}", e)))? {
                        if frame.empty() {
                            eprintln!(
                                "Warning: OpenCV read empty frame at index {} from {}. Skipping frame.",
                                frame_number,
                                video_path.display()
                            );
                            continue;
                        }

                        // Check frame size
                        if frame.size().map_err(|e| ProcessError::ProcessingFailed(format!("Failed to get frame size: {}", e)))? != expected_size {
                            eprintln!(
                                "Warning: Frame {} size does not match writer size in video {}. Skipping frame.",
                                frame_number,
                                video_path.display()
                            );
                            continue;
                        }

                        // Write the frame
                        if let Err(e) = writer.write(&frame) {
                            eprintln!(
                                "Error writing frame {} from video {}: {}. Aborting directory.",
                                frame_number,
                                video_path.display(),
                                e
                            );
                            let _ = writer.release();
                            return Err(ProcessError::ProcessingFailed(format!("VideoWriter write error: {}", e)));
                        }
                    } else {
                        println!(
                            "Finished reading frames or encountered read error for video {}",
                            video_path.display()
                        );
                        break;
                    }
                }
                videos_processed_count += 1;
            }
        }

        // Release writer
        if let Some(mut writer) = output_writer {
            println!("Releasing VideoWriter for {}", output_video_path.display());
            writer.release().map_err(|e| ProcessError::ProcessingFailed(format!("Failed to release writer: {}", e)))?;
            if videos_processed_count > 0 {
                println!("Successfully created video (direct/opencv): {}", output_video_path.display());
            } else {
                println!("No videos successfully processed to create output file {}", output_video_path.display());
                let _ = fs::remove_file(&output_video_path);
            }
        } else {
            println!("VideoWriter was never initialized. No output file created.");
        }

        Ok(())
    }

    /// Process using direct FFmpeg method
    fn process_direct_ffmpeg(
        video_list: &[PathBuf],
        output_video_path: &PathBuf,
        config: &VideoExtractionConfig,
        output_base: &PathBuf,
        dir_tag: &str,
        temp_dirs_created: Arc<Mutex<Vec<PathBuf>>>,
    ) -> Result<(), ProcessError> {
        println!("Using ffmpeg extraction with direct creation.");
        
        // Create temp directory
        let dir_name = format!("{}_{}_ffmpeg_direct_temp_{:?}", config.output_prefix, dir_tag, thread::current().id());
        let temp_path = output_base.join(dir_name);
        fs::create_dir_all(&temp_path)
            .map_err(|e| ProcessError::IoError(format!("Failed to create temp directory: {}", e)))?;
        temp_dirs_created.lock().unwrap().push(temp_path.clone());
        println!("Created transient temp directory for ffmpeg: {}", temp_path.display());

        // Extract frames using FFmpeg
        for (video_index, video_path) in video_list.iter().enumerate() {
            println!(
                "  Thread {:?} extracting via ffmpeg from video {}/{}: {}",
                thread::current().id(),
                video_index + 1,
                video_list.len(),
                video_path.display()
            );
            
            Self::extract_frames_ffmpeg(
                video_path.to_str().unwrap(),
                video_index,
                temp_path.to_str().unwrap(),
                config.frame_interval,
            )?;
        }

        // Create video from extracted frames
        Self::create_video_from_temp_frames(temp_path.to_str().unwrap(), output_video_path, config.output_fps)
    }

    /// Process using temp frames method
    fn process_temp_frames(
        video_list: &[PathBuf],
        output_video_path: &PathBuf,
        config: &VideoExtractionConfig,
        output_base: &PathBuf,
        dir_tag: &str,
        temp_dirs_created: Arc<Mutex<Vec<PathBuf>>>,
    ) -> Result<(), ProcessError> {
        println!("Using temp frames approach.");
        
        // Create temp directory
        let dir_name = format!("{}_{}_temp_{:?}", config.output_prefix, dir_tag, thread::current().id());
        let temp_path = output_base.join(dir_name);
        fs::create_dir_all(&temp_path)
            .map_err(|e| ProcessError::IoError(format!("Failed to create temp directory: {}", e)))?;
        temp_dirs_created.lock().unwrap().push(temp_path.clone());

        // Extract frames
        for (video_index, video_path) in video_list.iter().enumerate() {
            if config.extraction_mode == "ffmpeg" {
                Self::extract_frames_ffmpeg(
                    video_path.to_str().unwrap(),
                    video_index,
                    temp_path.to_str().unwrap(),
                    config.frame_interval,
                )?;
            } else {
                Self::extract_frames_opencv(
                    video_path.to_str().unwrap(),
                    video_index,
                    temp_path.to_str().unwrap(),
                    config.frame_interval,
                )?;
            }
        }

        // Create video from frames
        Self::create_video_from_temp_frames(temp_path.to_str().unwrap(), output_video_path, config.output_fps)
    }

    /// Extract frames using OpenCV (matching extraction/video.rs::extract_frames_opencv)
    pub fn extract_frames_opencv(
        video_filename: &str,
        video_index: usize,
        temp_frame_dir: &str,
        frame_interval: usize,
    ) -> Result<(), ProcessError> {
        fs::create_dir_all(temp_frame_dir)
            .map_err(|e| ProcessError::IoError(format!("Failed to create temp frame directory: {}", e)))?;

        let mut cap = VideoCapture::from_file(video_filename, CAP_ANY.into())
            .map_err(|e| ProcessError::ProcessingFailed(format!("Failed to open video {}: {}", video_filename, e)))?;

        if !cap.is_opened()
            .map_err(|e| ProcessError::ProcessingFailed(format!("OpenCV error: {}", e)))? {
            return Err(ProcessError::ProcessingFailed(format!("Failed to open video: {}", video_filename)));
        }

        let total_frames = cap.get(videoio::CAP_PROP_FRAME_COUNT)
            .map_err(|e| ProcessError::ProcessingFailed(format!("Failed to get frame count: {}", e)))? as usize;

        for frame_number in (0..total_frames).step_by(frame_interval) {
            let mut frame = Mat::default();
            if !cap.set(videoio::CAP_PROP_POS_FRAMES, frame_number as f64)
                .map_err(|e| ProcessError::ProcessingFailed(format!("Failed to seek frame: {}", e)))? {
                eprintln!("Warning: Failed to seek to frame {} in {}", frame_number, video_filename);
                continue;
            }

            if cap.read(&mut frame)
                .map_err(|e| ProcessError::ProcessingFailed(format!("Failed to read frame: {}", e)))? {
                if frame.empty() {
                    eprintln!("Warning: Read empty frame at index {} from {}", frame_number, video_filename);
                    continue;
                }
                let output_path = format!(
                    "{}/video{:03}_frame{:07}.jpg",
                    temp_frame_dir,
                    video_index,
                    frame_number
                );
                imgcodecs::imwrite(&output_path, &frame, &Vector::new())
                    .map_err(|e| ProcessError::ProcessingFailed(format!("Failed to write frame: {}", e)))?;
            } else {
                break;
            }
        }
        Ok(())
    }

    /// Extract frames using FFmpeg (matching extraction/video.rs::extract_frames_ffmpeg)
    pub fn extract_frames_ffmpeg(
        video_filename: &str,
        video_index: usize,
        temp_frame_dir: &str,
        frame_interval: usize,
    ) -> Result<(), ProcessError> {
        fs::create_dir_all(temp_frame_dir)
            .map_err(|e| ProcessError::IoError(format!("Failed to create temp frame directory: {}", e)))?;

        if frame_interval == 0 {
            return Err(ProcessError::ValidationError("frame_interval must be greater than 0 for ffmpeg extraction.".to_string()));
        }

        let output_pattern = Path::new(temp_frame_dir)
            .join(format!("video{}_frame%06d.jpg", video_index));
        let output_pattern_str = output_pattern.to_str()
            .ok_or_else(|| ProcessError::ProcessingFailed("Invalid output path pattern".to_string()))?;

        let mut cmd = Command::new("ffmpeg");
        cmd.arg("-i")
            .arg(video_filename)
            .arg("-vf")
            .arg(format!("select=not(mod(n\\,{}))", frame_interval))
            .arg("-vsync")
            .arg("vfr")
            .arg("-q:v")
            .arg("2")
            .arg(output_pattern_str)
            .arg("-hide_banner")
            .arg("-loglevel")
            .arg("warning");

        println!(
            "Running ffmpeg frame extraction for video {}: {}",
            video_index,
            video_filename
        );

        let output = cmd.output()
            .map_err(|e| ProcessError::ProcessingFailed(format!("Failed to execute ffmpeg: {}", e)))?;

        if !output.status.success() {
            eprintln!("ffmpeg stdout: {}", String::from_utf8_lossy(&output.stdout));
            eprintln!("ffmpeg stderr: {}", String::from_utf8_lossy(&output.stderr));
            return Err(ProcessError::ProcessingFailed(format!(
                "ffmpeg frame extraction failed for video {}",
                video_filename
            )));
        }

        println!(
            "Successfully extracted frames using ffmpeg for video {}: {}",
            video_index,
            video_filename
        );
        Ok(())
    }

    /// Create video from temp frames (matching extraction/video.rs::create_video_from_temp_frames)
    pub fn create_video_from_temp_frames(
        temp_frame_dir: &str,
        output_video_path: &PathBuf,
        fps: i32,
    ) -> Result<(), ProcessError> {
        let frame_source_dir = Path::new(temp_frame_dir);
        let final_output_dir = output_video_path.parent().unwrap_or_else(|| Path::new("."));

        fs::create_dir_all(final_output_dir)
            .map_err(|e| ProcessError::IoError(format!("Failed to create output directory: {}", e)))?;

        if !frame_source_dir.exists() {
            eprintln!("Warning: Temporary frame directory {} does not exist. Skipping video creation.", temp_frame_dir);
            return Ok(());
        }

        let mut image_files: Vec<fs::DirEntry> = match fs::read_dir(frame_source_dir) {
            Ok(reader) => reader
                .filter_map(|entry| entry.ok())
                .filter(|entry| {
                    entry.path().is_file() &&
                    entry.path().extension()
                        .and_then(|ext| ext.to_str())
                        .map(|ext| ext.eq_ignore_ascii_case("jpg"))
                        .unwrap_or(false)
                })
                .collect(),
            Err(e) => {
                eprintln!("Warning: Failed to read temporary frame directory {}: {}. Skipping video creation.", temp_frame_dir, e);
                return Ok(());
            }
        };

        if image_files.is_empty() {
            println!("No .jpg frames found in {}. No video will be created.", temp_frame_dir);
            return Ok(());
        }

        // Sort files by frame number
        image_files.sort_by(|a, b| {
            let path_a = a.path();
            let path_b = b.path();
            let filename_a = path_a.file_stem().and_then(|n| n.to_str()).unwrap_or("");
            let filename_b = path_b.file_stem().and_then(|n| n.to_str()).unwrap_or("");

            match (Self::parse_frame_filename(filename_a), Self::parse_frame_filename(filename_b)) {
                (Some((vid_a, frame_a)), Some((vid_b, frame_b))) => {
                    vid_a.cmp(&vid_b).then_with(|| frame_a.cmp(&frame_b))
                }
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            }
        });

        // Create FFmpeg list file
        let list_file_path = frame_source_dir.join("ffmpeg_list.txt");
        {
            let mut list_file = fs::File::create(&list_file_path)
                .map_err(|e| ProcessError::IoError(format!("Failed to create ffmpeg list file: {}", e)))?;
            for entry in &image_files {
                match fs::canonicalize(entry.path()) {
                    Ok(absolute_path) => {
                        let path_str = absolute_path.to_string_lossy().replace("\\", "/");
                        if writeln!(list_file, "file '{}'", path_str).is_err() {
                            eprintln!("Error writing to ffmpeg list file for {}", entry.path().display());
                        }
                    }
                    Err(e) => {
                        eprintln!("Warning: Could not canonicalize path {}: {}", entry.path().display(), e);
                    }
                }
            }
            list_file.flush()
                .map_err(|e| ProcessError::IoError(format!("Failed to flush list file: {}", e)))?;
        }

        // Create video using FFmpeg
        let mut cmd = Command::new("ffmpeg");
        cmd.arg("-y")
            .arg("-f")
            .arg("concat")
            .arg("-safe")
            .arg("0")
            .arg("-i")
            .arg(list_file_path.to_str().unwrap())
            .arg("-r")
            .arg(fps.to_string())
            .arg("-c:v")
            .arg("libx264")
            .arg("-pix_fmt")
            .arg("yuv420p")
            .arg(output_video_path.to_str().unwrap())
            .arg("-hide_banner")
            .arg("-loglevel")
            .arg("warning");

        println!("Creating video from frames: {}", output_video_path.display());

        let output = cmd.output()
            .map_err(|e| ProcessError::ProcessingFailed(format!("Failed to execute ffmpeg for video creation: {}", e)))?;

        if !output.status.success() {
            eprintln!("ffmpeg stdout: {}", String::from_utf8_lossy(&output.stdout));
            eprintln!("ffmpeg stderr: {}", String::from_utf8_lossy(&output.stderr));
            return Err(ProcessError::ProcessingFailed(format!(
                "ffmpeg video creation failed for {}",
                output_video_path.display()
            )));
        }

        println!("Successfully created video: {}", output_video_path.display());
        Ok(())
    }

    /// Parse frame filename to extract video index and frame number
    fn parse_frame_filename(filename: &str) -> Option<(usize, usize)> {
        // Parse patterns like "video001_frame0000123" or "video0_frame000456"
        if let Some(captures) = regex::Regex::new(r"video(\d+)_frame(\d+)")
            .ok()?
            .captures(filename) {
            let video_index = captures.get(1)?.as_str().parse().ok()?;
            let frame_number = captures.get(2)?.as_str().parse().ok()?;
            Some((video_index, frame_number))
        } else {
            None
        }
    }
} 