# ---- Builder Stage ----
# Use a specific Rust version or 'latest'
FROM rust:1.81.0 as builder

# Install build dependencies required by the Rust crates (like opencv-rust)
# and ffmpeg, plus musl tools for cross-compilation, AND clang/llvm for clang-sys
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    pkg-config \
    cmake \
    ffmpeg \
    libopencv-dev \
    musl-tools \
    clang \
    libclang-dev \
    llvm \
    # Clean up apt cache
    && rm -rf /var/lib/apt/lists/*

# Add the musl target for Rust
RUN rustup target add x86_64-unknown-linux-musl

WORKDIR /app

# Set environment variables for cross-compilation using clang
ENV CC_x86_64_unknown_linux_musl=clang
ENV CXX_x86_64_unknown_linux_musl=clang++
ENV AR_x86_64_unknown_linux_musl=llvm-ar
ENV RANLIB_x86_64_unknown_linux_musl=llvm-ranlib
ENV CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=clang

# Copy manifests and cache dependencies
COPY Cargo.toml Cargo.lock ./
# Create dummy src directory and main file to cache dependencies first
# Build for musl target here as well to ensure dependencies are compatible
RUN mkdir src && echo "fn main() {}" > src/main.rs && \
    cargo build --release --target x86_64-unknown-linux-musl && \
    rm -rf src target

# Copy the actual source code
COPY src ./src

# Build the application in release mode for the musl target
# This will use the cached dependencies layer if Cargo.lock hasn't changed
RUN cargo build --release --target x86_64-unknown-linux-musl

# ---- Runtime Stage ----
# Use a minimal Alpine base image
FROM alpine:latest as runtime

# Install only runtime dependencies: ffmpeg and necessary OpenCV shared libraries
# Using Alpine's package manager 'apk'
# 'opencv' provides the necessary .so files for OpenCV on Alpine
RUN apk add --no-cache \
    ffmpeg \
    opencv \
    ca-certificates

WORKDIR /app

# Copy the compiled musl binary from the builder stage
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/rtsp_stream_extractor .

# Create a directory for the output media files inside the container.
# This path should match what you intend to use in config.json's output_directory
# It serves as a mount point for a host volume.
RUN mkdir /app/media

# Set the command to run the application when the container starts
# The application will look for config.json in its working directory (/app)
ENTRYPOINT ["./rtsp_stream_extractor"] 