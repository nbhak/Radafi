mod midhyae;

use midhyae::Listener;
use std::env;
use tokio::runtime;
use log::{error, info};

fn main() {
    env_logger::init();
    
    // Parse command-line arguments
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 {
        error!("Usage: {} <country> <directory> <duration>", args[0]);
        return;
    }

    let country = &args[1];
    let directory = &args[2];
    let duration = args[3].parse::<u64>().unwrap_or_else(|_| {
        error!("Invalid duration: {}", args[3]);
        std::process::exit(1);
    });

    let rt: runtime::Runtime = runtime::Runtime::new().expect("Failed to create a runtime");
    let mut listener = Listener::new("http://radio.garden/api/ara/content/");

    rt.block_on(async {
        // Store streams for the given country
        match listener.store_streams(country).await {
            Ok(count) => info!("Stored {} streams.", count),
            Err(e) => error!("Failed to store streams: {}", e),
        }

        // Record streams
        match listener.record_streams(duration, directory).await {
            Ok(()) => info!("Successfully recorded streams."),
            Err(e) => error!("Failed to record streams: {}", e),
        }
    });
}
