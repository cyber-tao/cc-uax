use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "cc-uax", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    #[arg(long, global = true, help = "Emit compact JSON")]
    pub compact: bool,

    #[arg(short, long, global = true, value_name = "FILE")]
    pub output: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Analyze one versioned, uncooked UE5 package.
    Asset(AssetArgs),
    /// Index and analyze an Unreal project or Content directory.
    Project(ProjectArgs),
}

#[derive(Debug, Args)]
pub struct AssetArgs {
    #[arg(value_name = "FILE")]
    pub file: PathBuf,

    #[arg(long, value_enum, default_value_t = AssetViewArg::Full)]
    pub view: AssetViewArg,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum AssetViewArg {
    Summary,
    Logic,
    Properties,
    References,
    Full,
}

#[derive(Debug, Args)]
pub struct ProjectArgs {
    #[arg(value_name = "PROJECT_OR_CONTENT_DIR")]
    pub root: PathBuf,

    #[arg(long, value_name = "PACKAGE_OR_GLOB")]
    pub focus: Vec<String>,

    #[arg(
        long,
        value_name = "PACKAGE=RELATIVE",
        help = "Add a project-relative mount, for example /Plugin=Plugins/X/Content"
    )]
    pub mount: Vec<String>,

    #[arg(
        long,
        help = "Return a partial report with exit code 0 when mapped assets fail"
    )]
    pub allow_partial: bool,

    #[arg(long, value_name = "FILE", conflicts_with = "no_cache")]
    pub cache_file: Option<PathBuf>,

    #[arg(long, conflicts_with = "cache_file")]
    pub no_cache: bool,
}
