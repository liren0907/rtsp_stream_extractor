#!/bin/bash
# Build Script for Media Core Project
# Usage: ./run_build.sh [release|performance|apple-silicon]

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}Media Core - Build System${NC}"

BUILD_TYPE=${1:-release}

case $BUILD_TYPE in    
    "release")
        echo -e "${GREEN}Building Release Version...${NC}"
        cargo build --release
        ;;
    
    "performance")
        echo -e "${BLUE}Building High Performance Version...${NC}"
        echo -e "${YELLOW}Note: This build includes CPU-specific optimizations${NC}"
        
        # CPU-specific optimizations
        export RUSTFLAGS="-C target-cpu=native -C target-feature=+avx2,+fma"
        cargo build --release
        ;;
    
    "apple-silicon")
        echo -e "${GREEN}Building Apple Silicon Optimized Version...${NC}"
        
        # Detect Apple Silicon chip
        APPLE_CHIP=$(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo "Unknown")
        echo -e "${BLUE}Detected CPU: $APPLE_CHIP${NC}"
        
        # Apple Silicon optimizations for media processing
        export RUSTFLAGS="-C target-cpu=native -C target-feature=+neon,+crc,+dotprod,+fp16,+ras,+lse,+rdm,+bf16,+i8mm,+flagm -C prefer-dynamic=false"
        
        # Linker optimizations
        export CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER=clang
        export CARGO_TARGET_AARCH64_APPLE_DARWIN_RUSTFLAGS="-C link-arg=-fuse-ld=lld -C link-arg=-Wl,-dead_strip -C link-arg=-Wl,-x"
        
        # Media processing environment optimizations
        export OPENCV_VIDEOIO_PRIORITY_LIST=AVFOUNDATION,FFMPEG
        export OPENCV_FFMPEG_CAPTURE_OPTIONS="hw_decoders_any;videotoolbox"
        export VECLIB_MAXIMUM_THREADS=8
        
        echo -e "${YELLOW}Applying media processing optimizations...${NC}"
        # cargo build --release
        cargo build --release --target aarch64-apple-darwin
        ;;
        
    *)
        echo -e "${RED}Error: Unknown build type '$BUILD_TYPE'${NC}"
        echo -e "${YELLOW}Usage: ./run_build.sh [dev|release|performance|apple-silicon]${NC}"
        echo ""
        echo "Build types:"
        echo "  release       - Standard release build"
        echo "  performance   - High performance build with CPU optimizations"
        echo "  apple-silicon - Apple Silicon optimized build for media processing"
        exit 1
        ;;
esac

echo -e "${GREEN}Build completed successfully!${NC}"