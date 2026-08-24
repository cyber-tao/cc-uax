use clap::Parser;
use std::process::ExitCode;

fn main() -> ExitCode {
    // Clap exits 2 for a usage error by default, which collides with the exit code
    // reserved for a project hard scan failure that *did* write a report. A bad
    // command line produced no report at all, so it belongs with the other exit-1
    // cases. `--help` and `--version` still exit 0 through clap's own path.
    let cli = match cc_uax_cli::args::Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            let usage_error = !matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp
                    | clap::error::ErrorKind::DisplayVersion
                    | clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
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
