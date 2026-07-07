use anyhow::{Context, Result, bail};
use cc_uax::{OutputSections, Package};
use clap::Parser;
use cli::args::Args;
use cli::reverse_refs::compute_referenced_by;
use serde_json::{Value, json};
use std::fs;
use std::io::Write;

mod cli;

fn main() -> Result<()> {
    let args = Args::parse();

    let data = fs::read(&args.input)
        .with_context(|| format!("Failed to read file: {}", args.input.display()))?;

    let package = Package::parse(&data)
        .with_context(|| format!("Failed to parse: {}", args.input.display()))?;

    let sections = match args.sections.as_deref() {
        Some(spec) => OutputSections::parse(spec)
            .with_context(|| format!("Invalid --sections value: '{spec}'"))?,
        None => OutputSections::dump(),
    };

    let mut json = package.to_json(&data, &sections);

    if sections.references {
        if let Some(scan_dir) = args.scan_dir.as_deref() {
            let (self_pkg, referenced_by) =
                compute_referenced_by(&args.input, scan_dir, &args.mount, !args.no_cache)?;
            if let Value::Object(ref mut m) = json
                && let Some(Value::Object(refs)) = m.get_mut("references")
            {
                refs.insert("self".into(), json!(self_pkg));
                refs.insert("referenced_by".into(), json!(referenced_by));
            }
        }
    } else if args.scan_dir.is_some() {
        bail!(
            "--scan-dir requires the references section; add -S refs (for example: -S dump,refs)"
        );
    }

    if let Value::Object(ref mut m) = json {
        m.insert("file".into(), json!(args.input.display().to_string()));
    }

    let text = if args.compact {
        serde_json::to_string(&json)?
    } else {
        serde_json::to_string_pretty(&json)?
    };

    match args.output {
        Some(path) => {
            fs::write(&path, &text)
                .with_context(|| format!("Failed to write file: {}", path.display()))?;
            eprintln!("Written to {}", path.display());
        }
        None => {
            let mut out = std::io::stdout().lock();
            out.write_all(text.as_bytes())?;
            out.write_all(b"\n")?;
        }
    }

    Ok(())
}
