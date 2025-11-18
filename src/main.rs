use clap::{Parser as ClapParser, Subcommand};
use std::path::PathBuf;

use crate::{
    bybit::BybitParser,
    traits::{AppConfig, Parser},
    ttb::TTBParser,
};

mod bybit;
mod traits;
mod ttb;

#[derive(ClapParser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Parse statements to Output folder
    Parse,
    Upload,
}

enum ParserType {
    Pdf(PathBuf),
    Html(PathBuf),
    // OCR(PathBuf),
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let config_path = "Settings/config.toml";
    let mut cfg: AppConfig = confy::load_path(config_path)?;

    match cli.command {
        Commands::Parse => parse_statements(&mut cfg, &config_path),
        Commands::Upload => todo!(),
    }
}

fn parse_statements(cfg: &mut AppConfig, cfg_path: &str) -> anyhow::Result<()> {
    let parsers = detect_all_parsers(&cfg.input_folder)?;

    if !parsers.is_empty() {
        std::fs::create_dir_all(&cfg.output_folder)?;
    }

    for parser_type in parsers {
        let mut parser: Box<dyn Parser> = match parser_type {
            ParserType::Pdf(pdf) => Box::new(TTBParser::new(pdf, &cfg.output_folder)),
            ParserType::Html(html) => Box::new(BybitParser::new(html, &cfg.output_folder)),
            // ParserType::OCR(images_path) => Box::new(OCRParser::new(images_path, &cfg.output_folder)),
        };

        let is_parsed = match parser.parse(cfg, cfg_path) {
            Ok(()) => true,
            Err(err) => {
                println!("Ошибка обработки транзакций: {}", err);
                false
            }
        };

        if is_parsed {
            match parser.write_csv() {
                Ok(()) => println!(
                    "Обработка транзакций успешно закончена: {:?}",
                    parser.get_output()
                ),
                Err(err) => println!(
                    "Не удалось сохранить csv-файл {:?}: {}",
                    parser.get_output(),
                    err
                ),
            };
        }
    }

    Ok(())
}

fn detect_all_parsers(input_folder: &str) -> anyhow::Result<Vec<ParserType>> {
    let mut parsers = Vec::new();

    for entry in std::fs::read_dir(input_folder)?.filter_map(|e| e.ok()) {
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
            println!("TODO: OCR parser");
            // parsers.push(ParserType::OCR(path));
        }
    }

    if parsers.is_empty() {
        Err(anyhow::anyhow!(
            "Не найдено подходящих файлов в: {}",
            input_folder
        ))
    } else {
        Ok(parsers)
    }
}
