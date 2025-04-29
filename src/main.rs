use opencv::{prelude::*, videoio, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use std::thread;
use std::process::{Command, Stdio, Child};
use chrono::Local;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum SavingOption {
    Single,
    List,
    Both,
}

#[derive(Serialize, Deserialize)]
struct Config {
    rtsp_url: String,
    rtsp_url_list: Vec<String>,
    output_directory: String,
    show_preview: bool,
    saving_option: SavingOption,
    saved_time_duration: u64,
    use_fps: bool,
    fps: f64,
}

struct RTSPCapture {
    url: String,
    output_dir: String,
    show_preview: bool,
    capture: Option<videoio::VideoCapture>,
    writer: Option<videoio::VideoWriter>,
    ffmpeg_process: Option<Child>,
    current_file_start: Instant,
    segment_duration: Duration,
    use_custom_fps: bool,
    custom_fps: f64,
}

impl RTSPCapture {
    fn new(url: String, output_dir: String, show_preview: bool, segment_duration_secs: u64, 
           use_custom_fps: bool, custom_fps: f64) -> Result<Self> {
        Ok(Self {
            url,
            output_dir,
            show_preview,
            capture: None,
            writer: None,
            ffmpeg_process: None,
            current_file_start: Instant::now(),
            segment_duration: Duration::from_secs(segment_duration_secs),
            use_custom_fps,
            custom_fps,
        })
    }

    fn start_ffmpeg_recording(&mut self) -> std::io::Result<()> {
        // Create camera-specific output directory
        let camera_dir = PathBuf::from(&self.output_dir)
            .join(format!("camera_{}", self.url.replace("://", "_").replace("/", "_").replace(":", "_")));
        fs::create_dir_all(&camera_dir)?;

        // Prepare FFmpeg command
        let output_pattern = camera_dir.join("segment_%Y%m%d_%H%M%S.mp4")
            .to_str()
            .unwrap()
            .to_string();

        let mut command = Command::new("ffmpeg");
        command.args([
            "-y",
            "-loglevel", "warning",  // Reduce log noise
            "-rtsp_transport", "tcp",
            "-use_wallclock_as_timestamps", "1",  // Use system clock for timestamps
            "-i", &self.url,
            "-c:v", "copy",  // Copy video stream directly
            "-an",  // Remove audio
            "-f", "segment",
            "-segment_time", &self.segment_duration.as_secs().to_string(),
            "-segment_format", "mp4",
            "-reset_timestamps", "1",
            "-segment_format_options", "movflags=+faststart+frag_keyframe+empty_moov+default_base_moof",
            "-segment_time_delta", "0.05",  // Small delta to handle rounding
            "-strftime", "1",
            "-reconnect_at_eof", "1",       // Reconnect if stream ends
            "-reconnect_streamed", "1",     // Reconnect if stream fails
            "-reconnect_delay_max", "120",  // Maximum reconnection delay
            &output_pattern,
        ]);

        println!("Starting FFmpeg with command: {:?}", command);

        // Start FFmpeg process with proper buffer handling
        let process = command
            .stdin(Stdio::null())  // Don't need stdin
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        self.ffmpeg_process = Some(process);
        Ok(())
    }

    fn start_opencv_recording(&mut self) -> Result<()> {
        // Create capture with FFMPEG backend for better control
        let mut capture = videoio::VideoCapture::from_file(&self.url, videoio::CAP_FFMPEG)?;
        
        if !capture.is_opened()? {
            return Err(opencv::Error::new(
                opencv::core::StsError,
                "Failed to open RTSP stream",
            ));
        }

        // Get the stream's FPS
        let stream_fps = capture.get(videoio::CAP_PROP_FPS)?;
        let actual_fps = if stream_fps <= 0.0 {
            if self.use_custom_fps {
                self.custom_fps
            } else {
                30.0 // Default fallback
            }
        } else {
            stream_fps
        };

        println!("Stream FPS: {}", actual_fps);

        // Optimize for video-only capture
        let _ = capture.set(videoio::CAP_PROP_CONVERT_RGB, 1.0);

        self.capture = Some(capture);
        Ok(())
    }

    fn process_stream(&mut self) -> Result<()> {
        if self.use_custom_fps {
            // Use OpenCV for custom FPS recording
            self.start_opencv_recording()?;
            self.process_stream_opencv()
        } else {
            // Use FFmpeg for direct stream copying
            self.start_ffmpeg_recording().map_err(|e| {
                opencv::Error::new(
                    opencv::core::StsError,
                    &format!("Failed to start FFmpeg: {}", e),
                )
            })?;
            self.process_stream_ffmpeg()
        }
    }

    fn process_stream_ffmpeg(&mut self) -> Result<()> {
        let mut consecutive_failures = 0;
        let max_failures = 3;  // Maximum number of consecutive failures before longer wait

        loop {
            if self.ffmpeg_process.is_none() {
                // Start a new FFmpeg process if none exists
                match self.start_ffmpeg_recording() {
                    Ok(_) => {
                        println!("Successfully started FFmpeg process for {}", self.url);
                        consecutive_failures = 0;
                    }
                    Err(e) => {
                        eprintln!("Failed to start FFmpeg for {}: {}", self.url, e);
                        consecutive_failures += 1;
                        if consecutive_failures >= max_failures {
                            // Wait longer if we've had multiple failures
                            thread::sleep(Duration::from_secs(10));
                        } else {
                            thread::sleep(Duration::from_secs(1));
                        }
                        continue;
                    }
                }
            }

            if let Some(process) = &mut self.ffmpeg_process {
                match process.try_wait() {
                    Ok(Some(status)) => {
                        // Process has finished
                        println!("FFmpeg process for {} ended with status: {}", self.url, status);
                        if !status.success() {
                            eprintln!("FFmpeg process failed for {}, restarting...", self.url);
                            consecutive_failures += 1;
                        }
                        self.ffmpeg_process = None;
                        
                        if consecutive_failures >= max_failures {
                            thread::sleep(Duration::from_secs(10));
                        } else {
                            thread::sleep(Duration::from_secs(1));
                        }
                    }
                    Ok(None) => {
                        // Process is still running
                        consecutive_failures = 0;  // Reset failure count while running
                        thread::sleep(Duration::from_secs(1));
                    }
                    Err(e) => {
                        eprintln!("Error checking FFmpeg process for {}: {}", self.url, e);
                        self.ffmpeg_process = None;
                        consecutive_failures += 1;
                        if consecutive_failures >= max_failures {
                            thread::sleep(Duration::from_secs(10));
                        } else {
                            thread::sleep(Duration::from_secs(1));
                        }
                    }
                }
            }
        }
    }

    fn process_stream_opencv(&mut self) -> Result<()> {
        let window = if self.show_preview {
            let window_name = format!("RTSP Stream - {}", self.url);
            opencv::highgui::named_window(&window_name, opencv::highgui::WINDOW_AUTOSIZE)?;
            Some(window_name)
        } else {
            None
        };

        let mut frame = Mat::default();

        // Create first video file
        self.create_new_video_file()?;

        loop {
            let current_time = Instant::now();
            let segment_elapsed = current_time.duration_since(self.current_file_start);

            // Check if we need to start a new segment
            if segment_elapsed >= self.segment_duration {
                self.create_new_video_file()?;
                continue;
            }

            // Read new frame if available
            if let Some(capture) = &mut self.capture {
                let frame_read = capture.read(&mut frame)?;
                
                if frame_read && !frame.empty() {
                    // Write frame to file
                    if let Some(writer) = &mut self.writer {
                        writer.write(&frame)?;
                    }

                    // Show preview if enabled
                    if let Some(window) = &window {
                        opencv::highgui::imshow(window, &frame)?;
                        let key = opencv::highgui::wait_key(1)?;
                        if key == 27 {  // ESC key
                            return Ok(());
                        }
                    }
                } else {
                    // If frame read failed, wait a bit before trying again
                    thread::sleep(Duration::from_millis(10));
                }
            }
        }
    }

    fn create_new_video_file(&mut self) -> Result<()> {
        if !self.use_custom_fps {
            return Ok(()); // FFmpeg handles file creation for direct stream copy
        }

        // Create camera-specific output directory
        let camera_dir = PathBuf::from(&self.output_dir)
            .join(format!("camera_{}", self.url.replace("://", "_").replace("/", "_").replace(":", "_")));
        
        fs::create_dir_all(&camera_dir).map_err(|e| {
            opencv::Error::new(
                opencv::core::StsError,
                &format!("Failed to create output directory: {}", e),
            )
        })?;

        // Generate filename with timestamp
        let timestamp = Local::now().format("%Y%m%d_%H%M%S");
        let filename = format!("segment_{}.mp4", timestamp);
        let filepath = camera_dir.join(filename);

        // Get video properties from capture
        let fps = if self.use_custom_fps {
            self.custom_fps
        } else {
            if let Some(capture) = &self.capture {
                let src_fps = capture.get(videoio::CAP_PROP_FPS)?;
                if src_fps <= 0.0 {
                    30.0 // Fallback FPS
                } else {
                    src_fps
                }
            } else {
                30.0
            }
        };

        let width = self.capture.as_ref().unwrap().get(videoio::CAP_PROP_FRAME_WIDTH)? as i32;
        let height = self.capture.as_ref().unwrap().get(videoio::CAP_PROP_FRAME_HEIGHT)? as i32;

        // Create VideoWriter with appropriate settings
        let fourcc = videoio::VideoWriter::fourcc('m', 'p', '4', 'v')?;
        let mut writer = videoio::VideoWriter::new(
            filepath.to_str().unwrap(),
            fourcc,
            fps,
            opencv::core::Size::new(width, height),
            true, // isColor
        )?;

        if !writer.is_opened()? {
            return Err(opencv::Error::new(
                opencv::core::StsError,
                "Failed to create video writer",
            ));
        }

        self.writer = Some(writer);
        self.current_file_start = Instant::now();

        println!("Created new video segment for {} (FPS: {}): {}", 
            self.url, 
            fps,
            filepath.display()
        );
        Ok(())
    }
}

fn main() -> Result<()> {
    // Read and parse config file
    let config_content = fs::read_to_string("config.json").map_err(|e| {
        opencv::Error::new(
            opencv::core::StsError,
            &format!("Failed to read config file: {}", e),
        )
    })?;

    let config: Config = serde_json::from_str(&config_content).map_err(|e| {
        opencv::Error::new(
            opencv::core::StsError,
            &format!("Failed to parse config file: {}", e),
        )
    })?;

    // Create a vector to store all thread handles
    let mut handles = vec![];

    // Clone common values that will be used across threads
    let output_directory = config.output_directory;
    let saved_time_duration = config.saved_time_duration;
    let use_fps = config.use_fps;
    let fps = config.fps;

    match config.saving_option {
        SavingOption::Single => {
            let url = config.rtsp_url;
            let show_preview = config.show_preview;
            let output_dir = output_directory.clone();
            
            handles.push(thread::spawn(move || {
                if let Err(e) = RTSPCapture::new(
                    url.clone(),
                    output_dir,
                    show_preview,
                    saved_time_duration,
                    use_fps,
                    fps,
                ).and_then(|mut capture| capture.process_stream()) {
                    eprintln!("Error processing stream {}: {}", url, e);
                }
            }));
        }
        SavingOption::Both => {
            // Process single URL
            {
                let url = config.rtsp_url;
                let output_dir = output_directory.clone();
                
                handles.push(thread::spawn(move || {
                    if let Err(e) = RTSPCapture::new(
                        url.clone(),
                        output_dir,
                        false,
                        saved_time_duration,
                        use_fps,
                        fps,
                    ).and_then(|mut capture| capture.process_stream()) {
                        eprintln!("Error processing stream {}: {}", url, e);
                    }
                }));
            }

            // Process list URLs
            for url in config.rtsp_url_list {
                let output_dir = output_directory.clone();
                
                handles.push(thread::spawn(move || {
                    if let Err(e) = RTSPCapture::new(
                        url.clone(),
                        output_dir,
                        false,
                        saved_time_duration,
                        use_fps,
                        fps,
                    ).and_then(|mut capture| capture.process_stream()) {
                        eprintln!("Error processing stream {}: {}", url, e);
                    }
                }));
            }
        }
        SavingOption::List => {
            // Process URLs from the list without preview
            for url in config.rtsp_url_list {
                let output_dir = output_directory.clone();
                
                handles.push(thread::spawn(move || {
                    if let Err(e) = RTSPCapture::new(
                        url.clone(),
                        output_dir,
                        false,
                        saved_time_duration,
                        use_fps,
                        fps,
                    ).and_then(|mut capture| capture.process_stream()) {
                        eprintln!("Error processing stream {}: {}", url, e);
                    }
                }));
            }
        }
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.join().unwrap();
    }

    Ok(())
}
