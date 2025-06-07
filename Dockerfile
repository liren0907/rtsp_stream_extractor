# Use a recent Rust image as the base
FROM rust:latest

# Install build and runtime dependencies
# (ffmpeg for recording, opencv dev libraries for custom FPS/preview)
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    pkg-config \
    cmake \
    ffmpeg \
    libopencv-dev \
    clang \
    libclang-dev \
    llvm \
    ca-certificates \
    # Clean up apt cache
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the entire project context into the container
COPY . .

# Build the entire workspace in release mode
# The `--workspace` flag is crucial for a workspace project
RUN cargo build --workspace --release

# Set the command to run the application when the container starts
ENTRYPOINT ["./target/release/rtsp_stream_extractor"] 