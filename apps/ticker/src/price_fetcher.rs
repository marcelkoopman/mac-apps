use crate::config::Asset;
use polars::prelude::*;
use reqwest::blocking::Client;
use serde_json::Value;
use std::error::Error;
use std::thread;
use std::time::Duration;

const MAX_FETCH_ATTEMPTS: u32 = 3;
const RETRY_DELAY: Duration = Duration::from_millis(500);

pub struct PriceFetcher {
    client: Client,
}

impl PriceFetcher {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let client = Client::builder()
            .user_agent("rust-price-fetcher/1.0")
            .build()?;
        Ok(PriceFetcher { client })
    }

    pub fn fetch_price(&self, asset: &Asset) -> f64 {
        for attempt in 1..=MAX_FETCH_ATTEMPTS {
            let price = self.fetch_price_once(asset, attempt);
            if !price.is_nan() {
                return price;
            }

            if attempt < MAX_FETCH_ATTEMPTS {
                eprintln!(
                    "⚠️  {} failed (attempt {}/{}), retrying in {:?}...",
                    asset.name, attempt, MAX_FETCH_ATTEMPTS, RETRY_DELAY
                );
                thread::sleep(RETRY_DELAY);
            }
        }

        eprintln!(
            "✗ Giving up on {} after {} attempts",
            asset.name, MAX_FETCH_ATTEMPTS
        );
        f64::NAN
    }

    fn fetch_price_once(&self, asset: &Asset, attempt: u32) -> f64 {
        eprintln!(
            "🔍 Fetching {} from {} (attempt {}/{})",
            asset.name, asset.url, attempt, MAX_FETCH_ATTEMPTS
        );
        match self.client.get(&asset.url).send() {
            Ok(response) => match response.error_for_status() {
                Ok(resp) => match resp.json::<Value>() {
                    Ok(json) => {
                        if let Some(price_value) = self.get_value_by_path(&json, &asset.price_path)
                        {
                            match price_value {
                                Value::Number(n) => n.as_f64().unwrap_or(f64::NAN),
                                Value::String(s) => s.parse().unwrap_or(f64::NAN),
                                _ => f64::NAN,
                            }
                        } else {
                            eprintln!("✗ Path '{}' not found in JSON!", asset.price_path);
                            f64::NAN
                        }
                    }
                    Err(e) => {
                        eprintln!("✗ JSON parse error: {}", e);
                        f64::NAN
                    }
                },
                Err(e) => {
                    eprintln!("✗ HTTP error: {}", e);
                    f64::NAN
                }
            },
            Err(e) => {
                eprintln!("✗ Network error: {}", e);
                f64::NAN
            }
        }
    }

    pub fn fetch_all(&self, assets: &[Asset]) -> Result<DataFrame, Box<dyn Error>> {
        let mut symbols: Vec<String> = Vec::with_capacity(assets.len());
        let mut names: Vec<String> = Vec::with_capacity(assets.len());
        let mut prices: Vec<f64> = Vec::with_capacity(assets.len());
        let mut units: Vec<String> = Vec::with_capacity(assets.len());
        let mut unit_hints: Vec<String> = Vec::with_capacity(assets.len());

        for asset in assets {
            let price = self.fetch_price(asset);
            symbols.push(asset.symbol.clone());
            names.push(asset.name.clone());
            prices.push(price);
            units.push(asset.unit.clone());
            unit_hints.push(asset.unit_hint.clone());
        }

        DataFrame::new_infer_height(vec![
            Series::new("symbol".into(), symbols).into(),
            Series::new("name".into(), names).into(),
            Series::new("price".into(), prices).into(),
            Series::new("unit".into(), units).into(),
            Series::new("unit_hint".into(), unit_hints).into(),
        ])
        .map_err(|e| e.into())
    }

    pub fn build_initial_dataframe(
        &self,
        assets: &[Asset],
        day_opens: &std::collections::HashMap<String, f64>,
    ) -> Result<DataFrame, Box<dyn Error>> {
        let base = self.fetch_all(assets)?;
        Self::attach_change_columns(base, None, day_opens)
    }

    pub fn update_dataframe(
        &self,
        previous: &DataFrame,
        assets: &[Asset],
        day_opens: &std::collections::HashMap<String, f64>,
    ) -> Result<DataFrame, Box<dyn Error>> {
        let fresh = self.fetch_all(assets)?;

        let prev_names = previous.column("name")?.str()?;
        let prev_prices = previous.column("price")?.f64()?;

        let mut prev_by_name: std::collections::HashMap<String, f64> =
            std::collections::HashMap::new();
        for i in 0..previous.height() {
            if let (Some(name), Some(price)) = (prev_names.get(i), prev_prices.get(i)) {
                prev_by_name.insert(name.to_string(), price);
            }
        }

        Self::attach_change_columns(fresh, Some(&prev_by_name), day_opens)
    }

    fn attach_change_columns(
        mut df: DataFrame,
        prev_by_name: Option<&std::collections::HashMap<String, f64>>,
        day_opens: &std::collections::HashMap<String, f64>,
    ) -> Result<DataFrame, Box<dyn Error>> {
        let n = df.height();
        let names = df.column("name")?.str()?;
        let prices = df.column("price")?.f64()?;

        let mut prev_price_col: Vec<Option<f64>> = Vec::with_capacity(n);
        let mut change_col: Vec<Option<f64>> = Vec::with_capacity(n);
        let mut pct_col: Vec<Option<f64>> = Vec::with_capacity(n);
        let mut direction_col: Vec<String> = Vec::with_capacity(n);

        let mut day_open_col: Vec<Option<f64>> = Vec::with_capacity(n);
        let mut change_day_col: Vec<Option<f64>> = Vec::with_capacity(n);
        let mut pct_day_col: Vec<Option<f64>> = Vec::with_capacity(n);
        let mut direction_day_col: Vec<String> = Vec::with_capacity(n);

        for i in 0..n {
            let name = names.get(i).unwrap_or("");
            let price = prices.get(i).unwrap_or(f64::NAN);

            let prev = prev_by_name.and_then(|m| m.get(name).copied());
            prev_price_col.push(prev);
            if let Some(p) = prev {
                if price.is_nan() || p.is_nan() || p == 0.0 {
                    change_col.push(None);
                    pct_col.push(None);
                    direction_col.push(String::new());
                } else {
                    let change = price - p;
                    let pct = (change / p) * 100.0;
                    change_col.push(Some(change));
                    pct_col.push(Some(pct));
                    direction_col.push(Self::direction_label(change));
                }
            } else {
                change_col.push(None);
                pct_col.push(None);
                direction_col.push(String::new());
            }

            let open = day_opens
                .get(name)
                .copied()
                .filter(|o| !o.is_nan() && *o != 0.0)
                .or_else(|| {
                    if !price.is_nan() && price != 0.0 {
                        Some(price)
                    } else {
                        None
                    }
                });

            day_open_col.push(open);
            if let Some(o) = open {
                if price.is_nan() {
                    change_day_col.push(None);
                    pct_day_col.push(None);
                    direction_day_col.push(String::new());
                } else {
                    let change = price - o;
                    let pct = (change / o) * 100.0;
                    change_day_col.push(Some(change));
                    pct_day_col.push(Some(pct));
                    direction_day_col.push(Self::direction_label(change));
                }
            } else {
                change_day_col.push(None);
                pct_day_col.push(None);
                direction_day_col.push(String::new());
            }
        }

        df.with_column(Series::new("prev_price".into(), prev_price_col).into())?;
        df.with_column(Series::new("change".into(), change_col).into())?;
        df.with_column(Series::new("pct_change".into(), pct_col).into())?;
        df.with_column(Series::new("direction".into(), direction_col).into())?;
        df.with_column(Series::new("day_open".into(), day_open_col).into())?;
        df.with_column(Series::new("change_day".into(), change_day_col).into())?;
        df.with_column(Series::new("pct_day".into(), pct_day_col).into())?;
        df.with_column(Series::new("direction_day".into(), direction_day_col).into())?;

        eprintln!("📊 DataFrame:\n{df}");
        Ok(df)
    }

    fn direction_label(change: f64) -> String {
        if change > 0.01 {
            "up".to_string()
        } else if change < -0.01 {
            "down".to_string()
        } else {
            "flat".to_string()
        }
    }

    fn get_value_by_path(&self, value: &Value, path: &str) -> Option<Value> {
        let mut current = value.clone();

        for part in path.split('.') {
            if part.contains('=') {
                let (field_name, filter_value) = part.split_once('=')?;

                if let Value::Array(arr) = &current {
                    current = arr
                        .iter()
                        .find(|item| {
                            if let Value::Object(map) = item
                                && let Some(field) = map.get(field_name)
                            {
                                return field.as_str().map(|s| s == filter_value).unwrap_or(false);
                            }
                            false
                        })?
                        .clone();
                } else {
                    return None;
                }
            } else {
                current = match &current {
                    Value::Object(map) => map.get(part)?.clone(),
                    Value::Array(arr) => {
                        let index: usize = part.parse().ok()?;
                        arr.get(index)?.clone()
                    }
                    _ => return None,
                };
            }
        }

        Some(current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;

    fn fetcher() -> PriceFetcher {
        PriceFetcher::new().expect("client should build")
    }

    #[test]
    fn max_fetch_attempts_is_three() {
        assert_eq!(MAX_FETCH_ATTEMPTS, 3);
    }

    #[test]
    fn empty_assets_dataframe() {
        let f = fetcher();
        let df = f.fetch_all(&[]).expect("empty df");
        assert_eq!(df.height(), 0);
        assert_eq!(df.width(), 5);
    }

    #[test]
    fn simple_object_path() {
        let f = fetcher();
        let data = json!({"price": 42000.5});
        let v = f.get_value_by_path(&data, "price").unwrap();
        assert_eq!(v.as_f64(), Some(42000.5));
    }

    #[test]
    fn nested_object_path() {
        let f = fetcher();
        let data = json!({
            "data": {
                "quote": {
                    "EUR": {
                        "price": 91.23
                    }
                }
            }
        });
        let v = f.get_value_by_path(&data, "data.quote.EUR.price").unwrap();
        assert_eq!(v.as_f64(), Some(91.23));
    }

    #[test]
    fn array_index_path() {
        let f = fetcher();
        let data = json!([{"price": 10.0}, {"price": 20.0}]);
        let v = f.get_value_by_path(&data, "1.price").unwrap();
        assert_eq!(v.as_f64(), Some(20.0));
    }

    #[test]
    fn array_filter_by_field() {
        let f = fetcher();
        let data = json!({
            "items": [
                {"symbol": "BTC", "price": 50000.0},
                {"symbol": "ETH", "price": 3000.0}
            ]
        });
        let v = f
            .get_value_by_path(&data, "items.symbol=ETH.price")
            .unwrap();
        assert_eq!(v.as_f64(), Some(3000.0));
    }

    #[test]
    fn missing_key_returns_none() {
        let f = fetcher();
        let data = json!({"price": 1.0});
        assert!(f.get_value_by_path(&data, "missing").is_none());
    }

    #[test]
    fn direction_label_thresholds() {
        assert_eq!(PriceFetcher::direction_label(0.0), "flat");
        assert_eq!(PriceFetcher::direction_label(0.01), "flat");
        assert_eq!(PriceFetcher::direction_label(-0.01), "flat");
        assert_eq!(PriceFetcher::direction_label(0.011), "up");
        assert_eq!(PriceFetcher::direction_label(-0.011), "down");
    }

    fn base_df(name: &str, price: f64) -> DataFrame {
        DataFrame::new_infer_height(vec![
            Series::new("symbol".into(), vec!["X".to_string()]).into(),
            Series::new("name".into(), vec![name.to_string()]).into(),
            Series::new("price".into(), vec![price]).into(),
            Series::new("unit".into(), vec!["EUR".to_string()]).into(),
            Series::new("unit_hint".into(), vec!["/u".to_string()]).into(),
        ])
        .expect("df")
    }

    #[test]
    fn attach_change_columns_poll_and_day_up() {
        let df = base_df("Bitcoin", 110.0);
        let prev = HashMap::from([("Bitcoin".to_string(), 100.0)]);
        let opens = HashMap::from([("Bitcoin".to_string(), 100.0)]);
        let df = PriceFetcher::attach_change_columns(df, Some(&prev), &opens).unwrap();

        let change = df.column("change").unwrap().f64().unwrap().get(0);
        let pct = df.column("pct_change").unwrap().f64().unwrap().get(0);
        let dir = df.column("direction").unwrap().str().unwrap().get(0);
        let day_dir = df.column("direction_day").unwrap().str().unwrap().get(0);
        assert_eq!(change, Some(10.0));
        assert!((pct.unwrap() - 10.0).abs() < 1e-9);
        assert_eq!(dir, Some("up"));
        assert_eq!(day_dir, Some("up"));
    }

    #[test]
    fn attach_change_columns_day_down_poll_flat() {
        let df = base_df("Gold", 100.005);
        let prev = HashMap::from([("Gold".to_string(), 100.0)]);
        let opens = HashMap::from([("Gold".to_string(), 110.0)]);
        let df = PriceFetcher::attach_change_columns(df, Some(&prev), &opens).unwrap();

        assert_eq!(
            df.column("direction").unwrap().str().unwrap().get(0),
            Some("flat")
        );
        assert_eq!(
            df.column("direction_day").unwrap().str().unwrap().get(0),
            Some("down")
        );
    }

    #[test]
    fn attach_change_columns_nan_price_has_no_change() {
        let df = base_df("Gas", f64::NAN);
        let prev = HashMap::from([("Gas".to_string(), 40.0)]);
        let opens = HashMap::from([("Gas".to_string(), 40.0)]);
        let df = PriceFetcher::attach_change_columns(df, Some(&prev), &opens).unwrap();

        assert!(df.column("change").unwrap().f64().unwrap().get(0).is_none());
        assert_eq!(
            df.column("direction").unwrap().str().unwrap().get(0),
            Some("")
        );
        assert_eq!(
            df.column("direction_day").unwrap().str().unwrap().get(0),
            Some("")
        );
    }

    #[test]
    fn attach_change_columns_missing_open_uses_current_price() {
        let df = base_df("Power", 50.0);
        let df = PriceFetcher::attach_change_columns(df, None, &HashMap::new()).unwrap();
        assert_eq!(
            df.column("day_open").unwrap().f64().unwrap().get(0),
            Some(50.0)
        );
        assert_eq!(
            df.column("direction_day").unwrap().str().unwrap().get(0),
            Some("flat")
        );
        assert!(df.column("change").unwrap().f64().unwrap().get(0).is_none());
    }

    #[test]
    fn get_value_by_path_string_number() {
        let f = fetcher();
        let data = json!({"price": "42.5"});
        let v = f.get_value_by_path(&data, "price").unwrap();
        assert_eq!(v.as_str(), Some("42.5"));
    }
}
