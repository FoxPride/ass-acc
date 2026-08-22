use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::Local;
use reqwest::{header, multipart};

use ass_acc::parsers::{bybit::BybitParser, truemoney::TrueMoneyParser, ttb::TTBParser};
use ass_acc::{AppConfig, Parser};

enum ParserType {
    Pdf(PathBuf),
    Html(PathBuf),
    Images(PathBuf),
}

/// Name of the file that records the mtime of each generated CSV so `upload`
/// can tell which files haven't been edited since `parse` ran.
const MANIFEST_FILENAME: &str = "parsed_manifest";

fn manifest_path(output_folder: &str) -> PathBuf {
    PathBuf::from(output_folder).join(MANIFEST_FILENAME)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string()
}

fn mtime_secs(path: &Path) -> anyhow::Result<u64> {
    let modified = std::fs::metadata(path)?.modified()?;
    Ok(modified.duration_since(std::time::UNIX_EPOCH)?.as_secs())
}

fn read_manifest(output_folder: &str) -> BTreeMap<String, u64> {
    let path = manifest_path(output_folder);
    let Ok(contents) = std::fs::read_to_string(path) else {
        return BTreeMap::new();
    };

    contents
        .lines()
        .filter_map(|line| {
            let (name, mtime) = line.split_once('=')?;
            Some((name.to_string(), mtime.parse::<u64>().ok()?))
        })
        .collect()
}

fn write_manifest(output_folder: &str, manifest: &BTreeMap<String, u64>) -> anyhow::Result<()> {
    let mut contents = String::new();
    for (name, mtime) in manifest {
        contents.push_str(name);
        contents.push('=');
        contents.push_str(&mtime.to_string());
        contents.push('\n');
    }
    std::fs::write(manifest_path(output_folder), contents)?;
    Ok(())
}

fn is_generated(path: &Path, manifest: &BTreeMap<String, u64>) -> anyhow::Result<bool> {
    let Some(generated_mtime) = manifest.get(&file_name(path)) else {
        return Ok(false);
    };
    Ok(mtime_secs(path)? == *generated_mtime)
}

/// Parses every statement found in the input folder and writes the resulting
/// CSVs to the output folder.
pub fn parse(cfg: &mut AppConfig, cfg_path: &str) -> anyhow::Result<()> {
    let parsers = get_all_parsers(&cfg.input_folder)?;

    std::fs::create_dir_all(&cfg.output_folder)?;

    let mut manifest = read_manifest(&cfg.output_folder);
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

        let path = parser.get_output().clone();
        let mtime = mtime_secs(&path)?;
        manifest.insert(file_name(&path), mtime);

        println!("Transactions processed successfully: {:?}", path);
    }

    if !errors.is_empty() {
        anyhow::bail!(
            "Failed to process {} statement(s):\n  {}",
            errors.len(),
            errors.join("\n  ")
        );
    }

    write_manifest(&cfg.output_folder, &manifest)?;

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
            cfg.firefly_port,
            statement,
            address,
        )
        .await?;
    }

    Ok(())
}

fn get_all_statements(input_folder: &str) -> anyhow::Result<Vec<PathBuf>> {
    let manifest = read_manifest(input_folder);
    let mut statements = Vec::new();

    for entry in std::fs::read_dir(input_folder)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file()
            && let Some(ext) = path.extension().and_then(|e| e.to_str())
            && ext == "csv"
        {
            if is_generated(&path, &manifest)? {
                println!("Unedited csv file! Skipping {}", path.display());
                continue;
            }

            statements.push(path)
        }
    }

    Ok(statements)
}

async fn upload_statement(
    client: &reqwest::Client,
    access_token: &str,
    client_secret: &str,
    port: u16,
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
            "http://{address}:{port}/autoupload?secret={client_secret}"
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

        if !path.is_file() {
            continue;
        }

        match path.extension().and_then(|e| e.to_str()) {
            Some("json") => println!("Keeping upload template: {}", path.display()),
            _ => {
                println!("Removing output file: {}", path.display());
                std::fs::remove_file(path)?;
            }
        }
    }

    println!("Folders cleared.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ass_acc_{name}_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn get_all_parsers_finds_supported_files() {
        let dir = temp_dir("parsers");
        std::fs::write(dir.join("a.pdf"), b"%PDF-1.4").unwrap();
        std::fs::write(dir.join("b.html"), b"<html></html>").unwrap();
        std::fs::write(dir.join("c.txt"), b"ignore").unwrap();
        std::fs::create_dir_all(dir.join("Images")).unwrap();

        let parsers = get_all_parsers(dir.to_str().unwrap()).unwrap();

        let pdfs = parsers
            .iter()
            .filter(|p| matches!(p, ParserType::Pdf(_)))
            .count();
        let html = parsers
            .iter()
            .filter(|p| matches!(p, ParserType::Html(_)))
            .count();
        let images = parsers
            .iter()
            .filter(|p| matches!(p, ParserType::Images(_)))
            .count();
        assert_eq!((pdfs, html, images), (1, 1, 1));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn get_all_statements_skips_unedited_files() {
        let dir = temp_dir("statements");
        let edited = dir.join("edited.csv");
        let generated = dir.join("generated.csv");
        std::fs::write(&edited, "a;b;c").unwrap();
        std::fs::write(&generated, "a;b;c").unwrap();

        let mut manifest = std::collections::BTreeMap::new();
        manifest.insert("generated.csv".to_string(), mtime_secs(&generated).unwrap());
        write_manifest(dir.to_str().unwrap(), &manifest).unwrap();

        let statements = get_all_statements(dir.to_str().unwrap()).unwrap();
        let names: Vec<String> = statements
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["edited.csv".to_string()]);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn get_all_statements_returns_empty_when_no_files() {
        let dir = temp_dir("empty");
        let statements = get_all_statements(dir.to_str().unwrap()).unwrap();
        assert!(statements.is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn upload_exits_successfully_when_nothing_to_upload() {
        let dir = temp_dir("upload_empty");
        let mut cfg = AppConfig {
            input_folder: dir.to_string_lossy().into_owned(),
            output_folder: dir.to_string_lossy().into_owned(),
            ..Default::default()
        };

        upload(&mut cfg, "127.0.0.1").await.unwrap();

        std::fs::remove_dir_all(&dir).ok();
    }
}
