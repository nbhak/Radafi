use log::{error, info};
use reqwest::{Client, Error};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

mod threadpool;
use self::threadpool::ThreadPool;

/**
 * Defines the categories of errors that may occur when recording radio streams
 * from Radio Garden.
 */
#[derive(Debug, Error)]
pub enum RecordingError {
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("MP3 decoding error: {0}")]
    Decode(#[from] minimp3::Error),
}

/**
 * ----------------------------------------------------------------------------
 * The following are structures for storing results returned by the Radio
 * Garden API.
 */
#[derive(Deserialize)]
struct Place {
    id: String,
    country: String,
}

#[derive(Deserialize)]
struct Data {
    list: Vec<Place>,
}

#[derive(Deserialize)]
struct PlaceList {
    data: Data,
}

#[derive(Serialize, Deserialize, Debug)]
struct ChannelResponse {
    #[serde(rename = "data")]
    channel_data: ChannelData,
}

#[derive(Serialize, Deserialize, Debug)]
struct ChannelData {
    content: Vec<Content>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Content {
    items: Vec<Item>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Item {
    page: Page,
}

#[derive(Serialize, Deserialize, Debug)]
struct Page {
    url: String,
    title: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct Stream {
    name: String,
    url: String,
}

/**
 * ----------------------------------------------------------------------------
 * This struct provides the functionality to obtain mp3 radio recordings from
 * via Radio Garden.
 */
pub struct Listener {
    url: Url,             // Radio Garden API URL
    client: Client,       // HTTP client
    streams: Vec<Stream>, // Radio broadcast links to record
}

impl Listener {
    pub fn new(base_url: &str) -> Self {
        let url = Url::parse(base_url).expect("Failed to parse base URL");
        info!("Initialized Listener with URL: {}", url);
        Listener {
            url,
            client: Client::new(),
            streams: Vec::new(),
        }
    }

    /**
     * Saves mp3 recordings for a given duration and directory.
     * It will record up to ten channels at once.
     */
    pub async fn record_streams(
        &mut self,
        duration_seconds: u64,
        directory: &str,
    ) -> Result<(), RecordingError> {
        fs::create_dir_all(directory)?;

        let num_workers = std::cmp::min(10, self.streams.len());
        let pool = ThreadPool::new(num_workers);

        // Record stream from each channel identified in the region
        for stream_info in self.streams.iter() {
            let stream_url = stream_info.url.clone();
            let filename = format!("stream_{}.mp3", stream_info.name);
            let target_path = Path::new(directory).join(filename);
            let client = self.client.clone();
            let duration = duration_seconds;

            // Add a recording task to be scheduled by the threadpool
            pool.execute(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    match client.get(&stream_url).send().await {
                        Ok(mut response) => {
                            if let Ok(mut file) = File::create(&target_path) {
                                let start_time = Instant::now();
                                while start_time.elapsed() < Duration::from_secs(duration) {
                                    match response.chunk().await {
                                        Ok(Some(chunk)) => {
                                            if let Err(e) = file.write_all(&chunk) {
                                                error!("Error writing to file: {}", e);
                                                break;
                                            }
                                        }
                                        Ok(None) => break,
                                        Err(e) => {
                                            error!("Error reading from response: {}", e);
                                            break;
                                        }
                                    }
                                }
                                info!("Successfully recorded: {}", target_path.display());
                            } else {
                                error!("Error creating file: {}", target_path.display());
                            }
                        }
                        Err(e) => {
                            error!("Error fetching stream URL: {}", e);
                        }
                    }
                });
            });
        }

        pool.terminate();

        Ok(())
    }

    /**
     * Obtains a list of Radio Garden locations with IDs for a given country.
     */
    async fn fetch_places(&self, country: &str) -> Result<Vec<Place>, Error> {
        let places_url = self
            .url
            .join("places")
            .expect("Failed to construct places URL");
        info!("Fetching places from URL: {}", places_url);

        let places_response: PlaceList = self
            .client
            .get(places_url)
            .send()
            .await?
            .json::<PlaceList>()
            .await?;

        Ok(places_response
            .data
            .list
            .into_iter()
            .filter(|p| p.country == country)
            .collect())
    }

    /**
     * Obtains channel information for a particular location (represented by
     * its Radio Garden ID).
     */
    async fn fetch_channels(&self, place_id: &str) -> Result<Vec<Item>, Error> {
        let channels_url: Url = self
            .url
            .join(&format!("page/{}/channels", place_id))
            .expect("Failed to construct channels URL");
        info!("Fetching channels from URL: {}", channels_url);

        let channel_response: ChannelResponse = self
            .client
            .get(channels_url)
            .send()
            .await?
            .json::<ChannelResponse>()
            .await?;

        Ok(channel_response
            .channel_data
            .content
            .into_iter()
            .flat_map(|c| c.items)
            .collect())
    }

    /**
     * Obtains the links to radio streams in a given country. Returns the
     * number of channels identified in the region.
     */
    pub async fn store_streams(&mut self, country: &str) -> Result<usize, Error> {
        let places = self.fetch_places(country).await?;
        // Replace list of streams with those from new country
        self.streams.clear();

        for place in places {
            let items = self.fetch_channels(&place.id).await?;
            for item in items {
                let name: String = item
                    .page
                    .title
                    .chars()
                    .filter(|c| c.is_alphanumeric())
                    .collect();
                // The channel ID is the last element of the path in the URL
                let parts: Vec<&str> = item.page.url.split('/').collect();
                if let Some(last_part) = parts.last() {
                    let stream_url = format!("{}listen/{}/channel.mp3", self.url, last_part);
                    self.streams.push(Stream {
                        url: stream_url,
                        name: name,
                    });
                }
            }
        }

        Ok(self.streams.len())
    }
}
