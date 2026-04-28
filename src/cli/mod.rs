use clap::Parser;
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "prompt-bom", version, about)]
struct Cli {}

pub fn run() -> ExitCode {
    let _cli = Cli::parse();
    ExitCode::SUCCESS
}
