# RTSP Stream Recorder

A robust Rust application for recording RTSP camera streams with support for multiple cameras, automatic reconnection, and segment-based recording. The application can handle both single camera and multiple camera setups, with options for video preview and custom FPS settings.

## Features

- Record from single or multiple RTSP camera streams simultaneously
- Automatic stream reconnection on failure with exponential backoff
- Segment-based recording with customizable duration
- Two recording modes:
  - FFmpeg-based (direct stream copy for optimal performance)
  - OpenCV-based (with custom FPS support)
- Optional real-time video preview
- Configurable output directories with timestamp-based file naming
- Robust error handling and recovery
- Support for TCP transport mode for better reliability

## Prerequisites

- Rust (latest stable version)
- FFmpeg (for stream recording)
- OpenCV system dependencies (for preview and custom FPS recording)

## Installation

1. Clone this repository:
   ```bash
   git clone [repository-url]
   cd rtsp-stream
   ```

2. Build the project:
   ```bash
   cargo build --release
   ```

## Configuration

The application uses `config.json` for configuration:

```json
{
    "rtsp_url": "rtsp://username:password@camera-ip:port/stream",
    "rtsp_url_list": [
        "rtsp://username:password@camera1-ip:port/stream",
        "rtsp://username:password@camera2-ip:port/stream"
    ],
    "output_directory": "media",
    "show_preview": false,
    "saving_option": "list",
    "saved_time_duration": 30,
    "use_fps": false,
    "fps": 30.0
}
```

### Configuration Options

- `rtsp_url`: Single camera RTSP URL (used when `saving_option` is "single")
- `rtsp_url_list`: List of RTSP URLs for multiple cameras
- `output_directory`: Base directory for recorded segments
- `show_preview`: Enable/disable video preview window
- `saving_option`: Recording mode ("single", "list", or "both")
- `saved_time_duration`: Duration of each recorded segment in seconds
- `use_fps`: Enable custom FPS mode (uses OpenCV instead of FFmpeg)
- `fps`: Custom FPS value when `use_fps` is true

## Usage

1. Configure your settings in `config.json`
2. Run the application:
   ```bash
   cargo run --release
   ```

### Output Structure

The application creates a directory structure like this:
```
media/
├── camera_rtsp_username_password_camera1-ip_port_stream/
│   ├── segment_20240102_123000.mp4
│   ├── segment_20240102_123030.mp4
│   └── ...
├── camera_rtsp_username_password_camera2-ip_port_stream/
│   ├── segment_20240102_123000.mp4
│   └── ...
```

## Features in Detail

### Recording Modes

1. **FFmpeg Mode** (default):
   - Direct stream copy without re-encoding
   - Lowest CPU usage
   - Maintains original stream quality
   - Automatic reconnection on failure

2. **OpenCV Mode** (when `use_fps` is true):
   - Custom FPS control
   - Frame-by-frame processing
   - Higher CPU usage
   - Useful for frame rate conversion

### Error Handling

- Automatic reconnection on stream failure
- Exponential backoff for repeated failures
- Separate error handling for each camera stream
- Detailed logging of stream status and errors

## Dependencies

- `opencv`: Video capture and processing
- `serde`/`serde_json`: Configuration file handling
- `chrono`: Timestamp generation
- `tempfile`: Temporary file management
- External dependency on FFmpeg for stream handling
