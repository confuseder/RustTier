use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct LogConfig {
    pub level: String
}
