use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;
use opencv::{
    core::{Size, Mat},
    imgcodecs,
    prelude::*,
    videoio::{self, VideoCapture, VideoCaptureAPIs::CAP_ANY},
};
use path_clean::PathClean;
use rayon::prelude::*;
use serde_path_to_error;

use crate::config::Config;
use crate::video::{
    create_video_from_temp_frames, extract_frames_ffmpeg,
    extract_frames_opencv, parse_frame_filename,
};

pub fn run(config_path: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
    let start_time = Instant::now();

    let config_data = fs::read_to_string(config_path)
        .map_err(|e| format!("Unable to read config file {}: {}", config_path, e))?;

    let deserializer = &mut serde_json::Deserializer::from_str(&config_data);
    let config: Config = serde_path_to_error::deserialize(deserializer)
        .map_err(|e| format!("Error parsing config.json at '{}': {}", e.path(), e))?;

    let config = Arc::new(config);

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

        let video_files: Vec<PathBuf> = fs::read_dir(dir_path)?
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
                if let Err(e) = process_directory(
                    dir_path.clone(),
                    video_list,
                    Arc::clone(&config),
                    Arc::clone(&temp_dirs_created),
                ) {
                    eprintln!("Error processing directory {}: {}", dir_path, e);
                }
            }
        }
        "parallel" | _ => {
            println!("Running in parallel mode.");
            let num_threads = config.num_threads.unwrap_or_else(num_cpus::get);
            rayon::ThreadPoolBuilder::new()
                .num_threads(num_threads)
                .build_global()?;

            video_files_by_dir
                .into_par_iter()
                .for_each(|(dir_path, video_list)| {
                    if let Err(e) = process_directory(
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

    Ok(())
}

fn get_directory_tag(input_dir_path: &str) -> String {
    Path::new(input_dir_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("default")
        .to_string()
}

pub fn process_directory(
    input_dir_path: String,
    video_list: Vec<PathBuf>,
    config: Arc<Config>,
    temp_dirs_created: Arc<Mutex<Vec<PathBuf>>>,
) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    let dir_tag = get_directory_tag(&input_dir_path);
    println!(
        "Thread {:?} processing directory: {} ({} videos, tag: '{}')",
        thread::current().id(),
        input_dir_path,
        video_list.len(),
        dir_tag
    );

    let output_base = PathBuf::from(&config.output_directory);
    fs::create_dir_all(&output_base)?;

    let output_video_file = format!(
        "{}_{}.mp4",
        config.output_prefix,
        dir_tag
    );
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

    // --- Processing Logic based on Modes --- 

    if creation_mode == "direct" && config.extraction_mode == "opencv" {
        // --- CASE 1: OpenCV Extract + Direct Create (Memory Efficient) ---
        println!("Using memory-efficient direct OpenCV processing.");
        let mut output_writer: Option<videoio::VideoWriter> = None;
        let mut output_frame_size: Option<Size> = None;
        let mut videos_processed_count = 0;

        for (video_index, video_path) in sorted_video_list.iter().enumerate() {
            println!(
                "  Thread {:?} processing video {}/{}: {}",
                thread::current().id(), video_index + 1, sorted_video_list.len(), video_path.display()
            );
            let mut cap = match VideoCapture::from_file(video_path.to_str().unwrap(), CAP_ANY.into()) {
                Ok(cap) => cap,
                Err(e) => {
                    eprintln!("Warning: OpenCV failed to create capture for video {}, skipping: {}", video_path.display(), e);
                    continue;
                }
            };
            if !cap.is_opened()? {
                eprintln!("Warning: OpenCV failed to open video {}, skipping.", video_path.display());
                continue;
            }

            // Determine frame size and initialize writer from the first valid video
            if output_writer.is_none() {
                let width = cap.get(videoio::CAP_PROP_FRAME_WIDTH)? as i32;
                let height = cap.get(videoio::CAP_PROP_FRAME_HEIGHT)? as i32;
                if width > 0 && height > 0 {
                    let size = Size::new(width, height);
                    println!("Determined output frame size {:?} from video {}", size, video_path.display());
                    output_frame_size = Some(size);
                    
                    let fourcc = videoio::VideoWriter::fourcc('a', 'v', 'c', '1')?;
                    let writer = videoio::VideoWriter::new(
                        output_video_path.to_str().unwrap(),
                        fourcc,
                        config.output_fps as f64,
                        size,
                        true, // isColor
                    )?;
                    
                    if !writer.is_opened()? {
                        eprintln!("Error: Failed to open VideoWriter for output file {}. Aborting directory processing.", output_video_path.display());
                        return Err(format!("Failed to open VideoWriter for {}", output_video_path.display()).into());
                    }
                    println!("Opened VideoWriter for {}", output_video_path.display());
                    output_writer = Some(writer);
                } else {
                     eprintln!("Warning: Could not get valid frame size from video {}, trying next video.", video_path.display());
                     continue; // Try next video to initialize writer
                }
            }

            // Process frames if writer is initialized
            if let Some(ref mut writer) = output_writer {
                 let expected_size = output_frame_size.unwrap(); // Safe to unwrap here
                 let total_frames_cv = cap.get(videoio::CAP_PROP_FRAME_COUNT)? as usize;
                 for frame_number in (0..total_frames_cv).step_by(config.frame_interval) {
                     let mut frame = Mat::default();
                     if !cap.set(videoio::CAP_PROP_POS_FRAMES, frame_number as f64)? {
                          eprintln!("Warning: OpenCV failed to seek to frame {} in {}. Skipping frame.", frame_number, video_path.display());
                          continue;
                     }
                     if cap.read(&mut frame)? {
                         if frame.empty() {
                              eprintln!("Warning: OpenCV read empty frame at index {} from {}. Skipping frame.", frame_number, video_path.display());
                              continue;
                         }
                         // Check frame size
                         if frame.size()? != expected_size {
                             eprintln!(
                                 "Warning: Frame {} size ({:?}) in video {} does not match writer size ({:?}). Skipping frame.",
                                 frame_number, frame.size()?, video_path.display(), expected_size
                             );
                             continue;
                         }
                         // Write the frame
                         if let Err(e) = writer.write(&frame) {
                             eprintln!("Error writing frame {} from video {}: {}. Aborting directory.", frame_number, video_path.display(), e);
                             // Attempt to release writer before returning error
                             let _ = writer.release();
                             return Err(format!("VideoWriter write error: {}", e).into());
                         }
                     } else {
                        // End of video or read error
                        println!("Finished reading frames or encountered read error for video {}", video_path.display());
                        break; 
                     }
                 } // End frame loop
                 videos_processed_count += 1;
            } else {
                // Should not happen if logic above is correct, means we couldn't init writer
                eprintln!("Error: VideoWriter not initialized, cannot process frames for {}.", video_path.display());
                // Potentially return error if no videos could initialize the writer
            }
        } // End video loop

        // Release the writer after processing all videos for the directory
        if let Some(mut writer) = output_writer {
            println!("Releasing VideoWriter for {}", output_video_path.display());
            writer.release()?;
            if videos_processed_count > 0 {
                 println!("Successfully created video (direct/opencv): {}", output_video_path.display());
            } else {
                 println!("No videos successfully processed to create output file {}", output_video_path.display());
                 // Consider deleting the potentially empty/invalid output file
                 let _ = fs::remove_file(&output_video_path); 
            }
        } else {
             println!("VideoWriter was never initialized for directory {}. No output file created.", input_dir_path);
        }

    } else if creation_mode == "direct" && use_ffmpeg_extraction {
        // --- CASE 2: FFmpeg Extract + Direct Create (via intermediate Vec<Mat>) ---
        println!("Using ffmpeg extraction with direct creation (via intermediate files).");
        let temp_dir_for_extraction: PathBuf; 

        // Create unique transient temp dir for ffmpeg output
        let dir_name = format!("{}_{}_ffmpeg_direct_temp_{:?}", config.output_prefix, dir_tag, thread::current().id());
        let temp_path = output_base.join(dir_name);
        fs::create_dir_all(&temp_path)?;
        temp_dirs_created.lock().unwrap().push(temp_path.clone()); // Track for cleanup
        temp_dir_for_extraction = temp_path;
        let should_cleanup_extraction_dir = true;
        println!("Created transient temp directory for ffmpeg: {}", temp_dir_for_extraction.display());

        let mut extraction_ok = true;
        for (video_index, video_path) in sorted_video_list.iter().enumerate() {
             println!(
                "  Thread {:?} extracting via ffmpeg from video {}/{}: {}",
                thread::current().id(), video_index + 1, sorted_video_list.len(), video_path.display()
            );
            match extract_frames_ffmpeg(
                video_path.to_str().unwrap(),
                video_index,
                temp_dir_for_extraction.to_str().unwrap(),
                config.frame_interval,
            ) {
                Ok(_) => { /* FFmpeg extraction ok */ }
                Err(e) => {
                    eprintln!("Error extracting frames via ffmpeg for video {}: {}", video_path.display(), e);
                    extraction_ok = false;
                    break; // Stop processing this directory on error
                }
            }
        }

        if !extraction_ok {
            eprintln!("Skipping direct video creation for directory {} due to ffmpeg extraction errors.", input_dir_path);
             return Err(format!("FFmpeg extraction failed for directory {}", input_dir_path).into());
        } else {
            // --- Load frames from temp dir and create video ---
            println!("Loading frames extracted by ffmpeg from {}", temp_dir_for_extraction.display());
            let mut all_frames_for_dir: Vec<Mat> = Vec::new();
            let mut frame_size: Option<Size> = None;

            let image_files: Vec<fs::DirEntry> = match fs::read_dir(&temp_dir_for_extraction) {
                Ok(reader) => reader.filter_map(|entry| entry.ok()).filter(|entry| {
                        entry.path().is_file() &&
                        entry.path().extension().and_then(|ext| ext.to_str()).map(|ext| ext.eq_ignore_ascii_case("jpg")).unwrap_or(false)
                    }).collect(),
                Err(e) => return Err(format!("Failed to read ffmpeg temp dir {}: {}", temp_dir_for_extraction.display(), e).into()),
            };
            
            if image_files.is_empty() {
                println!("No frames found in ffmpeg temp dir {}. Skipping video creation.", temp_dir_for_extraction.display());
            } else {
                 let mut sorted_image_files = image_files;
                 sorted_image_files.sort_by(|a, b| {
                     let path_a = a.path();
                     let path_b = b.path();
                     let filename_a = path_a.file_stem().and_then(|n| n.to_str()).unwrap_or("");
                     let filename_b = path_b.file_stem().and_then(|n| n.to_str()).unwrap_or("");
                     match (parse_frame_filename(filename_a), parse_frame_filename(filename_b)) {
                        (Some((vid_a, frame_a)), Some((vid_b, frame_b))) => vid_a.cmp(&vid_b).then_with(|| frame_a.cmp(&frame_b)),
                        _ => path_a.cmp(&path_b), // Fallback sort
                     }
                 });

                 for entry in &sorted_image_files {
                    let frame = match imgcodecs::imread(entry.path().to_str().unwrap(), imgcodecs::IMREAD_COLOR) {
                        Ok(f) => f,
                        Err(e) => {
                            eprintln!("Warning: Failed to read frame image {}: {}. Skipping.", entry.path().display(), e);
                            continue;
                        }
                    };
                    if frame.empty() {
                        eprintln!("Warning: Skipping empty frame read from {}", entry.path().display());
                        continue;
                    }
                    if frame_size.is_none() {
                         frame_size = Some(frame.size().unwrap());
                         println!("Determined frame size {:?} from first ffmpeg frame {}", frame_size.unwrap(), entry.path().display());
                    } else if frame.size().unwrap() != frame_size.unwrap() {
                         eprintln!(
                             "Warning: Frame size {:?} from {} differs from expected {:?}. Skipping.",
                             frame.size().unwrap(), entry.path().display(), frame_size.unwrap()
                         );
                        continue;
                    }
                    all_frames_for_dir.push(frame);
                 }
                 
                 // --- Create video from loaded frames ---
                 if let Some(size) = frame_size {
                     if !all_frames_for_dir.is_empty() {
                        println!(
                            "Thread {:?} creating summary (direct/ffmpeg) for directory {}: {}",
                            thread::current().id(),
                            input_dir_path,
                            output_video_path.display()
                        );
                        let fourcc_ffmpeg = videoio::VideoWriter::fourcc('a', 'v', 'c', '1')?;
                        let mut writer_ffmpeg = videoio::VideoWriter::new(
                            output_video_path.to_str().unwrap(),
                            fourcc_ffmpeg,
                            config.output_fps as f64,
                            size,
                            true, // isColor
                        )?;

                        if !writer_ffmpeg.is_opened()? {
                             eprintln!("Error: Failed to open VideoWriter for ffmpeg direct output {}.", output_video_path.display());
                             return Err(format!("Failed to open VideoWriter for {}", output_video_path.display()).into());
                        }

                        for frame in all_frames_for_dir {
                             if let Err(e) = writer_ffmpeg.write(&frame) {
                                  eprintln!("Error writing frame during ffmpeg direct creation: {}. Aborting directory.", e);
                                  let _ = writer_ffmpeg.release();
                                  return Err(format!("VideoWriter write error: {}", e).into());
                             }
                        }
                        writer_ffmpeg.release()?;
                        println!("Successfully created video (direct/ffmpeg): {}", output_video_path.display());
                     } else {
                         println!("No valid frames loaded after ffmpeg extraction for directory {}, skipping direct video creation.", input_dir_path);
                     }
                 } else {
                      println!("Could not determine frame size for directory {} after ffmpeg extraction, skipping direct video creation.", input_dir_path);
                 }
            } // End if !image_files.is_empty()
        } // End successful extraction block

        // Clean up the transient temp dir used for ffmpeg extract + direct create
        if should_cleanup_extraction_dir {
            println!("Cleaning up transient ffmpeg temp dir: {}", temp_dir_for_extraction.display());
            // Remove from global list first to avoid double-remove attempt at the end
            temp_dirs_created.lock().unwrap().retain(|p| p != &temp_dir_for_extraction);
            if let Err(e) = fs::remove_dir_all(&temp_dir_for_extraction) {
                 eprintln!("Warning: Failed to remove transient temp dir {}: {}", temp_dir_for_extraction.display(), e);
            }
        }
    } else { // creation_mode == "temp_frames"
        // --- CASE 3: Temp Frames Create (OpenCV or FFmpeg Extract) ---
        println!("Using temp_frames creation mode.");
        // Create persistent temp dir
        let dir_name = format!("{}_{}_temp", config.output_prefix, dir_tag);
        let temp_dir_for_creation = output_base.join(dir_name);
        fs::create_dir_all(&temp_dir_for_creation)?;
        temp_dirs_created.lock().unwrap().push(temp_dir_for_creation.clone()); // Track for cleanup
        
        let mut extraction_ok_temp = true;
        for (video_index, video_path) in sorted_video_list.iter().enumerate() {
            println!(
                "  Thread {:?} extracting frames to temp dir from video {}/{}: {}",
                thread::current().id(), video_index + 1, sorted_video_list.len(), video_path.display()
            );
            // Call the correct function directly based on mode
            let result = if use_ffmpeg_extraction {
                extract_frames_ffmpeg(
                    video_path.to_str().unwrap(),
                    video_index,
                    temp_dir_for_creation.to_str().unwrap(),
                    config.frame_interval,
                )
            } else { 
                extract_frames_opencv(
                    video_path.to_str().unwrap(),
                    video_index,
                    temp_dir_for_creation.to_str().unwrap(),
                    config.frame_interval,
                    "opencv" // Pass the required mode string
                )
            };

            // Handle the result
            if let Err(e) = result {
                eprintln!("Error extracting frames ({}) for temp mode for video {}: {}", 
                    if use_ffmpeg_extraction {"ffmpeg"} else {"opencv"},
                    video_path.display(), 
                    e
                );
                extraction_ok_temp = false;
                break; 
            }
        }

        if !extraction_ok_temp {
            eprintln!("Skipping temp_frames video creation for directory {} due to extraction errors.", input_dir_path);
             return Err(format!("Extraction failed for temp_frames directory {}", input_dir_path).into());
        } else {
            // --- Create video from temp frames using ffmpeg concat ---
            println!(
                "Thread {:?} creating summary (temp_frames) for directory {}: {}",
                thread::current().id(),
                input_dir_path,
                output_video_path.display()
            );
            match create_video_from_temp_frames(
                temp_dir_for_creation.to_str().unwrap(),
                &output_video_path,
                config.output_fps,
            ) {
                Ok(_) => { 
                     println!("Successfully created video (temp_frames): {}", output_video_path.display());
                }
                Err(e) => {
                    eprintln!("Error creating summary video (temp_frames) for directory {}: {}", input_dir_path, e);
                    return Err(format!("Failed creating summary (temp_frames) for {}: {}", input_dir_path, e).into());
                }
            }
        }
    } // End of main if/else if/else block for modes
    Ok(())
} 