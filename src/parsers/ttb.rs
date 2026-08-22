use std::path::PathBuf;

use anyhow::Context;
use regex::Regex;

use crate::{AppConfig, Parser, Transaction};

/// Date format from statement
const TTB_DATE_FORMAT: &str = "%-d %b %y %H:%M";

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

    fn parse(&mut self, cfg: &mut AppConfig, cfg_path: &str) -> anyhow::Result<()> {
        let bytes = std::fs::read(self.input.as_path())
            .with_context(|| format!("Error opening file: {:?}", self.input.as_path()))?;

        let parsed = pdf_extract::extract_text_from_mem(&bytes)
            .with_context(|| format!("Error processing file: {:?}", self.input.as_path()))?;

        let date_rg = Regex::new(r"^\d{1,2}\s+\w{3}\s+\d{2}\s+\d{2}:\d{2}").unwrap();
        let amount_rg = Regex::new(r"\d{1}\.\d{2}").unwrap();

        let mut continue_parse = false;
        let mut buffer = String::new();
        let mut update_config = false;

        for line in parsed.lines() {
            let is_transaction = date_rg.is_match_at(line, 0);

            if is_transaction || continue_parse {
                let to_parse = if continue_parse {
                    &format!("{buffer} {line}")
                } else {
                    line
                };

                let parts: Vec<&str> = to_parse.split_whitespace().collect();

                if !amount_rg.is_match_at(to_parse, 15) {
                    continue_parse = true;
                    buffer = to_parse.to_string();
                    continue;
                } else {
                    continue_parse = false;
                };

                let date_time = parts[0..4].join(" ");

                let mut channel_idx = 0;
                for (i, part) in parts[5..].iter().enumerate() {
                    if cfg.ttb_channels.iter().any(|ch| part.starts_with(ch)) {
                        channel_idx = 5 + i;
                        break;
                    }
                }

                if channel_idx == 0 {
                    println!("  Channel not found on line: {}", to_parse);
                    println!("  Known channels: {:?}", cfg.ttb_channels);
                    println!("  Enter a new channel: ");

                    let mut new_channel = String::new();
                    std::io::stdin().read_line(&mut new_channel).unwrap();
                    let new_channel = new_channel.trim().to_string();

                    if !new_channel.is_empty() {
                        cfg.ttb_channels.push(new_channel);
                        update_config = true;
                    }

                    for (i, part) in parts[5..].iter().enumerate() {
                        if cfg.ttb_channels.iter().any(|ch| part.starts_with(ch)) {
                            channel_idx = 5 + i;
                            break;
                        }
                    }

                    if channel_idx == 0 {
                        println!("  Cannot find channel index");
                        continue;
                    }
                }

                let mut amount_idx = 0;

                for (i, part) in parts[6..].iter().enumerate() {
                    if amount_rg.is_match(part) {
                        amount_idx = 6 + i;
                        break;
                    }
                }

                let category = parts[4..channel_idx].join(" ");
                let amount = parts[amount_idx].to_string();
                let description = if amount_idx + 2 < parts.len() {
                    parts[amount_idx + 2..].join(" ")
                } else {
                    String::new()
                };

                let mut transaction = Transaction {
                    date_time,
                    category,
                    amount,
                    description,
                };

                // Skip transactions that were already processed
                if !transaction.is_after(cfg.last_parsed_datetime, TTB_DATE_FORMAT) {
                    continue;
                }

                transaction.apply_rename_rules(&cfg.rules);

                self.transactions.push(transaction);
            }
        }

        if update_config {
            confy::store_path(cfg_path, cfg)?;
        }

        Ok(())
    }
}
