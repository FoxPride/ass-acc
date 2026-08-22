use std::path::PathBuf;

use anyhow::Context;
use scraper::{Html, Selector};

use crate::{AppConfig, Parser, Transaction};

/// Date format from statement
const BYBIT_DATE_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

pub struct BybitParser {
    input: PathBuf,
    output: PathBuf,
    pub transactions: Vec<Transaction>,
}

impl BybitParser {
    pub fn new(input: std::path::PathBuf, output_folder: &str) -> Self {
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

    fn get_output(&self) -> &std::path::PathBuf {
        &self.output
    }

    fn transactions(&self) -> &[Transaction] {
        &self.transactions
    }

    fn parse(&mut self, cfg: &mut AppConfig, _cfg_path: &str) -> anyhow::Result<()> {
        let html_content = std::fs::read_to_string(self.input.as_path())
            .with_context(|| format!("Error opening file: {:?}", self.input.as_path()))?;
        let document = Html::parse_document(&html_content);

        let row_selector = Selector::parse("tbody tr").unwrap();
        let td_selector = Selector::parse("td").unwrap();

        let merchant_selector = Selector::parse(&cfg.bybit_selectors.merchant).unwrap();
        let status_selector = Selector::parse(&cfg.bybit_selectors.status).unwrap();
        let amount_selector = Selector::parse(&cfg.bybit_selectors.amount).unwrap();
        let datetime_selector = Selector::parse(&cfg.bybit_selectors.datetime).unwrap();

        for row in document.select(&row_selector) {
            let cells: Vec<_> = row.select(&td_selector).collect();

            if cells.len() >= 6 {
                let description = cells[0]
                    .select(&merchant_selector)
                    .next()
                    .map(|e| e.text().collect::<String>().trim().to_string())
                    .unwrap_or_default();

                let status = cells[3]
                    .select(&status_selector)
                    .next()
                    .map(|e| e.text().collect::<String>().trim().to_string())
                    .unwrap_or_else(|| cells[3].text().collect::<String>().trim().to_string());

                if description.is_empty() || status != "Successful" {
                    continue;
                }

                let amount = cells[1]
                    .select(&amount_selector)
                    .next()
                    .map(|e| e.text().collect::<String>().trim().to_string())
                    .unwrap_or_default();

                let date_time = cells[4]
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
