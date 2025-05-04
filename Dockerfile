# Use the Rust image as the single base image
FROM rust:1.81.0

# Install build and runtime dependencies
# (ffmpeg, opencv dev libraries, clang/llvm for dependencies)
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

# Copy application source code
COPY Cargo.toml Cargo.lock ./
COPY src ./src

# Build the application in release mode
RUN cargo build --release

# Set the command to run the application when the container starts
# The binary is now in target/release/
ENTRYPOINT ["./target/release/rtsp_stream_extractor"] 