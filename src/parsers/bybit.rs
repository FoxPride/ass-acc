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

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use chrono::NaiveDateTime;
    use regex::Regex;

    use crate::{AppConfig, BybitSelectors, Parser, RenameRule, Transaction};

    use super::BybitParser;

    /// Gives each concurrent test its own temp file (they share one process).
    static TEST_COUNTER: AtomicUsize = AtomicUsize::new(0);

    /// Writes `body` into a temporary HTML file and runs the parser over it.
    fn parse_html(cfg: &AppConfig, rows: &str) -> Vec<Transaction> {
        let unique = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "ass_acc_bybit_{}_{}.html",
            std::process::id(),
            unique
        ));
        let html = format!("<html><body><table><tbody>{rows}</tbody></table></body></html>");
        std::fs::write(&path, html).unwrap();

        let mut parser = BybitParser::new(path.clone(), ".");
        parser.parse(cfg).unwrap();

        std::fs::remove_file(&path).ok();
        parser.transactions().to_vec()
    }

    /// One data row in the Bybit transaction table shape: six `<td>` cells,
    /// with the merchant/amount/status/datetime inside nested elements that the
    /// configured CSS selectors target.
    fn row(merchant: &str, amount: &str, status: &str, datetime: &str) -> String {
        format!(
            "<tr>\
                <td><div class=\"bycard__trans-table-merch-name-col\">{merchant}</div></td>\
                <td><p>{amount}</p></td>\
                <td>Type</td>\
                <td><span>{status}</span></td>\
                <td><span class=\"text-nowrap\">{datetime}</span></td>\
                <td>Action</td>\
            </tr>"
        )
    }

    /// Config with the same selectors the real `Settings/config.toml` uses.
    /// (`AppConfig::default()` leaves them empty: struct-default is not the
    /// same as the serde `default_*` config defaults.)
    fn cfg() -> AppConfig {
        AppConfig {
            bybit_selectors: BybitSelectors {
                merchant: ".bycard__trans-table-merch-name-col".to_string(),
                status: "span".to_string(),
                amount: "p".to_string(),
                datetime: "span.text-nowrap".to_string(),
            },
            ..AppConfig::default()
        }
    }

    #[test]
    fn extracts_fields_of_successful_rows() {
        let txs = parse_html(
            &cfg(),
            &format!(
                "{}{}",
                row(
                    "Telegram",
                    "-17.53 USD",
                    "Successful",
                    "2026-07-29 12:54:24"
                ),
                row("Foo", "-5.00 USD", "Failed", "2026-07-28 10:00:00")
            ),
        );

        assert_eq!(txs.len(), 1, "failed-status rows must be skipped");
        assert_eq!(txs[0].description, "Telegram");
        assert_eq!(txs[0].amount, "-17.53 USD");
        assert_eq!(txs[0].date_time, "2026-07-29 12:54:24");
        assert_eq!(txs[0].category, "?");
    }

    #[test]
    fn skips_rows_without_merchant() {
        let txs = parse_html(
            &cfg(),
            &format!(
                "{}{}",
                row("", "-1.00 USD", "Successful", "2026-07-29 12:54:24"),
                row("Shop", "-2.00 USD", "Successful", "2026-07-28 12:54:24")
            ),
        );

        assert_eq!(txs.len(), 1, "rows with an empty merchant must be skipped");
        assert_eq!(txs[0].description, "Shop");
    }

    #[test]
    fn applies_rename_rules_to_description() {
        let mut cfg = cfg();
        cfg.rules = vec![RenameRule {
            regex: Regex::new("^Telegram").unwrap(),
            category: "Messaging".to_string(),
            description: None,
            amount: None,
        }];

        let txs = parse_html(
            &cfg,
            &row(
                "Telegram",
                "-17.53 USD",
                "Successful",
                "2026-07-29 12:54:24",
            ),
        );

        assert_eq!(txs.len(), 1);
        assert_eq!(txs[0].category, "Messaging");
    }

    #[test]
    fn skips_transactions_older_than_last_parsed() {
        let mut cfg = cfg();
        cfg.last_parsed_datetime = Some(
            NaiveDateTime::parse_from_str("2026-07-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
        );

        let txs = parse_html(
            &cfg,
            &format!(
                "{}{}",
                row("Old", "-1.00 USD", "Successful", "2026-06-17 16:36:37"),
                row("New", "-2.00 USD", "Successful", "2026-07-29 12:54:24")
            ),
        );

        assert_eq!(txs.len(), 1, "old rows must be skipped by the cutoff");
        assert_eq!(txs[0].description, "New");
    }
}
