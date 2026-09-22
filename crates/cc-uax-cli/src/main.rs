use clap::Parser;
use std::process::ExitCode;

fn main() -> ExitCode {
    // Clap exits 2 for a usage error by default, which collides with the exit code
    // reserved for a project hard scan failure that *did* write a report. A bad
    // command line produced no report at all, so it belongs with the other exit-1
    // cases. Only an explicit `--help`/`--version` exits 0: a bare `cc-uax` with
    // no subcommand prints help too, but the caller asked for a report and did
    // not get one, which the contract lists as an invalid command line.
    let cli = match cc_uax_cli::args::Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            let usage_error = !matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            );
            let _ = error.print();
            return if usage_error {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            };
        }
    };
    cc_uax_cli::run(cli)
}
