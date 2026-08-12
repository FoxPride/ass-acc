use anyhow::{Context, Result};
use chrono::NaiveDateTime;
use csv::WriterBuilder;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Custom serde format for NaiveDateTime: "%d-%m-%Y %H:%M"
pub mod datetime_format {
    use chrono::NaiveDateTime;
    use serde::{self, Deserialize, Deserializer, Serializer};

    const FORMAT: &str = "%d-%m-%Y %H:%M";

    pub fn serialize<S>(date: &Option<NaiveDateTime>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match date {
            Some(dt) => serializer.serialize_str(&dt.format(FORMAT).to_string()),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<NaiveDateTime>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: Option<String> = Option::deserialize(deserializer)?;
        match s {
            Some(ref str) if !str.is_empty() => NaiveDateTime::parse_from_str(str, FORMAT)
                .map(Some)
                .map_err(serde::de::Error::custom),
            _ => Ok(None),
        }
    }
}

pub mod parsers;

pub trait Parser {
    fn name(&self) -> &'static str;

    fn get_output(&self) -> &PathBuf;

    fn transactions(&self) -> &[Transaction];

    fn parse(&mut self, cfg: &mut AppConfig, cfg_path: &str) -> Result<()>;

    fn write_csv(&self) -> Result<()> {
        let mut csv_writer = WriterBuilder::new()
            .delimiter(b';')
            .from_path(self.get_output())
            .with_context(|| format!("Error opening file: {:?}", self.get_output()))?;

        csv_writer.write_record(["Category", "Description", "Date Time", "Amount"])?;

        for transaction in self.transactions() {
            csv_writer.write_record(transaction.to_csv_row())?;
        }

        Ok(())
    }
}

#[derive(Debug, Default, Clone)]
pub struct Transaction {
    pub date_time: String,
    pub category: String,
    pub amount: String,
    pub description: String,
}

impl Transaction {
    fn to_csv_row(&self) -> Vec<&str> {
        vec![
            &self.category,
            &self.description,
            &self.date_time,
            &self.amount,
        ]
    }

    pub fn apply_rename_rules(&mut self, rules: &[RenameRule]) {
        for rule in rules {
            if !rule.regex.is_match(&self.description) {
                continue;
            }

            if let Some(required_amount) = &rule.amount
                && required_amount != &self.amount
            {
                continue;
            }

            self.category = rule.category.clone();
            if let Some(desc) = &rule.description {
                self.description = desc.clone();
            }
            break;
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct AppConfig {
    pub input_folder: String,
    pub output_folder: String,
    #[serde(with = "datetime_format", default)]
    pub last_parsed_datetime: Option<NaiveDateTime>,
    pub ocr_models: (String, String),
    pub ttb_channels: Vec<String>,
    pub rules: Vec<RenameRule>,
    pub access_token: String,
    pub client_secret: String,
    pub bybit_selectors: BybitSelectors,
}

#[derive(Default, Serialize, Deserialize)]
pub struct BybitSelectors {
    pub merchant: String,
    pub status: String,
    pub amount: String,
    pub datetime: String,
}

#[derive(Serialize, Deserialize)]
pub struct RenameRule {
    #[serde(with = "serde_regex")]
    pub regex: Regex,
    pub category: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub amount: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(regex: &str, category: &str) -> RenameRule {
        RenameRule {
            regex: Regex::new(regex).unwrap(),
            category: category.to_string(),
            description: None,
            amount: None,
        }
    }

    fn tx() -> Transaction {
        Transaction {
            date_time: "06 July 2026 13:25".to_string(),
            category: "?".to_string(),
            amount: "-20.00".to_string(),
            description: "True Money Wallet".to_string(),
        }
    }

    #[test]
    fn to_csv_row_uses_firefly_column_order() {
        let t = tx();
        assert_eq!(
            t.to_csv_row(),
            vec!["?", "True Money Wallet", "06 July 2026 13:25", "-20.00"]
        );
    }

    #[test]
    fn rename_rule_updates_category_only_by_default() {
        let mut t = tx();
        t.apply_rename_rules(&[rule("^True Money", "Transfer out")]);
        assert_eq!(t.category, "Transfer out");
        // description untouched when rule has no replacement
        assert_eq!(t.description, "True Money Wallet");
    }

    #[test]
    fn rename_rule_replaces_description_when_present() {
        let mut t = tx();
        let mut r = rule("^True Money", "Transfer out");
        r.description = Some("TrueMoney".to_string());
        t.apply_rename_rules(&[r]);
        assert_eq!(t.category, "Transfer out");
        assert_eq!(t.description, "TrueMoney");
    }

    #[test]
    fn rename_rule_skips_non_matching_regex() {
        let mut t = tx();
        t.apply_rename_rules(&[rule("^VILLA MARKET", "Food")]);
        assert_eq!(t.category, "?");
        assert_eq!(t.description, "True Money Wallet");
    }

    #[test]
    fn rename_rule_amount_filter_rejects_mismatch() {
        let mut t = tx();
        let mut r = rule("^True Money", "Transfer out");
        r.amount = Some("-100.00".to_string());
        t.apply_rename_rules(&[r]);
        assert_eq!(t.category, "?");
    }

    #[test]
    fn rename_rule_amount_filter_accepts_match() {
        let mut t = tx();
        let mut r = rule("^True Money", "Transfer out");
        r.amount = Some("-20.00".to_string());
        t.apply_rename_rules(&[r]);
        assert_eq!(t.category, "Transfer out");
    }

    #[test]
    fn rename_rules_first_match_wins() {
        let mut t = tx();
        let rules = vec![rule("^True Money", "First"), rule("^True", "Second")];
        t.apply_rename_rules(&rules);
        assert_eq!(t.category, "First");
    }
}
