//! Command-line dispatch.
//!
//! Exit codes:
//! - 0: success.
//! - 1: user input error (bad path, missing arg, unreadable file).
//! - 2: internal error (parser bug, unexpected git/serde failure).

use crate::error::{AppError, Result};
use crate::services::{
    blame::find_attributed_ranges,
    spdx_emit::{collect_file_contents, emit_spdx, EmitOptions},
    transcript::parse_claude_code_jsonl,
};
use clap::{Parser, Subcommand};
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use std::process::ExitCode;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "prompt-bom", version, about)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Parse a Claude Code transcript, join with git blame, emit SPDX 2.3 JSON.
    Emit {
        /// Path to a Claude Code transcript JSONL file.
        #[arg(long)]
        transcript: PathBuf,

        /// Repository root used for blame and file content lookup.
        #[arg(long, default_value = ".")]
        repo: PathBuf,

        /// Output SPDX JSON file. File output only — anti-scope: no stdout.
        #[arg(long)]
        out: PathBuf,

        /// Project name placed in the SPDX document.
        #[arg(long)]
        name: String,

        /// Document namespace URI. If omitted, derived deterministically from
        /// `<name>` and the document timestamp.
        #[arg(long)]
        namespace: Option<String>,

        /// RFC 3339 timestamp for `creationInfo.created`. If omitted, the
        /// current UTC time is used. Pin this in scripts that need byte-stable
        /// output across runs.
        #[arg(long)]
        created: Option<String>,
    },
}

pub fn run() -> ExitCode {
    init_tracing();
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Emit {
            transcript,
            repo,
            out,
            name,
            namespace,
            created,
        } => match emit_command(&transcript, &repo, &out, &name, namespace, created) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                tracing::error!("emit failed: {err}");
                exit_code_for(&err)
            }
        },
    }
}

fn emit_command(
    transcript: &std::path::Path,
    repo: &std::path::Path,
    out: &std::path::Path,
    name: &str,
    namespace: Option<String>,
    created: Option<String>,
) -> Result<()> {
    let records = parse_claude_code_jsonl(transcript)?;
    let ranges = find_attributed_ranges(&records, repo)?;
    let file_contents = collect_file_contents(&ranges, repo)?;

    let created_iso = created.unwrap_or_else(now_iso);
    let document_namespace = namespace.unwrap_or_else(|| derive_namespace(name, &created_iso));
    let opts = EmitOptions {
        project_name: name.to_string(),
        document_namespace,
        created_iso,
        tool_id: format!("Tool: prompt-bom-{}", env!("CARGO_PKG_VERSION")),
    };

    let doc = emit_spdx(&ranges, &file_contents, &opts)?;

    let file = File::create(out)?;
    serde_json::to_writer_pretty(BufWriter::new(file), &doc)?;
    Ok(())
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .try_init();
}

fn exit_code_for(err: &AppError) -> ExitCode {
    match err {
        AppError::Cli(_) => ExitCode::from(1),
        AppError::Io(_) | AppError::Config(_) => ExitCode::from(1),
        AppError::Parse(_) | AppError::Git(_) | AppError::Serde(_) => ExitCode::from(2),
    }
}

fn now_iso() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format_iso_utc(secs as i64)
}

fn format_iso_utc(unix_secs: i64) -> String {
    // Minimal RFC 3339 (UTC) formatter — avoids pulling in chrono just for this.
    // Adapted from civil-time arithmetic; valid for years 1970..=9999.
    let mut s = unix_secs;
    let days = s.div_euclid(86_400);
    s = s.rem_euclid(86_400);
    let hour = s / 3600;
    let minute = (s % 3600) / 60;
    let second = s % 60;
    let (year, month, day) = civil_date(days);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn civil_date(days_since_epoch: i64) -> (i64, u32, u32) {
    // From Howard Hinnant's "date" library — public domain. Days are
    // measured from 1970-01-01.
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if m <= 2 { y + 1 } else { y };
    (year, m, d)
}

fn derive_namespace(name: &str, created_iso: &str) -> String {
    let hash = blake3::hash(format!("{name}|{created_iso}").as_bytes());
    let short = &hash.to_hex()[..16];
    format!("https://verepdev.github.io/{name}/{short}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_date_unix_epoch() {
        assert_eq!(civil_date(0), (1970, 1, 1));
    }

    #[test]
    fn civil_date_y2k() {
        assert_eq!(civil_date(10_957), (2000, 1, 1));
    }

    #[test]
    fn civil_date_leap_day() {
        assert_eq!(civil_date(11_016), (2000, 2, 29));
    }

    #[test]
    fn format_iso_utc_unix_epoch() {
        assert_eq!(format_iso_utc(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn format_iso_utc_y2k() {
        assert_eq!(format_iso_utc(946_684_800), "2000-01-01T00:00:00Z");
    }

    #[test]
    fn derive_namespace_is_deterministic_and_prefixed() {
        let a = derive_namespace("foo", "2026-05-02T00:00:00Z");
        let b = derive_namespace("foo", "2026-05-02T00:00:00Z");
        assert_eq!(a, b);
        assert!(a.starts_with("https://verepdev.github.io/foo/"));
    }
}
