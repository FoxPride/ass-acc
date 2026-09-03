use std::path::PathBuf;

use anyhow::Context;
use scraper::{Html, Selector};

use crate::{AppConfig, Parser, Transaction};

/// Date format from statement
const BYBIT_DATE_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

/// Column indices in the Bybit transaction table.
const DESCRIPTION_COL: usize = 0;
const AMOUNT_COL: usize = 1;
const STATUS_COL: usize = 3;
const DATETIME_COL: usize = 4;

/// Parses a CSS selector, reporting the config key when it is invalid.
fn parse_selector(name: &str, value: &str) -> anyhow::Result<Selector> {
    Selector::parse(value).map_err(|e| anyhow::anyhow!("Invalid `{name}` selector {value:?}: {e}"))
}

pub struct BybitParser {
    input: PathBuf,
    output: PathBuf,
    pub transactions: Vec<Transaction>,
}

impl BybitParser {
    pub fn new(input: PathBuf, output_folder: &str) -> Self {
        let mut output = PathBuf::from(output_folder);
        output.push("Bybit.csv");

        Self {
            input,
            output,
            transactions: Vec::new(),
        }
    }
}

impl Parser for BybitParser {
    fn name(&self) -> &'static str {
        "Bybit"
    }

    fn get_output(&self) -> &PathBuf {
        &self.output
    }

    fn transactions(&self) -> &[Transaction] {
        &self.transactions
    }

    fn parse(&mut self, cfg: &AppConfig) -> anyhow::Result<()> {
        let html_content = std::fs::read_to_string(self.input.as_path())
            .with_context(|| format!("Error opening file: {:?}", self.input.as_path()))?;
        let document = Html::parse_document(&html_content);

        let row_selector = parse_selector("row", "tbody tr")?;
        let td_selector = parse_selector("cell", "td")?;

        let merchant_selector =
            parse_selector("bybit_selectors.merchant", &cfg.bybit_selectors.merchant)?;
        let status_selector =
            parse_selector("bybit_selectors.status", &cfg.bybit_selectors.status)?;
        let amount_selector =
            parse_selector("bybit_selectors.amount", &cfg.bybit_selectors.amount)?;
        let datetime_selector =
            parse_selector("bybit_selectors.datetime", &cfg.bybit_selectors.datetime)?;

        for row in document.select(&row_selector) {
            let cells: Vec<_> = row.select(&td_selector).collect();

            if cells.len() >= 6 {
                let description = cells[DESCRIPTION_COL]
                    .select(&merchant_selector)
                    .next()
                    .map(|e| e.text().collect::<String>().trim().to_string())
                    .unwrap_or_default();

                let status = cells[STATUS_COL]
                    .select(&status_selector)
                    .next()
                    .map(|e| e.text().collect::<String>().trim().to_string())
                    .unwrap_or_else(|| {
                        cells[STATUS_COL]
                            .text()
                            .collect::<String>()
                            .trim()
                            .to_string()
                    });

                if description.is_empty() || status != "Successful" {
                    continue;
                }

                let amount = cells[AMOUNT_COL]
                    .select(&amount_selector)
                    .next()
                    .map(|e| e.text().collect::<String>().trim().to_string())
                    .unwrap_or_default();

                let date_time = cells[DATETIME_COL]
                    .select(&datetime_selector)
                    .next()
                    .map(|e| e.text().collect::<String>().trim().to_string())
                    .unwrap_or_default();

                let category = "?".to_string();

                let mut transaction = Transaction {
                    date_time,
                    category,
                    amount,
                    description,
                };

                // Skip transactions that were already processed
                if !transaction.is_after(cfg.last_parsed_datetime, BYBIT_DATE_FORMAT) {
                    continue;
                }

                transaction.apply_rename_rules(&cfg.rules);

                self.transactions.push(transaction);
            }
        }

        Ok(())
    }
}
