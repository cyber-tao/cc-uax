use clap::Parser;
use std::process::ExitCode;

fn main() -> ExitCode {
    cc_uax_cli::run(cc_uax_cli::args::Cli::parse())
}
