use anyhow::{Context, Result};
use csv::WriterBuilder;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub trait Parser {
    type Transaction: Transaction;

    fn csv_header(&self) -> Vec<&'static str>;

    fn transactions(&self) -> &[Self::Transaction];

    fn parse(&mut self, path: PathBuf, cfg: &mut AppConfig, cfg_path: &str) -> Result<()>;

    fn write_csv<P: AsRef<Path> + std::fmt::Display>(&self, path: P) -> Result<()> {
        let mut csv_writer = WriterBuilder::new()
            .delimiter(b';')
            .from_path(&path)
            .with_context(|| format!("Ошибка открытия файла: {}", path))?;

        csv_writer.write_record(self.csv_header())?;

        for transaction in self.transactions() {
            csv_writer.write_record(transaction.to_csv_row())?;
        }

        Ok(())
    }
}

pub trait Transaction {
    fn to_csv_row(&self) -> Vec<&str>;

    fn apply_rename_rules(&mut self, rules: &[RenameRule]);
}

#[derive(Default, Serialize, Deserialize)]
pub struct AppConfig {
    pub input_folder: String,
    pub output_folder: String,
    pub ttb_channels: Vec<String>,
    pub rules: Vec<RenameRule>,
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
