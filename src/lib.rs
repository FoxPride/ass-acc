use anyhow::{Context, Result};
use chrono::NaiveDateTime;
use csv::WriterBuilder;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Custom serde format for NaiveDateTime: "%d-%m-%Y %H:%M"
pub mod datetime_format {
    use chrono::NaiveDateTime;
    use serde::{self, Deserialize, Deserializer, Serializer};

    const FORMAT: &str = "%d-%m-%Y %H:%M";

    pub fn serialize<S>(date: &Option<NaiveDateTime>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match date {
            Some(dt) => serializer.serialize_str(&dt.format(FORMAT).to_string()),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<NaiveDateTime>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: Option<String> = Option::deserialize(deserializer)?;
        match s {
            Some(ref str) if !str.is_empty() => NaiveDateTime::parse_from_str(str, FORMAT)
                .map(Some)
                .map_err(serde::de::Error::custom),
            _ => Ok(None),
        }
    }
}

pub mod parsers;

pub trait Parser {
    fn name(&self) -> &'static str;

    fn get_output(&self) -> &PathBuf;

    fn transactions(&self) -> &[Transaction];

    fn parse(&mut self, cfg: &mut AppConfig, cfg_path: &str) -> Result<()>;

    fn write_csv(&self) -> Result<()> {
        let mut csv_writer = WriterBuilder::new()
            .delimiter(b';')
            .from_path(self.get_output())
            .with_context(|| format!("Ошибка открытия файла: {:?}", self.get_output()))?;

        csv_writer.write_record(["Category", "Description", "Date Time", "Amount"])?;

        for transaction in self.transactions() {
            csv_writer.write_record(transaction.to_csv_row())?;
        }

        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct Transaction {
    pub date_time: String,
    pub category: String,
    pub amount: String,
    pub description: String,
}

impl Transaction {
    fn to_csv_row(&self) -> Vec<&str> {
        vec![
            &self.category,
            &self.description,
            &self.date_time,
            &self.amount,
        ]
    }

    pub fn apply_rename_rules(&mut self, rules: &[RenameRule]) {
        for rule in rules {
            if !rule.regex.is_match(&self.description) {
                continue;
            }

            if let Some(required_amount) = &rule.amount
                && required_amount != &self.amount
            {
                continue;
            }

            self.category = rule.category.clone();
            if let Some(desc) = &rule.description {
                self.description = desc.clone();
            }
            break;
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct AppConfig {
    pub input_folder: String,
    pub output_folder: String,
    #[serde(with = "datetime_format", default)]
    pub last_parsed_datetime: Option<NaiveDateTime>,
    pub ocr_models: (String, String),
    pub ttb_channels: Vec<String>,
    pub rules: Vec<RenameRule>,
    pub access_token: String,
    pub client_secret: String,
    pub bybit_selectors: BybitSelectors,
    pub truemoney_config: TrueMoneyConfig,
}

#[derive(Default, Serialize, Deserialize)]
pub struct BybitSelectors {
    pub merchant: String,
    pub status: String,
    pub amount: String,
    pub datetime: String,
}

#[derive(Default, Serialize, Deserialize)]
pub struct TrueMoneyConfig {
    pub region_config: TrueMoneyRegionConfig,
    pub date_params: TrueMoneyRegionSearchParams,
    pub description_params: TrueMoneyRegionSearchParams,
    pub amount_params: TrueMoneyRegionSearchParams,
    pub time_params: TrueMoneyRegionSearchParams,
    pub search_params: TrueMoneyRegionSearchParams,
}

#[derive(Default, Serialize, Deserialize)]
pub struct TrueMoneyRegionConfig {
    pub bound_offset: u32,
    pub transaction_background: u8,
    pub date_background: u8,
}

#[derive(Default, Serialize, Deserialize)]
pub struct TrueMoneyRegionSearchParams {
    pub region_x: u32,
    pub region_width: u32,
    pub left_bound_start: u32,
    pub right_bound_start: u32,
    pub current_region_skip: u32,
    pub next_region_skip: i32,
    pub empty_column_threshold: u32,
}

#[derive(Serialize, Deserialize)]
pub struct RenameRule {
    #[serde(with = "serde_regex")]
    pub regex: Regex,
    pub category: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub amount: Option<String>,
}
