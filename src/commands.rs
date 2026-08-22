use std::path::PathBuf;

use chrono::Local;
use reqwest::{header, multipart};

use ass_acc::parsers::{bybit::BybitParser, truemoney::TrueMoneyParser, ttb::TTBParser};
use ass_acc::{AppConfig, Parser};

enum ParserType {
    Pdf(PathBuf),
    Html(PathBuf),
    Images(PathBuf),
}

/// Parses every statement found in the input folder and writes the resulting
/// CSVs to the output folder.
pub fn parse(cfg: &mut AppConfig, cfg_path: &str) -> anyhow::Result<()> {
    let parsers = get_all_parsers(&cfg.input_folder)?;

    std::fs::create_dir_all(&cfg.output_folder)?;

    let mut errors = Vec::new();

    for parser_type in parsers {
        let mut parser: Box<dyn Parser> = match parser_type {
            ParserType::Pdf(pdf) => Box::new(TTBParser::new(pdf, &cfg.output_folder)),
            ParserType::Html(html) => Box::new(BybitParser::new(html, &cfg.output_folder)),
            ParserType::Images(images_folder) => {
                Box::new(TrueMoneyParser::new(images_folder, &cfg.output_folder))
            }
        };

        if let Err(err) = parser.parse(cfg) {
            errors.push(format!("{}: {err}", parser.name()));
            continue;
        }

        if parser.transactions().is_empty() {
            println!("No new transactions to process: {}", parser.name());
            continue;
        }

        if let Err(err) = parser.write_csv() {
            errors.push(format!(
                "{}: failed to save {:?}: {err}",
                parser.name(),
                parser.get_output()
            ));
            continue;
        }

        println!(
            "Transactions processed successfully: {:?}",
            parser.get_output()
        );
    }

    if !errors.is_empty() {
        anyhow::bail!(
            "Failed to process {} statement(s):\n  {}",
            errors.len(),
            errors.join("\n  ")
        );
    }

    // Advance the timestamp only when every statement succeeded, so failed
    // statements are retried on the next run instead of being skipped.
    cfg.last_parsed_datetime = Some(Local::now().naive_local());
    confy::store_path(cfg_path, cfg)?;

    Ok(())
}

fn get_all_parsers(input_folder: &str) -> anyhow::Result<Vec<ParserType>> {
    let mut parsers = Vec::new();

    for entry in std::fs::read_dir(input_folder)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                match ext {
                    "pdf" => parsers.push(ParserType::Pdf(path)),
                    "html" => parsers.push(ParserType::Html(path)),
                    _ => {}
                }
            }
        } else if path.is_dir() && path.file_name().and_then(|n| n.to_str()) == Some("Images") {
            parsers.push(ParserType::Images(path));
        }
    }

    if parsers.is_empty() {
        Err(anyhow::anyhow!(
            "No suitable files found in: {}",
            input_folder
        ))
    } else {
        Ok(parsers)
    }
}

/// Uploads every prepared CSV from the output folder to FireFly-III.
pub async fn upload(cfg: &mut AppConfig, address: &str) -> anyhow::Result<()> {
    let statements = get_all_statements(&cfg.output_folder)?;

    if statements.is_empty() {
        println!("Nothing to upload.");
        return Ok(());
    }

    let client = reqwest::Client::new();

    for statement in statements {
        upload_statement(
            &client,
            &cfg.access_token,
            &cfg.client_secret,
            statement,
            address,
        )
        .await?;
    }

    Ok(())
}

fn get_all_statements(input_folder: &str) -> anyhow::Result<Vec<PathBuf>> {
    let mut statements = Vec::new();

    for entry in std::fs::read_dir(input_folder)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file()
            && let Some(ext) = path.extension().and_then(|e| e.to_str())
            && ext == "csv"
        {
            if let Ok(metadata) = path.metadata()
                && metadata.created().unwrap() == metadata.modified().unwrap()
            {
                println!("Unedited csv file! Skipping {}", path.display());
                continue;
            }

            statements.push(path)
        }
    }

    if statements.is_empty() {
        Err(anyhow::anyhow!(
            "No suitable files found in: {}",
            input_folder
        ))
    } else {
        Ok(statements)
    }
}

async fn upload_statement(
    client: &reqwest::Client,
    access_token: &str,
    client_secret: &str,
    statement: PathBuf,
    address: &str,
) -> anyhow::Result<()> {
    let mut json = statement.clone();
    json.set_extension("json");

    if !json.exists() {
        println!("Config file not found: {}", json.display());
        return Ok(());
    }

    let mut headers = header::HeaderMap::new();
    headers.insert("Accept", "application/json".parse().unwrap());
    headers.insert(
        "Authorization",
        format!("Bearer {access_token}").parse().unwrap(),
    );

    let form = multipart::Form::new()
        .file("importable", statement)
        .await?
        .file("json", json)
        .await?;

    let content = client
        .post(format!(
            "http://{address}:8081/autoupload?secret={client_secret}"
        ))
        .headers(headers)
        .multipart(form)
        .send()
        .await?
        .text()
        .await?;

    println!("{content}");
    Ok(())
}

/// Clears the input and output folders, keeping upload templates and the
/// `Images` folder itself.
pub fn clear(cfg: &AppConfig) -> anyhow::Result<()> {
    // Remove everything from the input folder except the Images folder.
    for entry in std::fs::read_dir(&cfg.input_folder)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() {
            println!("Removing input file: {}", path.display());
            std::fs::remove_file(path)?;
        } else if path.is_dir() && path.file_name().and_then(|n| n.to_str()) == Some("Images") {
            for entry in std::fs::read_dir(path)? {
                let entry = entry?;
                let path = entry.path();
                println!("Removing image: {}", path.display());
                std::fs::remove_file(path)?;
            }
        }
    }

    // Remove everything from the output folder except upload templates.
    for entry in std::fs::read_dir(&cfg.output_folder)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file()
            && let Some(ext) = path.extension().and_then(|e| e.to_str())
        {
            match ext {
                "json" => println!("Keeping upload template: {}", path.display()),
                _ => {
                    println!("Removing output file: {}", path.display());
                    std::fs::remove_file(path)?;
                }
            }
        }
    }

    println!("Folders cleared.");
    Ok(())
}
