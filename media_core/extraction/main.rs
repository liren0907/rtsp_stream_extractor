mod config;
mod processing;
mod video;

use std::error::Error;

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let config_path = "config.json";
    processing::run(config_path)
}