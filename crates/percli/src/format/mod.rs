pub mod json;
pub mod status;
pub mod table;

use anyhow::Result;
use clap::ValueEnum;
use owo_colors::OwoColorize;
use percli_core::EngineSnapshot;

use percli_core::scenario::runner::{RunResult, StepOutcome};

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum OutputFormat {
    Table,
    Json,
}

impl OutputFormat {
    pub fn from_env() -> Self {
        match std::env::var("PERC_FORMAT").ok().as_deref() {
            Some("json") => OutputFormat::Json,
            _ => OutputFormat::Table,
        }
    }
}

pub fn print_run_result(result: &RunResult, format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Table => {
            let total = result.step_results.len();
            for sr in &result.step_results {
                let status = match &sr.outcome {
                    StepOutcome::Ok => format!(
                        "{} {}",
                        "\u{2713}".if_supports_color(owo_colors::Stream::Stdout, |t| t.green()),
                        "ok".if_supports_color(owo_colors::Stream::Stdout, |t| t.green())
                    ),
                    StepOutcome::Warning(w) => format!(
                        "{} {}: {w}",
                        "\u{25b2}".if_supports_color(owo_colors::Stream::Stdout, |t| t.yellow()),
                        "warn".if_supports_color(owo_colors::Stream::Stdout, |t| t.yellow())
                    ),
                    StepOutcome::QueryResult(_) => format!(
                        "{} {}",
                        "\u{25cf}".if_supports_color(owo_colors::Stream::Stdout, |t| t.cyan()),
                        "query".if_supports_color(owo_colors::Stream::Stdout, |t| t.cyan())
                    ),
                    StepOutcome::AssertPassed => format!(
                        "{} {}",
                        "\u{2713}".if_supports_color(owo_colors::Stream::Stdout, |t| t.green()),
                        "pass".if_supports_color(owo_colors::Stream::Stdout, |t| t.green())
                    ),
                    StepOutcome::AssertFailed(msg) => format!(
                        "{} {}: {msg}",
                        "\u{2717}".if_supports_color(owo_colors::Stream::Stdout, |t| t.red()),
                        "FAIL".if_supports_color(owo_colors::Stream::Stdout, |t| t.red())
                    ),
                };

                let prefix = format!(
                    "{:>8}",
                    format!("{}/{total}", sr.step_num)
                        .if_supports_color(owo_colors::Stream::Stdout, |t| t.dimmed())
                );
                println!("{prefix}  {:<52} {status}", sr.description);

                if let Some(delta) = &sr.delta {
                    table::print_delta(delta);
                }

                if let StepOutcome::QueryResult(snap) = &sr.outcome {
                    println!();
                    table::print_snapshot(snap);
                    println!();
                } else if let Some(snap) = &sr.snapshot {
                    println!();
                    table::print_snapshot(snap);
                    println!();
                }
            }

            println!();
            table::print_snapshot(&result.final_snapshot);
            Ok(())
        }
        OutputFormat::Json => json::print_run_result(result),
    }
}

pub fn print_snapshot(snap: &EngineSnapshot, format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Table => {
            table::print_snapshot(snap);
            Ok(())
        }
        OutputFormat::Json => json::print_snapshot(snap),
    }
}
