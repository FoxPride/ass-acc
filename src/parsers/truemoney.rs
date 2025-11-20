use anyhow::Context;
use image::{DynamicImage, GenericImageView};
use ocrs::{ImageSource, OcrEngine, OcrEngineParams};
use regex::Regex;
use rten::Model;
use std::path::PathBuf;

use crate::{AppConfig, Parser, Transaction};

struct ImageRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(PartialEq, Clone)]
enum TransactionType {
    Date,
    Time,
    Description,
    Amount(bool),
    Search,
}

struct RegionConfig {
    bound_offset: u32,
    transaction_background: u8,
    date_background: u8,
    transaction_positive_pixel: u8,
}

impl Default for RegionConfig {
    fn default() -> Self {
        Self {
            bound_offset: 5,
            transaction_background: 255,
            date_background: 240,
            transaction_positive_pixel: 25,
        }
    }
}

struct RegionSearchMask {
    search_type: TransactionType,
    region_x: u32,
    region_y: u32,
    region_width: u32,
    region_height: u32,
    left_bound_start: u32,
    right_bound_start: u32,
    current_region_skip: u32,
    next_region_skip: i32,
    region_y_found: bool,
    empty_column_threshold: u32,
}

impl RegionSearchMask {
    fn new() -> Self {
        Self {
            search_type: TransactionType::Date,
            region_x: 50,
            region_y: 0,
            region_width: 70,
            region_height: 0,
            left_bound_start: 59,
            right_bound_start: 300,
            current_region_skip: 20,
            next_region_skip: 0,
            region_y_found: false,
            empty_column_threshold: 25,
        }
    }

    fn update_for_next_search(&mut self) {
        match self.search_type {
            TransactionType::Date => {
                self.search_type = TransactionType::Description;
                self.region_x = 240;
                self.region_width = 70;
                self.left_bound_start = 255;
                self.right_bound_start = 350;
                self.current_region_skip = 35;
                self.next_region_skip = 70 + 50;
                self.region_y_found = false;
                self.empty_column_threshold = 15;
            }
            TransactionType::Time => {
                self.search_type = TransactionType::Search;
                self.next_region_skip = 100;
            }
            TransactionType::Description => {
                self.search_type = TransactionType::Amount(false);
                self.region_x = 1290;
                self.region_width = 70;
                self.left_bound_start = 1290;
                self.right_bound_start = 1365;
                self.current_region_skip = 35;
                self.next_region_skip = -50;
                self.region_y_found = false;
                self.empty_column_threshold = 5;
            }
            TransactionType::Amount(_) => {
                self.search_type = TransactionType::Time;
                self.region_x = 307;
                self.region_width = 115;
                self.left_bound_start = 325;
                self.right_bound_start = 402;
                self.current_region_skip = 25;
                self.next_region_skip = 35;
                self.region_y_found = false;
                self.empty_column_threshold = 5;
            }
            TransactionType::Search => {
                self.search_type = TransactionType::Date;
                self.region_x = 50;
                self.region_width = 70;
                self.left_bound_start = 59;
                self.right_bound_start = 300;
                self.current_region_skip = 20;
                self.next_region_skip = 0;
                self.region_y_found = false;
                self.empty_column_threshold = 25;
            }
        }
    }
}

pub struct TrueMoneyParser {
    input: PathBuf,
    output: PathBuf,
    pub transactions: Vec<Transaction>,
}

impl TrueMoneyParser {
    pub fn new(input: std::path::PathBuf, output_folder: &str) -> Self {
        let mut output = PathBuf::from(output_folder);
        output.push("TrueMoney.csv");

        Self {
            input,
            output,
            transactions: Vec::new(),
        }
    }
}

impl Parser for TrueMoneyParser {
    fn get_output(&self) -> &PathBuf {
        &self.output
    }

    fn transactions(&self) -> &[Transaction] {
        &self.transactions
    }

    fn parse(&mut self, cfg: &mut AppConfig, _cfg_path: &str) -> anyhow::Result<()> {
        let (detection, recognition) = &cfg.ocr_models;
        let detection_model = Model::load_file(detection)
            .with_context(|| format!("Ошибка загрузки модели обнаружения: {detection}"))?;
        let recognition_model = Model::load_file(recognition)
            .with_context(|| format!("Ошибка загрузки модели распознавания: {recognition}"))?;

        let engine = OcrEngine::new(OcrEngineParams {
            detection_model: Some(detection_model),
            recognition_model: Some(recognition_model),
            ..Default::default()
        })
        .with_context(|| "Не удалось инициализировать OCR")?;

        let date_rg = Regex::new(r"(\d{1,2})\s+(\w+)\s+(\d{4})").unwrap();
        let time_rg = Regex::new(r"^([01]?\d|2[0-3]):([0-5]\d)$").unwrap();
        let amount_rg = Regex::new(r"([\d,\.]+)").unwrap();

        for entry in std::fs::read_dir(&self.input)?.filter_map(|e| e.ok()) {
            let path = entry.path();

            if path.is_file()
                && let Some(ext) = path.extension().and_then(|e| e.to_str())
                && ext == "jpg"
            {
                let img = image::open(path)?;

                let regions = find_regions(&img);

                // buffers for parsing
                let mut transaction = Transaction::default();
                let mut date = String::new();

                for (transaction_type, region) in regions {
                    let img = extract_region(&img, &region).to_rgb8();
                    let img_source = ImageSource::from_bytes(img.as_raw(), img.dimensions())?;
                    let ocr_input = engine.prepare_input(img_source)?;

                    let text = engine.get_text(&ocr_input).unwrap();

                    match transaction_type {
                        TransactionType::Date => {
                            if let Some(caps) = date_rg.captures(&text) {
                                let day = &caps[1];
                                let month = &caps[2];
                                let year = &caps[3];

                                date = format!("{} {} {}", day, month, year);
                            } else {
                                date = "??".to_string();
                                println!("Warning! Invalid date format: {text}");
                            }
                        }
                        TransactionType::Time => {
                            if let Some(caps) = time_rg.captures(&text) {
                                let hours = &caps[1];
                                let minutes = &caps[2];

                                transaction.date_time = format!("{} {}:{}", date, hours, minutes);
                            } else {
                                transaction.date_time = format!("{} ??:??", date);
                                println!("Warning! Invalid time format {text}");
                            }

                            transaction.apply_rename_rules(&cfg.rules);
                            self.transactions.push(transaction.clone());
                        }
                        TransactionType::Description => transaction.description = text,
                        TransactionType::Amount(is_positive) => {
                            let sign = if is_positive { "+" } else { "-" };
                            if let Some(caps) = amount_rg.captures(&text) {
                                let amount = &caps[1];
                                transaction.amount = format!("{sign}{amount}");
                            } else {
                                transaction.amount = "??".to_string();
                                println!("Warning! Invalid amount format {text}");
                            }
                        }
                        TransactionType::Search => (),
                    }
                }
            }
        }

        Ok(())
    }
}

fn find_regions(img: &DynamicImage) -> Vec<(TransactionType, ImageRegion)> {
    let config = RegionConfig::default();
    let mut state = RegionSearchMask::new();
    let mut regions = Vec::new();

    let mut y = 0u32;

    while y < img.height() {
        let (colored_pixels, should_skip) = set_top_bound(img, y, &mut state, &config);

        if should_skip {
            y += state.current_region_skip;
            continue;
        }

        if state.region_y_found && colored_pixels == 0 {
            // bottom bound
            state.region_height = y - state.region_y + config.bound_offset;

            set_left_bound(img, &mut state, &config);
            set_right_bound(img, &mut state, &config);

            regions.push((
                state.search_type.clone(),
                ImageRegion {
                    x: state.region_x,
                    y: state.region_y,
                    width: state.region_width,
                    height: state.region_height,
                },
            ));

            state.update_for_next_search();

            y = y.saturating_add_signed(state.next_region_skip);

            if y >= img.height() {
                break;
            }

            if state.search_type == TransactionType::Search {
                if img.get_pixel(state.region_x, y)[0] == config.transaction_background {
                    state.search_type = TransactionType::Date;
                }

                state.update_for_next_search();
            }
        }

        y += 1;
    }

    regions
}

fn set_top_bound(
    img: &DynamicImage,
    y: u32,
    state: &mut RegionSearchMask,
    config: &RegionConfig,
) -> (u32, bool) {
    let mut colored_pixels = 0u32;

    // Search for top bound by scanning horizontally
    for x in state.region_x..(state.region_x + state.region_width) {
        let pixel = img.get_pixel(x, y)[0];

        if pixel != config.date_background && pixel != config.transaction_background {
            if !state.region_y_found {
                // First time finding a matching pixel - set top bound and signal to skip ahead
                state.region_y = y - config.bound_offset;
                state.region_y_found = true;
                colored_pixels += 1;
                return (colored_pixels, true);
            } else {
                colored_pixels += 1;
            }

            // Handle amount sign detection
            if matches!(state.search_type, TransactionType::Amount(false))
                && pixel == config.transaction_positive_pixel
            {
                state.search_type = TransactionType::Amount(true);
            }
        }
    }

    (colored_pixels, false)
}

fn set_left_bound(img: &DynamicImage, state: &mut RegionSearchMask, config: &RegionConfig) {
    let mut empty_columns = 0u32;

    for x in (0..=state.left_bound_start).rev() {
        let colored_pixels =
            count_colored_pixels_in_column(img, x, state.region_y, state.region_height, config);

        if colored_pixels == 0 {
            empty_columns += 1;
        } else {
            empty_columns = 0;
        }

        if empty_columns > state.empty_column_threshold {
            state.region_x = x + state.empty_column_threshold - config.bound_offset;
            break;
        }
    }
}

fn set_right_bound(img: &DynamicImage, state: &mut RegionSearchMask, config: &RegionConfig) {
    let mut empty_columns = 0u32;

    for x in state.right_bound_start..img.width() {
        let colored_pixels =
            count_colored_pixels_in_column(img, x, state.region_y, state.region_height, config);

        if colored_pixels == 0 {
            empty_columns += 1;
        } else {
            empty_columns = 0;
        }

        if empty_columns > state.empty_column_threshold {
            state.region_width =
                x - state.region_x - state.empty_column_threshold + config.bound_offset;
            break;
        }
    }
}

fn count_colored_pixels_in_column(
    img: &DynamicImage,
    x: u32,
    start_y: u32,
    height: u32,
    config: &RegionConfig,
) -> u32 {
    (start_y..(start_y + height))
        .filter(|&y| {
            img.get_pixel(x, y)[0] != config.date_background
                && img.get_pixel(x, y)[0] != config.transaction_background
        })
        .count() as u32
}

fn extract_region(img: &DynamicImage, region: &ImageRegion) -> DynamicImage {
    img.crop_imm(region.x, region.y, region.width, region.height)
}
