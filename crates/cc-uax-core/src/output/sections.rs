//! Output section selection: the `-S`/`--sections` content selector shared by the
//! CLI and `Package::to_json`.

use anyhow::{Result, bail};

#[derive(Debug, Clone)]
pub struct OutputSections {
    pub(crate) summary: bool,
    pub(crate) imports: bool,
    pub(crate) names: bool,
    pub(crate) references: bool,
    pub(crate) exports: bool,
    pub(crate) pins: bool,
    pub(crate) properties: bool,
    pub(crate) layout: bool,
}

impl Default for OutputSections {
    fn default() -> Self {
        Self::dump()
    }
}

impl OutputSections {
    pub(crate) fn none() -> Self {
        Self {
            summary: false,
            imports: false,
            names: false,
            references: false,
            exports: false,
            pins: false,
            properties: false,
            layout: false,
        }
    }

    pub fn dump() -> Self {
        Self {
            summary: true,
            imports: true,
            names: false,
            references: false,
            exports: true,
            pins: true,
            properties: true,
            layout: true,
        }
    }

    pub fn all() -> Self {
        let mut sections = Self::dump();
        sections.names = true;
        sections.references = true;
        sections
    }

    pub fn summary(&self) -> bool {
        self.summary
    }

    pub fn imports(&self) -> bool {
        self.imports
    }

    pub fn names(&self) -> bool {
        self.names
    }

    pub fn references(&self) -> bool {
        self.references
    }

    pub fn exports(&self) -> bool {
        self.exports
    }

    pub fn pins(&self) -> bool {
        self.pins
    }

    pub fn properties(&self) -> bool {
        self.properties
    }

    pub fn layout(&self) -> bool {
        self.layout
    }

    pub fn parse(spec: &str) -> Result<Self> {
        let mut s = Self::none();
        let mut seen = false;
        for raw in spec.split(',') {
            let tok = raw.trim();
            if tok.is_empty() {
                continue;
            }
            seen = true;
            match tok.to_ascii_lowercase().as_str() {
                "dump" => s.merge(&Self::dump()),
                "all" => s.merge(&Self::all()),
                "logic" | "graph" => {
                    s.summary = true;
                    s.exports = true;
                    s.pins = true;
                }
                "debug" => {
                    s.summary = true;
                    s.imports = true;
                    s.exports = true;
                    s.properties = true;
                    s.layout = true;
                }
                "summary" => s.summary = true,
                "imports" => s.imports = true,
                "names" => s.names = true,
                "references" | "refs" => s.references = true,
                "exports" | "identity" => s.exports = true,
                "pins" => {
                    s.exports = true;
                    s.pins = true;
                }
                "properties" | "props" => {
                    s.exports = true;
                    s.properties = true;
                }
                "layout" => {
                    s.exports = true;
                    s.layout = true;
                }
                other => bail!(
                    "unknown section '{other}'; valid: summary, imports, exports, pins, properties, layout, names, references; presets: logic, debug, dump, all"
                ),
            }
        }
        if !seen {
            bail!("no sections specified");
        }
        Ok(s)
    }

    fn merge(&mut self, other: &Self) {
        self.summary |= other.summary;
        self.imports |= other.imports;
        self.names |= other.names;
        self.references |= other.references;
        self.exports |= other.exports;
        self.pins |= other.pins;
        self.properties |= other.properties;
        self.layout |= other.layout;
    }
}
