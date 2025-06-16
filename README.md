# RTSP Stream Recorder

A robust Rust application for recording RTSP camera streams with support for multiple cameras, automatic reconnection, and segment-based recording. This application is the runnable binary part of a larger Rust workspace.

## Features

- Record from single or multiple RTSP camera streams simultaneously
- Automatic stream reconnection on failure (FFmpeg mode)
- Segment-based recording with customizable duration
- Two recording modes:
  - FFmpeg-based (direct stream copy for optimal performance with robust reconnection)
  - OpenCV-based (with custom FPS support and live preview)
- Configurable output directories with timestamp-based file naming
- Robust error handling and recovery for FFmpeg mode

## Prerequisites

- Rust (latest stable version)
- **FFmpeg:** Required for the default, high-performance recording mode.
- **OpenCV:** System libraries are required for the custom FPS and live preview features.

## How to Run

The application runs in RTSP stream capture mode using the `rtsp` command.

**Usage:**
```bash
./target/release/rtsp_stream_extractor rtsp    # Start RTSP stream capture
./target/release/rtsp_stream_extractor help    # Show help information
```

### 1. Configure the Application

Create a `config.json` file in the project root directory.

**Path:** `config.json` (in the project root)

**Example `config.json`:**
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
    "saved_time_duration": 300,
    "audio": false,
    "use_fps": false,
    "fps": 30.0
}
```

**Configuration Options:**

- `rtsp_url`: URL for a single RTSP stream.
- `rtsp_url_list`: A list of RTSP stream URLs for multi-camera setups.
- `output_directory`: Base directory where segmented video files will be saved.
- `show_preview`: `true` or `false`. Enables a live preview window (only works with OpenCV mode and single stream).
- `saving_option`: `"single"`, `"list"`, or `"both"`. Determines which streams to record.
- `saved_time_duration`: Duration of each video segment in seconds.
- `audio`: `true` or `false`. Whether to include audio in recordings (currently not implemented in FFmpeg mode).
- `use_fps`: If `true`, enables OpenCV mode for custom `fps` and preview. If `false` (default), uses efficient FFmpeg mode.
- `fps`: The custom FPS value to use when `use_fps` is true.

### 2. Build and Run from Source

1.  **Clone the repository.**
2.  **Build the workspace:**
    ```bash
    cargo build --workspace --release
    ```
3.  **Run the application:**
    ```bash
    # Run the specific binary package from the workspace root
    cargo run -p rtsp_stream_extractor --release rtsp
    ```
    Alternatively, run the compiled binary directly:
    ```bash
    ./target/release/rtsp_stream_extractor rtsp
    ```

## Docker Deployment

This application can be easily deployed using Docker and Docker Compose.

### 1. Configure for Docker

Place your `config.json` file in the project root. The `docker-compose.yml` file is set up to mount this file into the container.

### 2. Run with Docker Compose

1.  **Build and Run:** From the project root, execute the following command:
    ```bash
    docker-compose up --build -d
    ```
    -   `--build`: Ensures the Docker image is built from the `Dockerfile`.
    -   `-d`: Runs the container in detached (background) mode.

2.  **View Logs:**
    ```bash
    docker-compose logs -f
    ```

3.  **Stop:**
    ```bash
    docker-compose down
    ```
    Your recorded videos will be saved to the `media` directory on your host machine, as mapped in `docker-compose.yml`.

## Features in Detail

### Recording Modes

1. **FFmpeg Mode** (default, `use_fps: false`):
   - Direct stream copy without re-encoding
   - Lowest CPU usage
   - Maintains original stream quality
   - **Robust automatic reconnection** with exponential backoff
   - Handles stream failures gracefully with retry logic

2. **OpenCV Mode** (`use_fps: true`):
   - Custom FPS control
   - Frame-by-frame processing
   - Higher CPU usage
   - Useful for frame rate conversion
   - **Limited reconnection** - stream failures may require manual restart
   - Supports live preview window (single stream only)

### Error Handling

- **FFmpeg Mode**: Automatic reconnection on stream failure with exponential backoff for repeated failures
- **OpenCV Mode**: Basic error handling, may require manual restart on stream failure
- Separate error handling for each camera stream
- Detailed logging of stream status and errors

### Preview Window

- Only available in OpenCV mode (`use_fps: true`)
- Only works with single stream configurations
- Automatically disabled for multi-stream setups
- Press ESC key to exit preview

## Dependencies

- `media_core`: Internal workspace library for RTSP capture functionality
- `opencv`: Video capture and processing
- `serde`/`serde_json`: Configuration file handling
- `chrono`: Timestamp generation
- `tempfile`: Temporary file management
- `rfd`: File dialog functionality
- `m3u8-rs`: M3U8 playlist handling
- External dependency on FFmpeg for stream handling
