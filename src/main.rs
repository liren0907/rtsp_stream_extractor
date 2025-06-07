use media_core::{CaptureConfig, RTSPCapture, SavingOption};
use serde_json;
use std::error::Error;
use std::fs::File;
use std::io::BufReader;
use std::thread;

fn main() -> Result<(), Box<dyn Error>> {
    // Load configuration from file
    let config_file = File::open("config.json")?;
    let reader = BufReader::new(config_file);
    let config: CaptureConfig = serde_json::from_reader(reader)?;

    let mut handles = vec![];

    let (urls_to_process, show_preview_for_list) = match config.saving_option {
        SavingOption::Single => (vec![config.rtsp_url.clone()], config.show_preview),
        SavingOption::List => (config.rtsp_url_list.clone(), false),
        SavingOption::Both => {
            let mut urls = vec![config.rtsp_url.clone()];
            urls.extend(config.rtsp_url_list.clone());
            (urls, false)
        }
    };

    for url in urls_to_process {
        let output_dir = config.output_directory.clone();
        // For 'Both' and 'List', show_preview is false for all streams.
        // For 'Single', it depends on the config.
        let show_preview = if config.rtsp_url == url {
            show_preview_for_list
        } else {
            false
        };
        let segment_duration = config.saved_time_duration;
        let use_fps = config.use_fps;
        let fps = config.fps;

        let handle = thread::spawn(move || {
            match RTSPCapture::new(
                url.clone(),
                output_dir,
                show_preview,
                segment_duration,
                use_fps,
                fps,
            ) {
                Ok(mut capture) => {
                    println!("Processing stream: {}", url);
                    if let Err(e) = capture.process_stream() {
                        eprintln!("Error processing stream {}: {:?}", url, e);
                    }
                }
                Err(e) => {
                    eprintln!("Failed to create RTSP capture for {}: {:?}", url, e);
                }
            }
        });
        handles.push(handle);
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.join().unwrap();
    }

    Ok(())
}
