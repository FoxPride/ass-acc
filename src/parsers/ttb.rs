use std::path::PathBuf;

use anyhow::Context;
use regex::Regex;

use crate::{AppConfig, Parser, Transaction};

/// Date format used in TTB statements.
const TTB_DATE_FORMAT: &str = "%e %b %y %H:%M";

/// A transaction starts with `day month year time` (4 whitespace-separated
/// tokens).
const DATE_TIME_TOKENS: usize = 4;

/// Index of the first token that can be a channel. Tokens `0..=4` hold the
/// date/time and a category prefix that may also look like a channel name
/// (e.g. "Auto Deposit of DEBIT CARD REFUND").
const CHANNEL_SCAN_START: usize = 5;

/// Matches a signed, comma-aware decimal such as "-2,463.00".
const AMOUNT_TOKEN_RE: &str = r"^[-+]?[\d,]+\.\d{2}$";

pub struct TTBParser {
    input: PathBuf,
    output: PathBuf,
    pub transactions: Vec<Transaction>,
}

impl TTBParser {
    pub fn new(input: PathBuf, output_folder: &str) -> Self {
        let mut output = PathBuf::from(output_folder);
        output.push("TTB.csv");

        Self {
            input,
            output,
            transactions: Vec::new(),
        }
    }
}

impl Parser for TTBParser {
    fn name(&self) -> &'static str {
        "TTB"
    }

    fn get_output(&self) -> &PathBuf {
        &self.output
    }

    fn transactions(&self) -> &[Transaction] {
        &self.transactions
    }

    fn parse(&mut self, cfg: &AppConfig) -> anyhow::Result<()> {
        let bytes = std::fs::read(self.input.as_path())
            .with_context(|| format!("Error opening file: {:?}", self.input.as_path()))?;

        let parsed = pdf_extract::extract_text_from_mem(&bytes)
            .with_context(|| format!("Error processing file: {:?}", self.input.as_path()))?;

        let date_rg = Regex::new(r"^\d{1,2}\s+\w{3}\s+\d{2}\s+\d{2}:\d{2}").unwrap();
        let amount_rg = Regex::new(AMOUNT_TOKEN_RE).unwrap();

        let mut buffer = String::new();
        let mut in_transaction = false;

        for line in parsed.lines() {
            let is_new_transaction = date_rg.is_match_at(line, 0);

            if !is_new_transaction && !in_transaction {
                continue;
            }

            let candidate = if in_transaction {
                format!("{buffer} {line}")
            } else {
                line.to_string()
            };

            // Keep buffering continuation lines until an amount shows up, so
            // transactions that wrap across physical lines are reassembled.
            let has_amount = candidate.split_whitespace().any(|t| amount_rg.is_match(t));
            if !has_amount {
                in_transaction = true;
                buffer = candidate;
                continue;
            }

            in_transaction = false;
            buffer.clear();

            match extract_transaction(&candidate, cfg, &amount_rg) {
                Ok(transaction) => {
                    if !transaction.is_after(cfg.last_parsed_datetime, TTB_DATE_FORMAT) {
                        continue;
                    }
                    let mut transaction = transaction;
                    transaction.apply_rename_rules(&cfg.rules);
                    self.transactions.push(transaction);
                }
                Err(err) => eprintln!("  Skipping line: {err}"),
            }
        }

        Ok(())
    }
}

/// Parses one complete TTB transaction line into a [`Transaction`].
///
/// Returns `Err` for lines whose channel or amount cannot be recognised so
/// callers can report the problem instead of prompting on stdin.
fn extract_transaction(
    line: &str,
    cfg: &AppConfig,
    amount_rg: &Regex,
) -> Result<Transaction, String> {
    let parts: Vec<&str> = line.split_whitespace().collect();

    if parts.len() < DATE_TIME_TOKENS {
        return Err(format!(
            "Expected at least {DATE_TIME_TOKENS} tokens in \"{line}\""
        ));
    }

    let date_time = parts[0..DATE_TIME_TOKENS].join(" ");

    // Scan from CHANNEL_SCAN_START so a category prefix such as "Auto" is not
    // mistaken for the channel.
    let channel_idx = (CHANNEL_SCAN_START..parts.len())
        .find(|&i| cfg.ttb_channels.iter().any(|ch| parts[i].starts_with(ch)))
        .ok_or_else(|| {
            format!(
                "Unknown channel in \"{line}\" (known: {:?}); \
                 add it to `ttb_channels` in config.toml",
                cfg.ttb_channels
            )
        })?;

    // Find the amount by scanning after the channel rather than assuming a
    // fixed offset, because a channel name may itself wrap across lines
    // (e.g. "E-" + "Commerce").
    let amount_idx = (channel_idx + 1..parts.len())
        .find(|&i| amount_rg.is_match(parts[i]))
        .ok_or_else(|| format!("Amount not found in \"{line}\""))?;

    let category = parts[4..channel_idx].join(" ");
    let amount = parts[amount_idx].to_string();
    let description = if amount_idx + 2 < parts.len() {
        parts[amount_idx + 2..].join(" ")
    } else {
        String::new()
    };

    Ok(Transaction {
        date_time,
        category,
        amount,
        description,
    })
}

#[cfg(test)]
mod tests {
    use regex::Regex;

    use crate::AppConfig;

    use super::{AMOUNT_TOKEN_RE, extract_transaction};

    fn cfg() -> AppConfig {
        AppConfig {
            ttb_channels: vec!["Auto".to_string(), "Mobile".to_string(), "E-".to_string()],
            ..AppConfig::default()
        }
    }

    fn amount_rg() -> Regex {
        Regex::new(AMOUNT_TOKEN_RE).unwrap()
    }

    #[test]
    fn parses_standard_transaction() {
        let line = "13 Aug 26 19:49 Purchasing Mobile -2,463.00 12,741.94 WWW.GRAB.COM";
        let tx = extract_transaction(line, &cfg(), &amount_rg()).unwrap();
        assert_eq!(tx.date_time, "13 Aug 26 19:49");
        assert_eq!(tx.category, "Purchasing");
        assert_eq!(tx.amount, "-2,463.00");
        assert_eq!(tx.description, "WWW.GRAB.COM");
    }

    #[test]
    fn parses_channel_split_across_lines() {
        let line = "13 Aug 26 19:49 Purchasing E- Commerce -2,463.00 12,741.94 WWW.GRAB.COM";
        let tx = extract_transaction(line, &cfg(), &amount_rg()).unwrap();
        assert_eq!(tx.category, "Purchasing");
        assert_eq!(tx.amount, "-2,463.00");
        assert_eq!(tx.description, "WWW.GRAB.COM");
    }

    #[test]
    fn parses_category_that_starts_with_channel_name() {
        let line = "31 Jul 26 15:33 Auto Deposit of DEBIT CARD REFUND Auto +611.10 58,804.34 -";
        let tx = extract_transaction(line, &cfg(), &amount_rg()).unwrap();
        assert_eq!(tx.category, "Auto Deposit of DEBIT CARD REFUND");
        assert_eq!(tx.amount, "+611.10");
        assert_eq!(tx.description, "-");
    }

    #[test]
    fn errors_on_unknown_channel() {
        let line = "13 Aug 26 19:49 Purchasing Unknown -2,463.00 12,741.94 X";
        assert!(extract_transaction(line, &cfg(), &amount_rg()).is_err());
    }
}
