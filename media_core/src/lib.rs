use chrono::Local;
use opencv::{prelude::*, videoio, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum SavingOption {
    Single,
    List,
    Both,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CaptureConfig {
    pub rtsp_url: String,
    pub rtsp_url_list: Vec<String>,
    pub output_directory: String,
    pub show_preview: bool,
    pub saving_option: SavingOption,
    pub saved_time_duration: u64,
    pub use_fps: bool,
    pub fps: f64,
}

pub struct RTSPCapture {
    pub url: String,
    pub output_dir: String,
    pub show_preview: bool,
    pub capture: Option<videoio::VideoCapture>,
    pub writer: Option<videoio::VideoWriter>,
    pub ffmpeg_process: Option<Child>,
    pub current_file_start: Instant,
    pub segment_duration: Duration,
    pub use_custom_fps: bool,
    pub custom_fps: f64,
}

impl RTSPCapture {
    pub fn new(
        url: String,
        output_dir: String,
        show_preview: bool,
        segment_duration_secs: u64,
        use_custom_fps: bool,
        custom_fps: f64,
    ) -> Result<Self> {
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

    pub fn start_ffmpeg_recording(&mut self) -> std::io::Result<()> {
        // Create camera-specific output directory
        let camera_dir = PathBuf::from(&self.output_dir).join(format!(
            "camera_{}",
            self.url
                .replace("://", "_")
                .replace("/", "_")
                .replace(":", "_")
        ));
        fs::create_dir_all(&camera_dir)?;

        // Prepare FFmpeg command
        let output_pattern = camera_dir
            .join("segment_%Y%m%d_%H%M%S.mp4")
            .to_str()
            .unwrap()
            .to_string();

        let mut command = Command::new("ffmpeg");
        command.args([
            "-y",
            "-loglevel",
            "error", // Reduce log noise
            "-rtsp_transport",
            "tcp",
            "-use_wallclock_as_timestamps",
            "1", // Use system clock for timestamps
            "-i",
            &self.url,
            "-c:v",
            "copy", // Copy video stream directly
            "-an",  // Remove audio
            "-f",
            "segment",
            "-segment_time",
            &self.segment_duration.as_secs().to_string(),
            "-segment_format",
            "mp4",
            "-reset_timestamps",
            "1",
            "-segment_format_options",
            "movflags=+faststart+frag_keyframe+empty_moov+default_base_moof",
            "-segment_time_delta",
            "0.05", // Small delta to handle rounding
            "-strftime",
            "1",
            "-reconnect_at_eof",
            "1", // Reconnect if stream ends
            "-reconnect_streamed",
            "1", // Reconnect if stream fails
            "-reconnect_delay_max",
            "120", // Maximum reconnection delay
            &output_pattern,
        ]);

        println!("Starting FFmpeg with command: {:?}", command);

        // Start FFmpeg process with proper buffer handling
        let process = command
            .stdin(Stdio::null()) // Don't need stdin
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        self.ffmpeg_process = Some(process);
        Ok(())
    }

    pub fn start_opencv_recording(&mut self) -> Result<()> {
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

    pub fn process_stream(&mut self) -> Result<()> {
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

    pub fn process_stream_ffmpeg(&mut self) -> Result<()> {
        let mut consecutive_failures = 0;
        let max_failures = 3; // Maximum number of consecutive failures before longer wait

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
                        println!(
                            "FFmpeg process for {} ended with status: {}",
                            self.url, status
                        );
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
                        consecutive_failures = 0; // Reset failure count while running
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

    pub fn process_stream_opencv(&mut self) -> Result<()> {
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

                    // Show preview window
                    if let Some(window_name) = &window {
                        opencv::highgui::imshow(window_name, &frame)?;
                    }
                } else {
                    // End of stream or error, break the loop
                    break;
                }
            } else {
                break;
            }

            // Wait for a short duration
            if self.show_preview {
                let key = opencv::highgui::wait_key(1)?;
                if key == 27 {
                    // ESC key
                    break;
                }
            } else {
                thread::sleep(Duration::from_millis(10)); // Adjust as needed
            }
        }

        if let Some(window_name) = &window {
            opencv::highgui::destroy_window(window_name)?;
        }

        Ok(())
    }

    pub fn create_new_video_file(&mut self) -> Result<()> {
        // Release previous writer
        if let Some(mut writer) = self.writer.take() {
            writer.release()?;
        }

        // Create camera-specific output directory
        let camera_dir = PathBuf::from(&self.output_dir).join(format!(
            "camera_{}",
            self.url
                .replace("://", "_")
                .replace("/", "_")
                .replace(":", "_")
        ));
        fs::create_dir_all(&camera_dir).map_err(|e| {
            opencv::Error::new(
                opencv::core::StsError,
                &format!("Failed to create directory: {}", e),
            )
        })?;

        // Create new file name with timestamp
        let timestamp = Local::now().format("%Y%m%d_%H%M%S");
        let file_name = camera_dir.join(format!("segment_{}.mp4", timestamp));

        // Get video properties from capture
        if let Some(capture) = &self.capture {
            let frame_width = capture.get(videoio::CAP_PROP_FRAME_WIDTH)? as i32;
            let frame_height = capture.get(videoio::CAP_PROP_FRAME_HEIGHT)? as i32;
            let stream_fps = capture.get(videoio::CAP_PROP_FPS)?;
            
            let fps = if self.use_custom_fps {
                self.custom_fps
            } else if stream_fps > 0.0 {
                stream_fps
            } else {
                30.0 // Default fallback
            };

            // Create new video writer with MP4V codec
            let fourcc = videoio::VideoWriter::fourcc('m', 'p', '4', 'v')?;
            let writer = videoio::VideoWriter::new(
                file_name.to_str().unwrap(),
                fourcc,
                fps,
                (frame_width, frame_height).into(),
                true,
            )?;

            if !writer.is_opened()? {
                return Err(opencv::Error::new(
                    opencv::core::StsError,
                    "Failed to create video writer",
                ));
            }

            self.writer = Some(writer);
            self.current_file_start = Instant::now();
        }

        Ok(())
    }
}
