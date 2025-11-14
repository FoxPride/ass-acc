use csv::WriterBuilder;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::io;
use std::path::PathBuf;

#[derive(Default, Serialize, Deserialize)]
struct AppConfig {
    parse_folder: String,
    ttb_channels: Vec<String>,
    rules: Vec<RenameRule>,
}

#[derive(Serialize, Deserialize)]
struct RenameRule {
    #[serde(with = "serde_regex")]
    regex: Regex,
    category: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    amount: Option<String>,
}

pub struct Transaction {
    pub date_time: String,
    pub category: String,
    pub amount: String,
    pub description: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config_path = "Settings/config.toml";
    let mut cfg: AppConfig = confy::load_path(config_path)?;

    println!("Начинаю обработку транзакций TTB-банка...");

    let mut file = PathBuf::from(&cfg.parse_folder);
    file.push("1.pdf");

    let bytes = std::fs::read(file.as_path())
        .unwrap_or_else(|err| panic!("  Не могу открыть файл: {:?}\n Err: {}", file, err));
    let parsed = pdf_extract::extract_text_from_mem(&bytes)
        .unwrap_or_else(|err| panic!("  Не могу обработать файл: {:?}\n Err: {}", file, err));

    let mut result: Vec<Transaction> = vec![];
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
                io::stdin().read_line(&mut new_channel).unwrap();
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

            let mut category = parts[4..channel_idx].join(" ");
            let amount = parts[amount_idx].to_string();
            let mut description = if amount_idx + 2 < parts.len() {
                parts[amount_idx + 2..].join(" ")
            } else {
                String::new()
            };

            for rule in &cfg.rules {
                if !rule.regex.is_match(&description) {
                    continue;
                }

                if let Some(required_amount) = &rule.amount
                    && required_amount != &amount
                {
                    continue;
                }

                category = rule.category.clone();
                if let Some(desc) = &rule.description {
                    description = desc.clone();
                }
                break;
            }

            result.push(Transaction {
                date_time,
                category,
                amount,
                description,
            });
        }
    }

    if update_config {
        confy::store_path(config_path, cfg)?;
        println!("  Конфиг успешно сохранён");
    }

    let mut csv_writer = WriterBuilder::new()
        .delimiter(b';')
        .from_path("TTB.csv")
        .unwrap_or_else(|err| panic!("  Не могу записать csv-файл:  {}", err));
    csv_writer
        .write_record(["Date Time", "Category", "Amount", "Description"])
        .unwrap();

    for line in result {
        csv_writer
            .write_record([line.date_time, line.category, line.amount, line.description])
            .unwrap();
    }
    csv_writer.flush().unwrap();

    println!("Обработка транзакций TTB-банка успешно закончена");

    Ok(())
}
