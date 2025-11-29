use std::path::PathBuf;

use anyhow::Context;
use scraper::{Html, Selector};

use crate::{AppConfig, Parser, Transaction};

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
    fn get_output(&self) -> &std::path::PathBuf {
        &self.output
    }

    fn transactions(&self) -> &[Transaction] {
        &self.transactions
    }

    fn parse(&mut self, cfg: &mut AppConfig, _cfg_path: &str) -> anyhow::Result<()> {
        let html_content = std::fs::read_to_string(self.input.as_path())
            .with_context(|| format!("Ошибка открытия файла: {:?}", self.input.as_path()))?;
        let document = Html::parse_document(&html_content);

        let row_selector = Selector::parse("tbody tr").unwrap();

        let merchant_selector = Selector::parse(".bycard__trans-table-merch-name-col").unwrap();
        let amount_selector = Selector::parse("td:nth-child(2) p").unwrap();
        let status_selector = Selector::parse("td:nth-child(4) p").unwrap();
        let datetime_selector = Selector::parse("td:nth-child(5) span").unwrap();

        for row in document.select(&row_selector) {
            let description = row
                .select(&merchant_selector)
                .next()
                .map(|e| e.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            let status = row
                .select(&status_selector)
                .next()
                .map(|e| e.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            if description.is_empty() || status != "Successful" {
                continue;
            }

            let amount = row
                .select(&amount_selector)
                .next()
                .map(|e| e.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            let date_time = row
                .select(&datetime_selector)
                .next()
                .map(|e| e.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            let category = "-".to_string();

            let mut transaction = Transaction {
                date_time,
                category,
                amount,
                description,
            };
            transaction.apply_rename_rules(&cfg.rules);

            self.transactions.push(transaction);
        }

        Ok(())
    }
}
