use media_core::{CaptureConfig, RTSPCapture, SavingOption};
use media_core::process::create_video_processor;
use serde_json;
use std::error::Error;
use std::fs::File;
use std::io::BufReader;
use std::thread;
use std::env;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();
    
    if args.len() < 2 {
        print_usage();
        return Ok(());
    }

    match args[1].as_str() {
        "rtsp" => run_rtsp_mode()?,
        "process" => {
            if args.len() < 3 {
                println!("Error: Process mode requires a config file path");
                println!("Usage: cargo run process <config_file_path>");
                return Ok(());
            }
            run_process_mode(&args[2])?;
        },
        "help" | "--help" | "-h" => print_usage(),
        _ => {
            println!("Error: Unknown mode '{}'", args[1]);
            print_usage();
        }
    }

    Ok(())
}

fn print_usage() {
    println!("Media Core - RTSP Stream Extractor");
    println!();
    println!("USAGE:");
    println!("    cargo run <MODE> [OPTIONS]");
    println!();
    println!("MODES:");
    println!("    rtsp                    Run RTSP stream capture mode");
    println!("    process <config_file>   Run video processing mode");
    println!("    help                    Show this help message");
    println!();
    println!("EXAMPLES:");
    println!("    cargo run rtsp                           # Capture RTSP streams using config.json");
    println!("    cargo run process video_config.json     # Process videos using video config");
    println!("    cargo run help                           # Show help");
}

/// Run RTSP stream capture mode (original functionality)
fn run_rtsp_mode() -> Result<(), Box<dyn Error>> {
    println!("🎥 Starting RTSP Stream Capture Mode...");
    
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

    println!("📡 Processing {} RTSP stream(s)...", urls_to_process.len());

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
                    println!("📹 Processing stream: {}", url);
                    if let Err(e) = capture.process_stream() {
                        eprintln!("❌ Error processing stream {}: {:?}", url, e);
                    }
                }
                Err(e) => {
                    eprintln!("❌ Failed to create RTSP capture for {}: {:?}", url, e);
                }
            }
        });
        handles.push(handle);
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.join().unwrap();
    }

    println!("✅ RTSP stream capture completed!");
    Ok(())
}

/// Run video processing mode (new Process module functionality)
fn run_process_mode(config_path: &str) -> Result<(), Box<dyn Error>> {
    println!("🎬 Starting Video Processing Mode...");
    println!("📄 Using config file: {}", config_path);
    
    // Create a video processor
    let mut processor = create_video_processor()?;
    
    // Run video extraction with the provided config
    match processor.run_video_extraction(config_path) {
        Ok(_) => {
            println!("✅ Video processing completed successfully!");
            
            // Print processing statistics
            let stats = processor.get_stats();
            println!("📊 Processing Statistics:");
            println!("   • Files processed: {}", stats.files_processed);
            println!("   • Files failed: {}", stats.files_failed);
            println!("   • Success rate: {:.2}%", stats.success_rate());
            println!("   • Processing time: {:?}", stats.processing_time);
            
            if !stats.errors.is_empty() {
                println!("⚠️  Errors encountered:");
                for error in &stats.errors {
                    println!("   • {}", error);
                }
            }
        },
        Err(e) => {
            eprintln!("❌ Video processing failed: {}", e);
            return Err(Box::new(e));
        }
    }
    
    Ok(())
}
