use std::path::PathBuf;

use anyhow::Context;
use regex::Regex;

use crate::traits::{AppConfig, Parser, Transaction};

pub struct TTBTransaction {
    date_time: String,
    category: String,
    amount: String,
    description: String,
}

impl Transaction for TTBTransaction {
    fn to_csv_row(&self) -> Vec<&str> {
        vec![
            &self.date_time,
            &self.category,
            &self.amount,
            &self.description,
        ]
    }

    fn apply_rename_rules(&mut self, rules: &[crate::traits::RenameRule]) {
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

pub struct TTBParser {
    pub transactions: Vec<TTBTransaction>,
}

impl TTBParser {
    pub fn new() -> Self {
        Self {
            transactions: Vec::new(),
        }
    }
}

impl Parser for TTBParser {
    type Transaction = TTBTransaction;

    fn csv_header(&self) -> Vec<&'static str> {
        ["Date Time", "Category", "Amount", "Description"].to_vec()
    }

    fn transactions(&self) -> &[Self::Transaction] {
        &self.transactions
    }

    fn parse(&mut self, path: PathBuf, cfg: &mut AppConfig, cfg_path: &str) -> anyhow::Result<()> {
        let bytes = std::fs::read(path.as_path())
            .with_context(|| format!("Ошибка открытия файла: {:?}", path.as_path()))?;

        let parsed = pdf_extract::extract_text_from_mem(&bytes)
            .with_context(|| format!("Ошибка обработки файла: {:?}", path.as_path()))?;

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
                    println!("  Не найден канал на строке: {}", to_parse);
                    println!("  Известные каналы: {:?}", &cfg.ttb_channels);
                    println!("  Введите новый канал: ");

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
                        println!("  Не могу найти индекс канала");
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

                let mut transaction = TTBTransaction {
                    date_time,
                    category,
                    amount,
                    description,
                };
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
