use anyhow::{Context, Result};
use csv::WriterBuilder;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub mod parsers;

pub trait Parser {
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
