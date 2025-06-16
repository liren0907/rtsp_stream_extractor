use std::time::{Duration, Instant};

/// Processing statistics and metrics
#[derive(Debug, Clone)]
pub struct ProcessingStats {
    pub files_processed: u64,
    pub files_failed: u64,
    pub total_size_processed: u64,
    pub processing_time: Duration,
    pub start_time: Instant,
    pub errors: Vec<String>,
}

impl ProcessingStats {
    pub fn new() -> Self {
        Self {
            files_processed: 0,
            files_failed: 0,
            total_size_processed: 0,
            processing_time: Duration::new(0, 0),
            start_time: Instant::now(),
            errors: Vec::new(),
        }
    }

    pub fn add_processed_file(&mut self, file_size: u64) {
        self.files_processed += 1;
        self.total_size_processed += file_size;
    }

    pub fn add_failed_file(&mut self, error: String) {
        self.files_failed += 1;
        self.errors.push(error);
    }

    pub fn finalize(&mut self) {
        self.processing_time = self.start_time.elapsed();
    }

    pub fn success_rate(&self) -> f64 {
        let total = self.files_processed + self.files_failed;
        if total == 0 {
            0.0
        } else {
            (self.files_processed as f64 / total as f64) * 100.0
        }
    }
} 