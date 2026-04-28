//! Streaming JSONL reader.
//!
//! Yields one `serde_json::Value` per non-empty line. Empty / whitespace-only
//! lines are silently skipped — Claude Code transcripts occasionally include
//! them around session boundaries. Malformed lines surface as
//! `AppError::Parse` carrying the 1-based line number, so callers can keep
//! reading and report partial data.

use crate::error::{AppError, Result};
use serde_json::Value;
use std::io::BufRead;

pub fn read_records<R: BufRead>(reader: R) -> impl Iterator<Item = Result<Value>> {
    reader
        .lines()
        .enumerate()
        .filter_map(|(idx, line_res)| match line_res {
            Err(e) => Some(Err(AppError::from(e))),
            Ok(line) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    return None;
                }
                match serde_json::from_str::<Value>(trimmed) {
                    Ok(v) => Some(Ok(v)),
                    Err(e) => Some(Err(AppError::Parse(format!("line {}: {}", idx + 1, e)))),
                }
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn skips_blank_lines_and_yields_parsed_values() {
        let input = "\n{\"a\":1}\n   \n{\"b\":2}\n";
        let cursor = Cursor::new(input);
        let values: Vec<_> = read_records(cursor).collect::<Result<_>>().unwrap();
        assert_eq!(values.len(), 2);
        assert_eq!(values[0]["a"], 1);
        assert_eq!(values[1]["b"], 2);
    }

    #[test]
    fn malformed_line_surfaces_with_line_number() {
        let input = "{\"a\":1}\nnot-json\n{\"b\":2}\n";
        let cursor = Cursor::new(input);
        let results: Vec<_> = read_records(cursor).collect();
        assert_eq!(results.len(), 3);
        assert!(results[0].is_ok());
        let err = results[1].as_ref().unwrap_err().to_string();
        assert!(
            err.contains("line 2"),
            "expected line number in error: {err}"
        );
        assert!(results[2].is_ok());
    }
}
