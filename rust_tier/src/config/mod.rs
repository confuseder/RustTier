mod schema;

use schema::LogConfig;
use serde::Deserialize;
use std::error::Error;
use tracing::{debug, error};

#[derive(Debug, Deserialize)]
pub struct Config {
    pub log: LogConfig,
}

impl Config {
    pub fn new(path: &str) -> Result<Self, Box<dyn Error>> {
        debug!("Attempting to load config from path '{}'", path);
        
        let settings = match config::Config::builder()
            .add_source(config::File::with_name(&path))
            .build() {
                Ok(s) => s,
                Err(e) => {
                    error!("Failed to load config file '{}': {}", path, e);
                    return Err(Box::new(e));
                }
            };

        match settings.try_deserialize::<Config>() {
            Ok(config) => {
                debug!("Successfully loaded config from '{}'", path);
                Ok(config)
            },
            Err(e) => {
                error!("Failed to parse config file '{}': {}", path, e);
                Err(Box::new(e))
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_config() {
        let config = match Config::new("./src/config/config_example.yml") {
            Ok(config) => config,
            Err(e) => {
                panic!("Failed to load test config file: {}", e);
            }
        };
        println!("{config:#?}");
    }
}
