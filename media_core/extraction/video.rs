use opencv::{
    core::{Vector, Mat},
    imgcodecs,
    prelude::*,
    videoio::{VideoCapture, VideoCaptureAPIs::CAP_ANY, CAP_PROP_FRAME_COUNT},
};
use std::error::Error;
use std::fs::{self, DirEntry};
use std::path::Path;
use std::process::Command;
use std::io::Write;
use std::cmp::Ordering as CmpOrdering;

pub fn extract_frames_opencv(
    video_filename: &str,
    video_index: usize,
    temp_frame_dir: &str,
    frame_interval: usize,
    extraction_mode: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    fs::create_dir_all(temp_frame_dir)?;
    
    match extraction_mode {
        "ffmpeg" | "ffmpeg_interval" => {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "FFmpeg extraction mode is currently blocked in main workflow.",
            )));
        }
        _ => {
            let mut cap = VideoCapture::from_file(video_filename, CAP_ANY.into())?;
            if !cap.is_opened()? {
                return Err(format!("Failed to open video: {}", video_filename).into());
            }
            let total_frames = cap.get(CAP_PROP_FRAME_COUNT)? as usize;

            for frame_number in (0..total_frames).step_by(frame_interval) {
                let mut frame = Mat::default();
                if !cap.set(opencv::videoio::CAP_PROP_POS_FRAMES, frame_number as f64)? {
                     eprintln!("Warning: Failed to seek to frame {} in {}", frame_number, video_filename);
                     continue;
                }

                if cap.read(&mut frame)? {
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
                    imgcodecs::imwrite(&output_path, &frame, &Vector::new())?;
                } else {
                    break;
                }
            }
        }
    }
    Ok(())
}

pub fn extract_frames_ffmpeg(
    video_filename: &str,
    video_index: usize,
    temp_frame_dir: &str,
    frame_interval: usize,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    fs::create_dir_all(temp_frame_dir)?;

    if frame_interval == 0 {
        return Err("frame_interval must be greater than 0 for ffmpeg extraction.".into());
    }

    let output_pattern = Path::new(temp_frame_dir)
        .join(format!("video{}_frame%06d.jpg", video_index));
    let output_pattern_str = output_pattern.to_str().ok_or("Invalid output path pattern")?;

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

    let output = cmd.output()?;

    if !output.status.success() {
        eprintln!("ffmpeg stdout: {}", String::from_utf8_lossy(&output.stdout));
        eprintln!("ffmpeg stderr: {}", String::from_utf8_lossy(&output.stderr));
        return Err(format!(
            "ffmpeg frame extraction failed for video {}",
            video_filename
        )
        .into());
    }

    println!(
        "Successfully extracted frames using ffmpeg for video {}: {}",
        video_index,
        video_filename
    );
    Ok(())
}

pub fn create_video_from_temp_frames(
    temp_frame_dir: &str, 
    output_video_path: &Path, 
    fps: i32
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let frame_source_dir = Path::new(temp_frame_dir);
    let final_output_dir = output_video_path.parent().unwrap_or_else(|| Path::new("."));

    fs::create_dir_all(final_output_dir)?;

    if !frame_source_dir.exists() {
        eprintln!("Warning: Temporary frame directory {} does not exist. Skipping video creation.", temp_frame_dir);
        return Ok(());
    }

    let mut image_files: Vec<DirEntry> = match fs::read_dir(frame_source_dir) {
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

    image_files.sort_by(|a, b| {
        let path_a = a.path();
        let path_b = b.path();
        let filename_a = path_a.file_stem().and_then(|n| n.to_str()).unwrap_or("");
        let filename_b = path_b.file_stem().and_then(|n| n.to_str()).unwrap_or("");

        match (parse_frame_filename(filename_a), parse_frame_filename(filename_b)) {
            (Some((vid_a, frame_a)), Some((vid_b, frame_b))) => {
                vid_a.cmp(&vid_b).then_with(|| frame_a.cmp(&frame_b))
            }
            (Some(_), None) => CmpOrdering::Less,
            (None, Some(_)) => CmpOrdering::Greater,
            (None, None) => CmpOrdering::Equal,
        }
    });

    let list_file_path = frame_source_dir.join("ffmpeg_list.txt");
    {
        let mut list_file = fs::File::create(&list_file_path)?;
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
        list_file.flush()?;
    }

    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-y")
        .arg("-f")
        .arg("concat")
        .arg("-safe")
        .arg("0")
        .arg("-i")
        .arg(list_file_path.to_str().unwrap())
        .arg("-c:v")
        .arg("libx264")
        .arg("-pix_fmt")
        .arg("yuv420p")
        .arg("-r")
        .arg(fps.to_string())
        .arg(output_video_path.to_str().unwrap());

    println!("Running ffmpeg for: {}", output_video_path.display());
    let output = cmd.output()?;
    
    if !output.status.success() {
        let _ = fs::remove_file(&list_file_path); 
        eprintln!("ffmpeg stdout: {}", String::from_utf8_lossy(&output.stdout));
        eprintln!("ffmpeg stderr: {}", String::from_utf8_lossy(&output.stderr));
        return Err(format!(
            "ffmpeg failed to create video from temp frames for {}",
            output_video_path.display()
        )
        .into());
    }

    let _ = fs::remove_file(&list_file_path);
    Ok(())
}

pub fn create_final_summary_video(
    parent_dir: &Path,
    temp_dir_pattern: &str,
    final_output_path: &Path,
    fps: i32,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    println!("Starting final summary video creation...");
    let mut all_image_files = Vec::new();

    for entry in fs::read_dir(parent_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
                if dir_name.starts_with(&temp_dir_pattern.replace("*", "")) {
                    println!("Collecting frames from: {}", path.display());
                    let mut image_files: Vec<std::path::PathBuf> = fs::read_dir(&path)?
                        .filter_map(|entry| entry.ok())
                        .map(|entry| entry.path())
                        .filter(|p| p.is_file() && p.extension().map_or(false, |ext| ext == "jpg"))
                        .collect();
                    all_image_files.append(&mut image_files);
                }
            }
        }
    }

    if all_image_files.is_empty() {
        println!("No frames found to create a final summary video.");
        return Ok(());
    }

    all_image_files.sort();

    let list_file_path = parent_dir.join("ffmpeg_final_list.txt");
    {
        let mut list_file = fs::File::create(&list_file_path)?;
        for path in &all_image_files {
             let path_str = path.to_string_lossy().replace("\\", "/");
             writeln!(list_file, "file '{}'", path_str)?;
        }
    }

    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-y")
        .arg("-f")
        .arg("concat")
        .arg("-safe")
        .arg("0")
        .arg("-i")
        .arg(&list_file_path)
        .arg("-c:v")
        .arg("libx264")
        .arg("-pix_fmt")
        .arg("yuv420p")
        .arg("-r")
        .arg(fps.to_string())
        .arg(final_output_path);
    
    println!("Running ffmpeg for final summary: {}", final_output_path.display());
    let output = cmd.output()?;

    if !output.status.success() {
        let _ = fs::remove_file(&list_file_path);
        eprintln!("ffmpeg stdout: {}", String::from_utf8_lossy(&output.stdout));
        eprintln!("ffmpeg stderr: {}", String::from_utf8_lossy(&output.stderr));
        return Err("ffmpeg failed to create the final summary video".into());
    }

    let _ = fs::remove_file(&list_file_path);
    println!("Final summary video created successfully: {}", final_output_path.display());

    Ok(())
}

pub fn get_video_duration(filename: &str) -> Result<f64, Box<dyn Error>> {
    let output = Command::new("ffprobe")
        .args(&[
            "-v", "error",
            "-show_entries", "format=duration",
            "-of", "default=noprint_wrappers=1:nokey=1",
            filename,
        ])
        .output()?;

    if !output.status.success() {
        return Err(format!(
            "ffprobe failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }

    let duration_str = String::from_utf8(output.stdout)?.trim().to_string();
    let duration = duration_str.parse::<f64>()?;
    Ok(duration)
}

pub fn parse_frame_filename(filename: &str) -> Option<(usize, usize)> {
    let parts: Vec<&str> = filename.split('_').collect();
    if parts.len() == 2 && parts[0].starts_with("video") && parts[1].starts_with("frame") {
        let video_num = parts[0].replace("video", "").parse().ok()?;
        let frame_num = parts[1].replace("frame", "").parse().ok()?;
        Some((video_num, frame_num))
    } else {
        None
    }
} 