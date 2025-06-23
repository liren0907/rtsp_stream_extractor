use opencv::videoio::{VideoCapture, VideoCaptureAPIs::*, VideoCaptureProperties::*};
use opencv::prelude::{VideoCaptureTraitConst, VideoCaptureTrait};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct HardwareAccelConfig {
    pub enabled: bool,
    pub mode: String,           // "auto", "apple_silicon", "cuda", "disabled"  
    pub fallback_to_cpu: bool,
    pub prefer_backends: Vec<String>,
}

impl Default for HardwareAccelConfig {
    fn default() -> Self {
        Self {
            enabled: false,  // Default to existing behavior
            mode: "auto".to_string(),
            fallback_to_cpu: true,
            prefer_backends: vec!["any".to_string()],
        }
    }
}

pub struct HardwareAcceleratedCapture;

impl HardwareAcceleratedCapture {
    /// Create VideoCapture with platform-specific hardware acceleration
    pub fn create_capture(
        video_filename: &str, 
        hw_config: &HardwareAccelConfig
    ) -> Result<VideoCapture, Box<dyn std::error::Error>> {
        
        if !hw_config.enabled {
            // Use original implementation - no changes to existing behavior
            println!("🔧 Hardware acceleration disabled - using original CPU implementation");
            return Ok(VideoCapture::from_file(video_filename, CAP_ANY.into())?);
        }

        println!("🚀 Attempting hardware acceleration (mode: {})", hw_config.mode);
        
        let backends = Self::get_platform_backends(hw_config);
        
        for (backend, description) in backends.iter() {
            if let Some(cap) = Self::try_backend(video_filename, *backend, description)? {
                return Ok(cap);
            }
        }

        // Fallback behavior
        if hw_config.fallback_to_cpu {
            println!("🔄 Falling back to CPU processing (original implementation)");
            Ok(VideoCapture::from_file(video_filename, CAP_ANY.into())?)
        } else {
            Err("Hardware acceleration failed and fallback disabled".into())
        }
    }

    fn get_platform_backends(config: &HardwareAccelConfig) -> Vec<(i32, &'static str)> {
        match config.mode.as_str() {
            "apple_silicon" => vec![
                (CAP_AVFOUNDATION.into(), "Apple AVFoundation + VideoToolbox"),
                (CAP_FFMPEG.into(), "FFmpeg + VideoToolbox"),
            ],
            "cuda" => vec![
                (CAP_FFMPEG.into(), "FFmpeg + CUDA"),  
            ],
            "auto" => {
                // Auto-detect platform and return appropriate backends
                if cfg!(target_os = "macos") && Self::is_apple_silicon() {
                    println!("🍎 Detected Apple Silicon - optimizing for VideoToolbox");
                    vec![
                        (CAP_AVFOUNDATION.into(), "Apple AVFoundation + VideoToolbox"),
                        (CAP_FFMPEG.into(), "FFmpeg + VideoToolbox"),
                        (CAP_ANY.into(), "CPU Fallback"),
                    ]
                } else if cfg!(target_os = "linux") {
                    println!("🐧 Detected Linux - optimizing for VAAPI");
                    vec![
                        (CAP_FFMPEG.into(), "FFmpeg + VAAPI"),
                        (CAP_GSTREAMER.into(), "GStreamer + Hardware"),
                        (CAP_ANY.into(), "CPU Fallback"),
                    ]
                } else if cfg!(target_os = "windows") {
                    println!("🪟 Detected Windows - optimizing for Media Foundation");
                    vec![
                        (CAP_MSMF.into(), "Microsoft Media Foundation"),
                        (CAP_FFMPEG.into(), "FFmpeg + D3D11"),
                        (CAP_ANY.into(), "CPU Fallback"),
                    ]
                } else {
                    println!("❓ Unknown platform - using CPU default");
                    vec![(CAP_ANY.into(), "CPU Default")]
                }
            },
            "disabled" => vec![(CAP_ANY.into(), "CPU Default")],
            _ => {
                println!("⚠️  Unknown acceleration mode '{}' - using CPU default", config.mode);
                vec![(CAP_ANY.into(), "CPU Default")]
            }
        }
    }

    fn try_backend(
        video_filename: &str, 
        backend: i32, 
        description: &str
    ) -> Result<Option<VideoCapture>, Box<dyn std::error::Error>> {
        
        println!("🔧 Trying backend: {}", description);
        
        match VideoCapture::from_file(video_filename, backend.into()) {
            Ok(mut cap) => {
                if cap.is_opened()? {
                    // Try to set hardware acceleration properties (OpenCV 4.5.2+)
                    let cap_any_i32: i32 = CAP_ANY.into();
                    if backend != cap_any_i32 {
                        let _ = cap.set(CAP_PROP_HW_ACCELERATION as i32, 1.0);
                        let _ = cap.set(CAP_PROP_HW_DEVICE as i32, -1.0);
                    }
                    
                    println!("✅ Successfully opened with {}", description);
                    if let Ok(backend_name) = cap.get_backend_name() {
                        println!("📹 Active backend: {}", backend_name);
                        
                        // Log hardware acceleration status
                        if let Ok(hw_status) = cap.get(CAP_PROP_HW_ACCELERATION as i32) {
                            println!("⚡ Hardware acceleration status: {}", hw_status);
                        }
                    }
                    
                    return Ok(Some(cap));
                }
            }
            Err(e) => {
                println!("❌ Failed {}: {}", description, e);
            }
        }
        
        Ok(None)
    }

    fn is_apple_silicon() -> bool {
        // Detect Apple Silicon
        #[cfg(target_os = "macos")]
        {
            use std::process::Command;
            if let Ok(output) = Command::new("sysctl")
                .args(&["-n", "machdep.cpu.brand_string"])
                .output() 
            {
                let cpu_info = String::from_utf8_lossy(&output.stdout);
                return cpu_info.contains("Apple");
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = HardwareAccelConfig::default();
        assert_eq!(config.enabled, false);
        assert_eq!(config.mode, "auto");
        assert_eq!(config.fallback_to_cpu, true);
    }

    #[test]
    fn test_platform_detection() {
        let config = HardwareAccelConfig {
            enabled: true,
            mode: "auto".to_string(),
            fallback_to_cpu: true,
            prefer_backends: vec!["any".to_string()],
        };
        
        let backends = HardwareAcceleratedCapture::get_platform_backends(&config);
        assert!(!backends.is_empty());
    }
} 