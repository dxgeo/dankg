//! `.dankg/config`: a minimal INI.
//!
//! No serde, so the format is what a hand-written parser can read without
//! guessing: sections, `key = value`, `#` comments, no nesting and no arrays.
//! Anything it does not understand warns with a line number and is skipped,
//! on the same principle as frontmatter -- a config DanKG half-understood
//! would be worse than one it refused.
//!
//! A `[lang.*]` section is also the allowlist: a fenced block in a language
//! with no configured command is reported and never executed.

use crate::diag::Diags;
use crate::hash;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

/// The directory whose presence marks a root.
pub const DIR: &str = ".dankg";
pub const FILE: &str = "config";
/// Entry plus this many hops, when a view is selected. See decision 7.
pub const DEFAULT_DEPTH: u32 = 2;

/// Sections DanKG reads today, with the keys each one accepts.
const KNOWN: &[(&str, &[&str])] = &[("graph", &["depth"]), ("editor", &["command"])];

/// Section families, named `<prefix><name>`. `db.` is reserved for milestone 8
/// and parsed now so that a config written ahead of it does not warn.
const FAMILIES: &[(&str, &[&str])] =
    &[("lang.", &["command", "ext"]), ("db.", &["command", "path"])];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub name: String,
    pub entries: Vec<(String, String)>,
}

impl Section {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }
}

/// One configured interpreter. Its presence is what permits execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lang {
    pub name: String,
    pub command: String,
    pub ext: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub sections: Vec<Section>,
    /// Root-relative path it was read from; `None` when there is no config.
    pub source: Option<String>,
    hash: u64,
}

impl Config {
    /// The config of a root with no config file. Distinct from an empty file:
    /// adding an empty `.dankg/config` changes the hash, and so invalidates
    /// the cache, which is the honest answer.
    pub fn none() -> Config {
        Config { sections: Vec::new(), source: None, hash: 0 }
    }

    /// Read `<root>/.dankg/config`, if there is one.
    pub fn load(root: &Path, diags: &mut Diags) -> Config {
        let rel = format!("{DIR}/{FILE}");
        match fs::read_to_string(root.join(DIR).join(FILE)) {
            Ok(text) => {
                let mut file_diags = Diags::new(&rel);
                let mut config = Config::parse(&text, &mut file_diags);
                diags.absorb(file_diags);
                config.source = Some(rel);
                config
            }
            Err(e) if e.kind() == ErrorKind::NotFound => Config::none(),
            Err(e) => {
                diags.warn_in(rel, 0, format!("could not be read ({e}); defaults used"));
                Config::none()
            }
        }
    }

    pub fn parse(text: &str, diags: &mut Diags) -> Config {
        let mut sections: Vec<Section> = Vec::new();
        let mut current: Option<usize> = None;

        for (i, raw) in text.lines().enumerate() {
            let line = i as u32 + 1;
            let trimmed = raw.trim();

            // A comment marker only counts at the start of a line: `command`
            // values legitimately contain `#`, and a value is not a place to
            // start guessing.
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
                continue;
            }

            if trimmed.starts_with('[') {
                current = section_header(trimmed, line, &mut sections, diags);
                continue;
            }

            let Some((key, value)) = trimmed.split_once('=') else {
                diags.warn(line, format!("expected `key = value`, found `{trimmed}`; skipped"));
                continue;
            };
            let key = key.trim().to_string();
            let value = unquote(value.trim()).to_string();

            let Some(index) = current else {
                diags.warn(line, format!("`{key}` sits outside any section; ignored"));
                continue;
            };
            let name = sections[index].name.clone();

            if let Some(allowed) = allowed_keys(&name) {
                if !allowed.iter().any(|k| *k == key) {
                    diags.warn(line, format!("unknown key `{key}` in `[{name}]`; ignored"));
                    continue;
                }
            }
            if sections[index].get(&key).is_some() {
                diags.warn(line, format!("`{key}` is set twice in `[{name}]`; the last wins"));
                sections[index].entries.retain(|(k, _)| *k != key);
            }
            sections[index].entries.push((key, value));
        }

        Config { sections, source: None, hash: hash::fnv1a(text.as_bytes()) }
    }

    /// Stamped into every cache entry, because a config change can change what
    /// a file parses to. See [`crate::graph::cache`].
    pub fn hash(&self) -> u64 {
        self.hash
    }

    pub fn section(&self, name: &str) -> Option<&Section> {
        self.sections.iter().find(|s| s.name == name)
    }

    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.section(section)?.get(key)
    }

    /// `[graph] depth`, or the default. A value that is not a number warns
    /// rather than silently becoming zero.
    pub fn depth(&self, diags: &mut Diags) -> u32 {
        let Some(raw) = self.get("graph", "depth") else {
            return DEFAULT_DEPTH;
        };
        match raw.parse::<u32>() {
            Ok(depth) => depth,
            Err(_) => {
                let where_ = self.source.clone().unwrap_or_else(|| DIR.to_string());
                diags.warn_in(
                    where_,
                    0,
                    format!("`depth = {raw}` is not a number; using {DEFAULT_DEPTH}"),
                );
                DEFAULT_DEPTH
            }
        }
    }

    /// Every configured interpreter, in config order.
    pub fn langs(&self) -> Vec<Lang> {
        self.sections
            .iter()
            .filter_map(|s| {
                let name = s.name.strip_prefix("lang.")?;
                Some(Lang {
                    name: name.to_string(),
                    command: s.get("command")?.to_string(),
                    ext: s.get("ext").map(str::to_string),
                })
            })
            .collect()
    }

    pub fn lang(&self, name: &str) -> Option<Lang> {
        self.langs().into_iter().find(|l| l.name == name)
    }

    /// `[editor] command`: the template used to jump to a node's source line,
    /// with `{file}` and `{line}` substituted. `None` when unconfigured --
    /// falling back to `$EDITOR`/`$VISUAL` is the caller's decision, not
    /// this file's, since that is environment rather than config.
    pub fn editor(&self) -> Option<&str> {
        self.get("editor", "command")
    }
}

fn section_header(
    trimmed: &str,
    line: u32,
    sections: &mut Vec<Section>,
    diags: &mut Diags,
) -> Option<usize> {
    let Some(name) = trimmed.strip_prefix('[').and_then(|r| r.strip_suffix(']')) else {
        diags.warn(line, format!("unterminated section header `{trimmed}`; skipped"));
        return None;
    };
    let name = name.trim();
    if name.is_empty() {
        diags.warn(line, "section header has no name; skipped");
        return None;
    }
    if allowed_keys(name).is_none() {
        diags.warn(line, format!("unknown section `[{name}]`; its keys are ignored"));
    }

    // A repeated header continues the same section, so writing `[graph]` twice
    // is untidy rather than destructive.
    Some(match sections.iter().position(|s| s.name == name) {
        Some(i) => i,
        None => {
            sections.push(Section { name: name.to_string(), entries: Vec::new() });
            sections.len() - 1
        }
    })
}

/// The keys a section accepts, or `None` when the section itself is unknown.
fn allowed_keys(name: &str) -> Option<&'static [&'static str]> {
    if let Some((_, keys)) = KNOWN.iter().find(|(n, _)| *n == name) {
        return Some(keys);
    }
    FAMILIES
        .iter()
        .find(|(prefix, _)| name.starts_with(prefix) && name.len() > prefix.len())
        .map(|(_, keys)| *keys)
}

/// Quotes are stripped when they wrap the whole value, so a command with
/// trailing spaces can be written down. They are not otherwise meaningful.
fn unquote(value: &str) -> &str {
    let bytes = value.as_bytes();
    if bytes.len() >= 2 && (bytes[0] == b'"' || bytes[0] == b'\'') && bytes[0] == bytes[bytes.len() - 1]
    {
        &value[1..value.len() - 1]
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> (Config, Diags) {
        let mut d = Diags::new(".dankg/config");
        let c = Config::parse(text, &mut d);
        (c, d)
    }

    #[test]
    fn reads_sections_keys_and_comments() {
        let (c, d) = parse(
            "# a knowledge base\n\n[graph]\ndepth = 3\n\n[lang.python]\ncommand = uv run python {file}\next     = py\n",
        );
        assert!(d.is_empty(), "{:?}", d.items());
        assert_eq!(c.get("graph", "depth"), Some("3"));
        assert_eq!(
            c.langs(),
            vec![Lang {
                name: "python".into(),
                command: "uv run python {file}".into(),
                ext: Some("py".into()),
            }]
        );
    }

    #[test]
    fn depth_falls_back_and_warns_on_nonsense() {
        let (c, mut d) = parse("[graph]\ndepth = lots\n");
        assert_eq!(c.depth(&mut d), DEFAULT_DEPTH);
        assert!(d.items().iter().any(|i| i.message.contains("is not a number")));

        let (c, mut d) = parse("");
        assert_eq!(c.depth(&mut d), DEFAULT_DEPTH);
        assert!(d.is_empty());
    }

    #[test]
    fn a_hash_inside_a_value_is_not_a_comment() {
        let (c, d) = parse("[lang.sh]\ncommand = sh -c 'echo #1'\n");
        assert_eq!(c.get("lang.sh", "command"), Some("sh -c 'echo #1'"));
        assert!(d.is_empty());
    }

    #[test]
    fn unknown_sections_and_keys_warn_and_are_dropped() {
        let (c, d) = parse("[grph]\ndepth = 2\n\n[graph]\ndeth = 2\n");
        assert_eq!(c.get("graph", "deth"), None);
        let messages: Vec<&str> = d.items().iter().map(|i| i.message.as_str()).collect();
        assert!(messages.iter().any(|m| m.contains("unknown section `[grph]`")));
        assert!(messages.iter().any(|m| m.contains("unknown key `deth`")));
    }

    #[test]
    fn editor_command_reads_back() {
        let (c, d) = parse("[editor]\ncommand = code -g {file}:{line}\n");
        assert!(d.is_empty(), "{:?}", d.items());
        assert_eq!(c.editor(), Some("code -g {file}:{line}"));

        let (c, _) = parse("");
        assert_eq!(c.editor(), None);
    }

    #[test]
    fn db_sections_parse_ahead_of_milestone_eight() {
        let (c, d) = parse("[db.warehouse]\ncommand = duckdb -csv {db} < {file}\npath = data/w.duckdb\n");
        assert!(d.is_empty(), "{:?}", d.items());
        assert_eq!(c.get("db.warehouse", "path"), Some("data/w.duckdb"));
    }

    #[test]
    fn malformed_lines_warn_with_their_line_number() {
        let (_, d) = parse("[graph\ndepth 2\n");
        assert_eq!(d.items()[0].line, 1);
        assert!(d.items()[0].message.contains("unterminated"));
        // A line with no `=` is reported as such, before anything asks which
        // section it would have joined.
        assert!(d.items()[1].message.contains("expected `key = value`"));

        let (_, d) = parse("depth = 2\n");
        assert!(d.items()[0].message.contains("outside any section"));
    }

    #[test]
    fn quotes_wrapping_a_value_are_stripped() {
        let (c, _) = parse("[lang.sh]\ncommand = \"sh {file} \"\n");
        assert_eq!(c.get("lang.sh", "command"), Some("sh {file} "));
    }

    #[test]
    fn repeated_headers_merge_and_repeated_keys_take_the_last() {
        let (c, d) = parse("[graph]\ndepth = 1\n[graph]\ndepth = 4\n");
        assert_eq!(c.sections.len(), 1);
        assert_eq!(c.get("graph", "depth"), Some("4"));
        assert!(d.items()[0].message.contains("set twice"));
    }

    #[test]
    fn absence_and_emptiness_hash_differently() {
        let (empty, _) = parse("");
        assert_ne!(Config::none().hash(), empty.hash());
    }
}
