use clap::{Args, Parser as ClapParser, Subcommand};
use reqwest::{header, multipart};
use std::path::PathBuf;

use ass_acc::{
    AppConfig, Parser,
    parsers::{bybit::BybitParser, truemoney::TrueMoneyParser, ttb::TTBParser},
};

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

    /// Upload csv from Output folder to FireFly-III
    Upload(AddArgs),
}

#[derive(Args)]
struct AddArgs {
    address: Option<String>,
}

enum ParserType {
    Pdf(PathBuf),
    Html(PathBuf),
    Images(PathBuf),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let config_path = "Settings/config.toml";
    let mut cfg: AppConfig = confy::load_path(config_path)?;

    match cli.command {
        Commands::Parse => parse_statements(&mut cfg, config_path),
        Commands::Upload(args) => match args.address {
            Some(address) => upload_statements(&mut cfg, &address).await,
            None => panic!("Ошибка: Укажите адрес загрузки!"),
        },
    }
}

fn parse_statements(cfg: &mut AppConfig, cfg_path: &str) -> anyhow::Result<()> {
    let parsers = get_all_parsers(&cfg.input_folder)?;

    if !parsers.is_empty() {
        std::fs::create_dir_all(&cfg.output_folder)?;
    }

    for parser_type in parsers {
        let mut parser: Box<dyn Parser> = match parser_type {
            ParserType::Pdf(pdf) => Box::new(TTBParser::new(pdf, &cfg.output_folder)),
            ParserType::Html(html) => Box::new(BybitParser::new(html, &cfg.output_folder)),
            ParserType::Images(images_folder) => {
                Box::new(TrueMoneyParser::new(images_folder, &cfg.output_folder))
            }
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

fn get_all_parsers(input_folder: &str) -> anyhow::Result<Vec<ParserType>> {
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
            parsers.push(ParserType::Images(path));
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

async fn upload_statements(cfg: &mut AppConfig, address: &str) -> anyhow::Result<()> {
    let statements = get_all_statements(&cfg.output_folder)?;

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

    for entry in std::fs::read_dir(input_folder)?.filter_map(|e| e.ok()) {
        let path = entry.path();

        if path.is_file()
            && let Some(ext) = path.extension().and_then(|e| e.to_str())
            && ext == "csv"
        {
            if let Ok(metadata) = path.metadata()
                && metadata.created().unwrap() == metadata.modified().unwrap()
            {
                println!("Неотредактированный csv-файл! Пропускаю {}", path.display());
                continue;
            }

            statements.push(path)
        }
    }

    if statements.is_empty() {
        Err(anyhow::anyhow!(
            "Не найдено подходящих файлов в: {}",
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
        println!("Не найден файл конфигурации: {}", json.display());
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
            "http://{address}:81/autoupload?secret={client_secret}"
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
