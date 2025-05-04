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
   cd rtsp_stream_extractor
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
   # Build and run directly
   cargo run --release
   ```
   Alternatively, if you have already built the project with `cargo build --release`,
   you can run the executable directly:
   ```bash
   ./target/release/rtsp_stream_extractor
   ```

## Docker deployment

This application can be easily deployed using Docker and Docker Compose.

### Prerequisites

- [Docker](https://docs.docker.com/get-docker/) installed on your system.
- [Docker Compose](https://docs.docker.com/compose/install/) (usually included with Docker Desktop).

### Configuration

1.  **Create `config.json`:** Before building or running, create a `config.json` file in the root directory of the project. Configure your RTSP URLs, output directory (use `/app/media` inside the container), and other settings as described in the [Configuration](#configuration) section.

    *Example `config.json` for Docker:* 
    ```json
    {
        "rtsp_url": "rtsp://your_camera_stream_url",
        "rtsp_url_list": [
            "rtsp://camera1_url",
            "rtsp://camera2_url"
        ],
        "output_directory": "/app/media", 
        "show_preview": false,
        "saving_option": "list",
        "saved_time_duration": 600, 
        "use_fps": false,
        "fps": 30.0
    }
    ```
    *Note: `show_preview` should generally be `false` in a Docker deployment.* 

2.  **(Optional) Create Host Media Directory:** Create a directory on your host machine where you want the recordings to be saved (e.g., `mkdir ./media`). The `docker-compose.yml` file is configured to mount `./media` from the host into the container.

### Building and Running with Docker Compose (Recommended)

1.  **Navigate:** Open a terminal in the project's root directory (where `docker-compose.yml` is located).
2.  **Build and Run:** Execute the following command:
    ```bash
    docker compose up --build -d
    ```
    *   `--build`: Ensures the image is built using the `Dockerfile` if it doesn't exist or if the `Dockerfile` changed.
    *   `-d`: Runs the container in detached mode (in the background).

3.  **View Logs:** To see the application logs, run:
    ```bash
    docker compose logs -f
    ```
4.  **Stop:** To stop the service, run:
    ```bash
    docker compose down
    ```

### Building and Running with Docker Commands

1.  **Navigate:** Open a terminal in the project's root directory.
2.  **Build the Image:**
    ```bash
    docker build -t rtsp_stream_extractor .
    ```
3.  **Run the Container:**
    ```bash
    docker run -d --name rtsp-recorder \
      --restart always \
      -e TZ=Asia/Taipei \
      -v "$(pwd)/config.json":/app/config.json:ro \
      -v "$(pwd)/media":/app/media \
      rtsp_stream_extractor
    ```
    *   `-d`: Detached mode.
    *   `--name rtsp-recorder`: Assigns a name.
    *   `--restart always`: Automatic restart.
    *   `-e TZ=Asia/Taipei`: Sets the timezone (adjust as needed).
    *   `-v "$(pwd)/config.json":/app/config.json:ro`: Mounts your local `config.json` (read-only).
    *   `-v "$(pwd)/media":/app/media`: Mounts your local `media` directory for recordings.
    *   `rtsp_stream_extractor`: The image name.

4.  **View Logs:**
    ```bash
    docker logs -f rtsp-recorder
    ```
5.  **Stop and Remove:**
    ```bash
    docker stop rtsp-recorder
    docker rm rtsp-recorder
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
