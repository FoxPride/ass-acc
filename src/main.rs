use regex::Regex;
use std::io;

pub struct Transaction {
    pub date_time: String,
    pub category: String,
    pub amount: String,
    pub description: String,
}

fn main() {
    println!("Parsing PDF file...");
    let bytes = match std::fs::read("Parsing/1 Original.pdf") {
        Ok(bytes) => bytes,
        Err(err) => panic!("Could not open file: {}", err),
    };
    let parsed = match pdf_extract::extract_text_from_mem(&bytes) {
        Ok(text) => text,
        Err(err) => panic!("Could not parse file: {}", err),
    };

    let mut result: Vec<Transaction> = vec![];
    let date_rg = Regex::new(r"^\d{1,2}\s+\w{3}\s+\d{2}\s+\d{2}:\d{2}").unwrap();
    let amount_rg = Regex::new(r"\d{1}\.\d{2}").unwrap();
    let mut channels: Vec<String> = vec![
        "Mobile".to_string(),
        "EDC".to_string(),
        "Foreign".to_string(),
        "Auto".to_string(),
    ];

    let mut continue_parse = false;
    let mut buffer = String::new();

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
                if channels.iter().any(|ch| part.starts_with(ch)) {
                    channel_idx = 5 + i;
                    break;
                }
            }

            if channel_idx == 0 {
                println!("Не найден канал на строке: {}", to_parse);
                println!("Известные каналы: {:?}", channels);
                println!("Введите новый канал: ");

                let mut new_channel = String::new();
                io::stdin().read_line(&mut new_channel).unwrap();
                let new_channel = new_channel.trim().to_string();

                if !new_channel.is_empty() {
                    channels.push(new_channel);
                }

                for (i, part) in parts[5..].iter().enumerate() {
                    if channels.iter().any(|ch| part.starts_with(ch)) {
                        channel_idx = 5 + i;
                        break;
                    }
                }

                if channel_idx == 0 {
                    println!("Не могу найти индекс канала");
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

            match description.as_str() {
                s if s.starts_with("True Money") => {
                    category = "Перевод".to_string();
                }
                s if s.starts_with("VILLA MARKET") => {
                    category = "Еда".to_string();
                    description = "Еда (магазин)".to_string();
                }
                s if s.to_lowercase().contains("cafe") => {
                    category = "Еда".to_string();
                    description = "Еда (кафе)".to_string();
                }
                s if s.starts_with("Metropolitan Electricity") => {
                    category = "Коммунальные платежи".to_string();
                    description = "Электричество".to_string();
                }
                s if s.starts_with("นิติบุคคลอาคารชุด") => {
                    category = "Коммунальные платежи".to_string();
                    description = "Вода".to_string();
                }
                s if s.starts_with("AIS Postpaid/AIS Fibre") => {
                    category = "Интернет".to_string();
                    description = "Интернет".to_string();
                }
                s if s.contains("น.ส. ชญานิน โกว") && amount == "-30,000.00" =>
                {
                    category = "Аренда".to_string();
                    description = "Оплата квартиры".to_string();
                }
                _ => {}
            }

            result.push(Transaction {
                date_time,
                category,
                amount,
                description,
            });
        }
    }

    // TODO add headers, save as csv with ';' delimiter

    for line in result {
        println!(
            "Time: {}, Tran: {}, Amount: {}, Det: {}",
            line.date_time, line.category, line.amount, line.description
        );
    }
}
