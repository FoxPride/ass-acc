use std::path::PathBuf;

use crate::{
    traits::{AppConfig, Parser},
    ttb::TTBParser,
};

mod traits;
mod ttb;

fn main() -> anyhow::Result<()> {
    let config_path = "Settings/config.toml";
    let mut cfg: AppConfig = confy::load_path(config_path)?;

    std::fs::create_dir_all(&cfg.output_folder)?;

    match get_file(&cfg.input_folder, "pdf") {
        Ok(pdf) => {
            let mut ttb = TTBParser::new();
            let parsed = match ttb.parse(pdf, &mut cfg, config_path) {
                Ok(()) => true,
                Err(err) => {
                    println!("Ошибка обработки TTB-транзакций: {}", err);
                    false
                }
            };

            if parsed {
                let path = "Output/TTB.csv";
                match ttb.write_csv(path) {
                    Ok(()) => println!("Обработка транзакций TTB-банка успешно закончена"),
                    Err(err) => println!("Не удалось сохранить csv-файл {}: {}", path, err),
                };
            }
        }
        Err(e) => {
            println!("Ошибка: {}", e);
        }
    }

    Ok(())
}

fn get_file(dir: &str, extension: &str) -> anyhow::Result<PathBuf> {
    std::fs::read_dir(dir)?
        .filter_map(|res| res.ok())
        .map(|dir_entry| dir_entry.path())
        .find(|path| path.extension().is_some_and(|ext| ext == extension))
        .ok_or_else(|| anyhow::anyhow!("Не удалось найти файл с расширением '{}'", extension))
}
