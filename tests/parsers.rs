//! Integration tests that verify each parser produces the transactions recorded
//! in `example/output/*.csv` for the corresponding input in `example/parse/`.
//!
//! These tests exercise the full pipeline (extraction + rename rules) and act as
//! a safety net before/while refactoring the parsers.

use std::path::{Path, PathBuf};

use ass_acc::parsers::{bybit::BybitParser, truemoney::TrueMoneyParser, ttb::TTBParser};
use ass_acc::{AppConfig, Parser, Transaction};

/// Absolute path to the crate root, independent of the CWD tests run from.
fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Loads the real application config and disables the "skip old transactions"
/// filter so the expected fixtures are fully reproduced.
fn load_config() -> AppConfig {
    let config_path = manifest_dir().join("Settings").join("config.toml");
    let mut cfg: AppConfig = confy::load_path(config_path).expect("failed to load config");
    cfg.last_parsed_datetime = None;
    cfg
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

/// Asserts that `actual` transactions match `expected`, in order.
fn assert_transactions_eq(actual: &[Transaction], expected: &[Transaction]) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "transaction count mismatch:\nactual:\n{:#?}\nexpected:\n{:#?}",
        actual,
        expected
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
    let mut cfg = load_config();
    let input = manifest_dir()
        .join("example")
        .join("parse")
        .join("Bybit.html");

    let mut parser = BybitParser::new(input, &cfg.output_folder);
    parser.parse(&mut cfg).unwrap();

    let expected = expected_transactions("Bybit.csv");
    assert_transactions_eq(parser.transactions(), &expected);
}

#[test]
fn ttb_parser_matches_expected_output() {
    let mut cfg = load_config();
    let input = manifest_dir()
        .join("example")
        .join("parse")
        .join("TTB.pdf");

    let mut parser = TTBParser::new(input, &cfg.output_folder);
    parser.parse(&mut cfg).unwrap();

    let expected = expected_transactions("TTB.csv");
    assert_transactions_eq(parser.transactions(), &expected);
}

// The TrueMoney parser runs OCR over a screenshot, which requires the two RTEN
// models referenced by `ocr_models` in the config and takes a while. It is
// skipped by default; run it explicitly with:
//
//     cargo test -- --ignored truemoney
#[test]
#[ignore = "requires OCR models and is slow; run with `cargo test -- --ignored truemoney`"]
fn truemoney_parser_matches_expected_output() {
    let mut cfg = load_config();

    // The model paths in config.toml are machine-specific. Bail out gracefully
    // when they are absent instead of failing.
    let detection = &cfg.ocr_models.detection;
    let recognition = &cfg.ocr_models.recognition;
    if !Path::new(detection).exists() || !Path::new(recognition).exists() {
        eprintln!("skipping: OCR models not found at {detection:?} / {recognition:?}");
        return;
    }

    let input = manifest_dir().join("example").join("parse").join("Images");

    let mut parser = TrueMoneyParser::new(input, &cfg.output_folder);
    parser.parse(&mut cfg).unwrap();

    let expected = expected_transactions("TrueMoney.csv");
    assert_transactions_eq(parser.transactions(), &expected);
}
