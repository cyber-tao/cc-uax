use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "cc-uax",
    version,
    about = "Parse UE5 Blueprint (.uasset) files into JSON",
    arg_required_else_help = true,
    after_help = r#"EXAMPLES:
  cc-uax asset.uasset                     Full JSON (summary + imports + exports)
  cc-uax asset.uasset -S logic            Graph nodes + pin connectivity only
  cc-uax asset.uasset -S debug            Summary + imports + full properties + layout
  cc-uax asset.uasset -S exports,pins     Pick sections explicitly
  cc-uax asset.uasset -c -o out.json      Write compact JSON to a file"#
)]
pub struct Args {
    #[arg(
        value_name = "INPUT",
        help = "Path to the UE5 Blueprint (.uasset) file to parse"
    )]
    pub input: PathBuf,

    #[arg(
        short,
        long,
        value_name = "FILE",
        help = "Write JSON to <FILE> instead of stdout"
    )]
    pub output: Option<PathBuf>,

    #[arg(short, long, help = "Output compact JSON instead of pretty-printed")]
    pub compact: bool,

    #[arg(
        short = 'S',
        long,
        value_name = "LIST",
        help = "Output sections to emit (comma-separated), or a preset. Sections: summary, imports, exports (alias: identity), pins, properties (props), layout, names, references (refs). Presets: logic (graph), debug, full (all; default)"
    )]
    pub sections: Option<String>,

    #[arg(
        short = 'd',
        long,
        value_name = "DIR",
        help = "Scan <DIR> recursively to also list assets that reference this file (with -S refs)"
    )]
    pub scan_dir: Option<PathBuf>,

    #[arg(
        short = 'm',
        long,
        value_name = "PREFIX",
        default_value = "/Game",
        help = "Mount prefix mapping <DIR> to package paths (default: /Game)"
    )]
    pub mount: String,

    #[arg(
        long,
        help = "Disable the on-disk reverse-reference cache (<DIR>/.cc-uax-cache.sqlite)"
    )]
    pub no_cache: bool,
}
