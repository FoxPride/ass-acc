//! Integration tests that verify each parser produces the transactions recorded
//! in `example/output/*.csv` for the corresponding input in `example/parse/`.
//!
//! These tests exercise the full pipeline (extraction + rename rules) and act as
//! a safety net before/while refactoring the parsers.
//!
//! The tests are self-contained: they build their own [`AppConfig`] instead of
//! reading `Settings/config.toml`, so they run identically from a clean clone.
//! The synthetic fixtures in `example/` contain no personal data.

use std::path::{Path, PathBuf};

#[cfg(not(debug_assertions))]
use ass_acc::parsers::truemoney::TrueMoneyParser;
use ass_acc::parsers::{bybit::BybitParser, ttb::TTBParser};
use ass_acc::{AppConfig, OcrModels, Parser, Transaction};

/// Absolute path to the crate root, independent of the CWD tests run from.
fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Builds the config the fixtures were generated against.
///
/// No rename rules are used, so the expected CSVs are stable and contain no
/// personal categories. `ttb_channels` lists every channel that appears in the
/// synthetic TTB statement.
fn test_config() -> AppConfig {
    AppConfig {
        ttb_channels: vec!["Auto", "Mobile", "EDC"]
            .into_iter()
            .map(String::from)
            .collect(),
        ocr_models: OcrModels {
            detection: ocr_model_path("text-detection.rten"),
            recognition: ocr_model_path("text-recognition.rten"),
        },
        ..AppConfig::default()
    }
}

/// Location of an OCR model file.
///
/// Defaults to the crate root so a checked-out model file (or symlink) is
/// picked up automatically; set `OCR_MODELS_DIR` to point elsewhere without
/// hard-coding a machine-specific path in the repository.
fn ocr_model_path(name: &str) -> String {
    let dir = std::env::var("OCR_MODELS_DIR")
        .unwrap_or_else(|_| manifest_dir().to_string_lossy().into_owned());
    Path::new(&dir).join(name).to_string_lossy().into_owned()
}

/// Reads the expected transactions from one of the `example/output/*.csv`
/// fixtures (semicolon-delimited, header + one row per transaction).
fn expected_transactions(csv_name: &str) -> Vec<Transaction> {
    let path = manifest_dir().join("example").join("output").join(csv_name);
    read_expected_transactions(&path)
}

fn read_expected_transactions(path: &Path) -> Vec<Transaction> {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(b';')
        .from_path(path)
        .unwrap_or_else(|e| panic!("failed to open {}: {e}", path.display()));

    reader
        .records()
        .map(|record| {
            let record = record.expect("invalid CSV record");
            assert_eq!(
                record.len(),
                4,
                "expected 4 columns in {}, got {:?}",
                path.display(),
                record
            );
            Transaction {
                category: record[0].to_string(),
                description: record[1].to_string(),
                date_time: record[2].to_string(),
                amount: record[3].to_string(),
            }
        })
        .collect()
}

#[track_caller]
fn assert_transactions_eq(actual: &[Transaction], expected: &[Transaction]) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "transaction count mismatch (actual vs expected)\nactual: {actual:#?}\nexpected: {expected:#?}"
    );

    for (i, (actual_tx, expected_tx)) in actual.iter().zip(expected).enumerate() {
        assert_eq!(
            actual_tx.category, expected_tx.category,
            "transaction #{i} category mismatch"
        );
        assert_eq!(
            actual_tx.description, expected_tx.description,
            "transaction #{i} description mismatch"
        );
        assert_eq!(
            actual_tx.date_time, expected_tx.date_time,
            "transaction #{i} date_time mismatch"
        );
        assert_eq!(
            actual_tx.amount, expected_tx.amount,
            "transaction #{i} amount mismatch"
        );
    }
}

#[test]
fn bybit_parser_matches_expected_output() {
    let cfg = test_config();
    let input = manifest_dir()
        .join("example")
        .join("parse")
        .join("Bybit.html");

    let mut parser = BybitParser::new(input, &cfg.output_folder);
    parser.parse(&cfg).unwrap();

    let expected = expected_transactions("Bybit.csv");
    assert_transactions_eq(parser.transactions(), &expected);
}

#[test]
fn ttb_parser_matches_expected_output() {
    let cfg = test_config();
    let input = manifest_dir().join("example").join("parse").join("TTB.pdf");

    let mut parser = TTBParser::new(input, &cfg.output_folder);
    parser.parse(&cfg).unwrap();

    let expected = expected_transactions("TTB.csv");
    assert_transactions_eq(parser.transactions(), &expected);
}

// The TrueMoney parser runs OCR over a screenshot, which needs the two RTEN
// models and is slow under a debug build. The test is therefore compiled only
// in release builds. Run it with:
//
//     OCR_MODELS_DIR=/path/to/models cargo test --release truemoney
//
// The models default to <repo-root>/text-detection.rten and
// <repo-root>/text-recognition.rten when `OCR_MODELS_DIR` is unset.
#[cfg(not(debug_assertions))]
#[test]
fn truemoney_parser_matches_expected_output() {
    let cfg = test_config();

    // Model files are machine-specific and never committed; bail out gracefully
    // when they are absent instead of failing the suite.
    let detection = &cfg.ocr_models.detection;
    let recognition = &cfg.ocr_models.recognition;
    if !Path::new(detection).exists() || !Path::new(recognition).exists() {
        eprintln!(
            "skipping: OCR models not found at {detection:?} / {recognition:?}\n\
             set OCR_MODELS_DIR to the folder containing the .rten models"
        );
        return;
    }

    let input = manifest_dir().join("example").join("parse").join("Images");

    let mut parser = TrueMoneyParser::new(input, &cfg.output_folder);
    parser.parse(&cfg).unwrap();

    let expected = expected_transactions("TrueMoney.csv");
    assert_transactions_eq(parser.transactions(), &expected);
}
