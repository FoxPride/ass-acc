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

    let mut file_path = PathBuf::from(&cfg.input_folder);
    file_path.push("1.pdf");

    std::fs::create_dir_all(&cfg.output_folder)?;

    let mut ttb = TTBParser::new();
    let parsed = match ttb.parse(file_path, &mut cfg, config_path) {
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

    Ok(())
}
