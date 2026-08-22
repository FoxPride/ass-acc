use std::path::PathBuf;

use anyhow::Context;
use image::{DynamicImage, GenericImageView};
use ocrs::{ImageSource, OcrEngine, OcrEngineParams};
use regex::Regex;
use rten::Model;
use rten_imageproc::{BoundingRect, RotatedRect};

use crate::{AppConfig, Parser, Transaction};

/// Date format from statement
const TRUEMONEY_DATE_FORMAT: &str = "%d %B %Y %H:%M";

/// Cells whose horizontal center is left of this x belong to the date column.
const DATE_COLUMN_MAX_X: i32 = 200;

/// Cells whose horizontal center is right of this x belong to the amount column.
const AMOUNT_COLUMN_MIN_X: i32 = 900;

/// Padding (in pixels) added around a detected text box before recognition.
const CELL_PADDING: i32 = 6;

/// A column of a TrueMoney transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Column {
    Date,
    Description,
    Time,
    Amount,
}

/// A piece of recognized text together with the column it belongs to and its
/// vertical position on the statement.
struct DetectedCell {
    column: Column,
    text: String,
    top: i32,
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
    fn name(&self) -> &'static str {
        "TrueMoney"
    }

    fn get_output(&self) -> &PathBuf {
        &self.output
    }

    fn transactions(&self) -> &[Transaction] {
        &self.transactions
    }

    fn parse(&mut self, cfg: &mut AppConfig, _cfg_path: &str) -> anyhow::Result<()> {
        let engine = build_engine(cfg)?;

        let date_rg = Regex::new(r"(\d{1,2})\s+(\w+)\s+(\d{4})").unwrap();
        let time_rg = Regex::new(r"([01]?\d|2[0-3]):([0-5]\d)").unwrap();
        let amount_rg = Regex::new(r"(\d[\d,]*\.\d{1,2})").unwrap();

        for entry in std::fs::read_dir(&self.input)?.filter_map(|e| e.ok()) {
            let path = entry.path();

            if path.is_file()
                && let Some(ext) = path.extension().and_then(|e| e.to_str())
                && ext == "jpg"
            {
                let img = image::open(path)?;
                let cells = detect_cells(&engine, &img, &time_rg)?;

                // Buffers for the transaction currently being assembled. The date
                // header is carried forward until a new one is seen, since several
                // transactions can share the same date header.
                let mut current_date = String::new();
                let mut current_description = String::new();
                let mut current_amount = String::new();

                for cell in cells {
                    match cell.column {
                        Column::Date => {
                            if let Some(caps) = date_rg.captures(&cell.text) {
                                current_date = format!("{} {} {}", &caps[1], &caps[2], &caps[3]);
                            }
                        }
                        Column::Description => current_description = cell.text,
                        Column::Amount => {
                            current_amount = amount_rg
                                .captures(&cell.text)
                                .map(|caps| format!("-{}", &caps[1]))
                                .unwrap_or_else(|| "??".to_string());
                        }
                        Column::Time => {
                            let mut transaction = Transaction {
                                date_time: format!("{} ??:??", current_date),
                                category: String::new(),
                                amount: current_amount.clone(),
                                description: current_description.clone(),
                            };

                            if let Some(caps) = time_rg.captures(&cell.text) {
                                transaction.date_time =
                                    format!("{} {}:{}", current_date, &caps[1], &caps[2]);
                            }

                            // Skip transactions that were already processed
                            if !transaction.is_after(
                                cfg.last_parsed_datetime,
                                TRUEMONEY_DATE_FORMAT,
                            ) {
                                continue;
                            }

                            transaction.apply_rename_rules(&cfg.rules);
                            self.transactions.push(transaction);
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

fn build_engine(cfg: &AppConfig) -> anyhow::Result<OcrEngine> {
    let detection = &cfg.ocr_models.detection;
    let recognition = &cfg.ocr_models.recognition;
    let detection_model = Model::load_file(detection)
        .with_context(|| format!("Error loading detection model: {detection}"))?;
    let recognition_model = Model::load_file(recognition)
        .with_context(|| format!("Error loading recognition model: {recognition}"))?;

    OcrEngine::new(OcrEngineParams {
        detection_model: Some(detection_model),
        recognition_model: Some(recognition_model),
        ..Default::default()
    })
    .with_context(|| "Failed to initialize OCR")
}

/// Detects text boxes with the OCR detection model, recognizes each box
/// individually and classifies it into a transaction column. Results are
/// returned sorted top-to-bottom in reading order.
fn detect_cells(
    engine: &OcrEngine,
    img: &DynamicImage,
    time_rg: &Regex,
) -> anyhow::Result<Vec<DetectedCell>> {
    let (width, height) = img.dimensions();
    let rgb = img.to_rgb8();
    let input = engine.prepare_input(ImageSource::from_bytes(rgb.as_raw(), rgb.dimensions())?)?;

    let words = engine.detect_words(&input)?;
    let lines = engine.find_text_lines(&input, &words);

    let mut cells = Vec::new();

    for line in lines {
        let Some((left, top, right, bottom)) = line_bounds(&line) else {
            continue;
        };

        let x = (left - CELL_PADDING).max(0) as u32;
        let y = (top - CELL_PADDING).max(0) as u32;
        let w = ((right + CELL_PADDING).min(width as i32) as u32).saturating_sub(x);
        let h = ((bottom + CELL_PADDING).min(height as i32) as u32).saturating_sub(y);
        if w == 0 || h == 0 {
            continue;
        }

        let crop = img.crop_imm(x, y, w, h).to_rgb8();
        let crop_input =
            engine.prepare_input(ImageSource::from_bytes(crop.as_raw(), crop.dimensions())?)?;
        let text = engine.get_text(&crop_input)?.trim().to_string();
        if text.is_empty() {
            continue;
        }

        let center_x = (left + right) / 2;
        let column = classify_column(center_x, &text, time_rg);

        cells.push(DetectedCell { column, text, top });
    }

    cells.sort_by_key(|cell| cell.top);
    Ok(cells)
}

fn classify_column(center_x: i32, text: &str, time_rg: &Regex) -> Column {
    if center_x > AMOUNT_COLUMN_MIN_X {
        Column::Amount
    } else if center_x < DATE_COLUMN_MAX_X {
        Column::Date
    } else if time_rg.is_match(text) {
        Column::Time
    } else {
        Column::Description
    }
}

/// Returns the axis-aligned bounding box `(left, top, right, bottom)` of a line
/// of detected words.
fn line_bounds(line: &[RotatedRect]) -> Option<(i32, i32, i32, i32)> {
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;

    for word in line {
        let b = word.bounding_rect();
        min_x = min_x.min(b.left());
        min_y = min_y.min(b.top());
        max_x = max_x.max(b.right());
        max_y = max_y.max(b.bottom());
    }

    if min_x == f32::MAX {
        return None;
    }

    Some((
        min_x.floor() as i32,
        min_y.floor() as i32,
        max_x.ceil() as i32,
        max_y.ceil() as i32,
    ))
}
