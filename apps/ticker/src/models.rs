use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub assets: Vec<Asset>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub price_path: String,
    pub unit: String,
    #[serde(default = "default_poll_interval")]
    pub poll_interval: String,
}

fn default_poll_interval() -> String {
    "1m".to_string()
}

impl Asset {
    /// Converts poll_interval string to seconds
    /// Valid values: "1m", "1h", "24h"
    pub fn poll_interval_seconds(&self) -> u64 {
        match self.poll_interval.as_str() {
            "1m" => 60,
            "1h" => 3600,
            "24h" => 86400,
            _ => 60, // default to 1 minute
        }
    }
}

#[derive(Debug, Clone)]
pub struct Price {
    pub name: String,
    pub value: f64,
    pub unit: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_poll_interval_1m() {
        let asset = Asset {
            name: "Bitcoin".to_string(),
            url: "https://example.com".to_string(),
            price_path: "price".to_string(),
            unit: "EUR".to_string(),
            poll_interval: "1m".to_string(),
        };
        assert_eq!(asset.poll_interval_seconds(), 60);
    }

    #[test]
    fn test_poll_interval_1h() {
        let asset = Asset {
            name: "Gold".to_string(),
            url: "https://example.com".to_string(),
            price_path: "price".to_string(),
            unit: "EUR".to_string(),
            poll_interval: "1h".to_string(),
        };
        assert_eq!(asset.poll_interval_seconds(), 3600);
    }

    #[test]
    fn test_poll_interval_24h() {
        let asset = Asset {
            name: "Gas".to_string(),
            url: "https://example.com".to_string(),
            price_path: "price".to_string(),
            unit: "EUR/MWh".to_string(),
            poll_interval: "24h".to_string(),
        };
        assert_eq!(asset.poll_interval_seconds(), 86400);
    }

    #[test]
    fn test_poll_interval_default() {
        let asset = Asset {
            name: "Test".to_string(),
            url: "https://example.com".to_string(),
            price_path: "price".to_string(),
            unit: "EUR".to_string(),
            poll_interval: "invalid".to_string(),
        };
        assert_eq!(asset.poll_interval_seconds(), 60); // defaults to 60s
    }

    #[test]
    fn test_default_poll_interval_function() {
        assert_eq!(default_poll_interval(), "1m");
    }
}
