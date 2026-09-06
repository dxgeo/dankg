# Config

`.dankg/config` is a minimal INI. It is hand-parsed rather than pulled
from a crate, for the same reason frontmatter is. The format is exactly
what a hand-written parser can read without guessing: sections,
`key = value`, `#` comments, no nesting, no arrays. Anything outside that
warns with a line number and is skipped. This follows the same
"half-understood is worse than refused" principle frontmatter already
holds. A `[lang.*]` section is also the allowlist. A fenced block in a
language with no configured command is reported and never evaluated,
never guessed at from its fence tag alone.

```rust name=module_doc path=config.rs
//! `.dankg/config`: a minimal INI.
//!
//! There is no serde. The format is what a hand-written parser can read
//! without guessing: sections, `key = value`, `#` comments, no nesting and
//! no arrays. Anything it does not understand warns with a line number and
//! is skipped. This follows the same principle as frontmatter: a config
//! DanKG half-understood would be worse than one it refused.
//!
//! A `[lang.*]` section is also the allowlist. A fenced block in a language
//! with no configured command is reported and never evaluated.

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
/// `dankg tui`'s own default, deliberately smaller than `DEFAULT_DEPTH`.
/// A terminal tree has no zoom the way HTML's output gets (*Terminal
/// UI*), so the same width a scrollable, zoomable page affords already
/// reads as sprawling stamped into one, especially over a large corpus.
/// Right-to-expand and `/`-to-jump are the reader's own way to widen it
/// back out, one node at a time.
pub const DEFAULT_TUI_DEPTH: u32 = 1;

/// Sections DanKG reads today, with the keys each one accepts.
const KNOWN: &[(&str, &[&str])] = &[
    ("graph", &["depth"]),
    ("tui", &["depth", "breadcrumb"]),
    ("editor", &["command"]),
    ("keys", &["up", "down", "left", "right", "quit", "reset", "eval", "breadcrumb"]),
];

/// Section families, named `<prefix><name>`. `db.`'s `list` (decision 37)
/// is the one command both `dankg graph --live` and *Provenance without a
/// driver*'s own before/after diff spawn -- one protocol-agnostic
/// primitive, two callers, neither assuming anything about the engine
/// beyond "one relation identifier per line."
/// `tangle.`'s `command` is optional (decision 25): a language with no
/// separate build step just materializes its tree and stops. `glue` is
/// independent of `command` and just as optional (decision 26): an external
/// program, never DanKG's own code, that adds language-specific structural
/// glue (Rust's `mod` declarations, say) to the assembled tree before
/// `command` builds it.
const FAMILIES: &[(&str, &[&str])] = &[
    ("lang.", &["command", "ext"]),
    ("db.", &["command", "path", "list"]),
    ("tangle.", &["command", "ext", "glue"]),
];

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
```

`Keymap` deliberately covers only the single-character actions the
TUI's own interaction table names by letter. Arrow keys are physical
direction keys, not mnemonics, so there is nothing meaningful to remap
about them. Enter, Tab, `/`, and `?` are fixed the same way, for their
own separate reasons (terminal special keys; close to universal across
terminal tools) laid out in architecture.md's own *Interaction* section.

```rust name=keymap path=config.rs
/// Single-character key bindings for the TUI, read from `[keys]`. Arrow
/// keys are not represented here. They are physical direction keys rather
/// than mnemonics, so nothing about them is meaningful to remap. They
/// always work alongside whatever a letter is bound to. Enter, Tab, `/`,
/// and `?` are the same story for their own reasons: `[keys]` only ever
/// touches the single-character actions the interaction table names by
/// letter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Keymap {
    pub up: char,
    pub down: char,
    pub left: char,
    pub right: char,
    pub quit: char,
    pub reset: char,
    /// Cycles through the selected node's named blocks (entering cycle mode
    /// on the first press), so `enter` can run whichever one is cycled to.
    pub eval: char,
    /// Toggles the origin breadcrumb (`[tui] breadcrumb`'s own runtime
    /// counterpart) on the status line while the panel has focus.
    pub breadcrumb: char,
}

impl Default for Keymap {
    fn default() -> Keymap {
        Keymap { up: 'k', down: 'j', left: 'h', right: 'l', quit: 'q', reset: 'r', eval: 'e', breadcrumb: 'b' }
    }
}

impl Keymap {
    fn fields(&self) -> [(&'static str, char); 8] {
        [
            ("up", self.up),
            ("down", self.down),
            ("left", self.left),
            ("right", self.right),
            ("quit", self.quit),
            ("reset", self.reset),
            ("eval", self.eval),
            ("breadcrumb", self.breadcrumb),
        ]
    }
}
```

`Tangle.command` is optional. `Lang`'s is not, in the same spirit. Take
a language with no separate build step, like tangling a Python module. It
has nothing left to spawn once the tree is materialized. That
materialization already *is* the whole operation. `glue` is a second,
equally optional, fully independent command. Where `command` builds the
assembled tree, `glue` adds structural connective tissue to it first.
Writing that tissue is squarely the kind of thing someone other than
DanKG might want to do, for a language DanKG never shipped one for.

```rust name=lang_and_tangle path=config.rs
/// One configured interpreter. Its presence is what permits execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lang {
    pub name: String,
    pub command: String,
    pub ext: Option<String>,
}

/// One `[db.*]` section (decision 16). `path` substitutes `{db}` in
/// `command`, the same way a block's own temp file substitutes `{file}`.
/// `list` (decision 37) is the protocol-agnostic catalog listing `--live`
/// spawns and *Provenance without a driver*'s own before/after diff runs;
/// parsed here so a config written ahead of either still parses clean, the
/// same "parsed ahead of milestone" precedent this family has already
/// held since before any of it was implemented.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Db {
    pub name: String,
    pub command: String,
    pub path: Option<String>,
    pub list: Option<String>,
}

/// One `[tangle.*]` section (decision 25). `command` is optional. Take a
/// language with no separate build step, like tangling a Python module.
/// It has nothing to spawn. Materializing the tree already is the whole
/// operation. `glue` is a second, independent, equally optional command
/// (decision 26). Where `command` builds the assembled tree, `glue` adds
/// to it first. It is an external, per-language extension point for
/// structural connective tissue (module declarations). That only makes
/// sense as a separate, swappable step: it is squarely the kind of thing
/// someone other than DanKG might want to write, for a language DanKG
/// never shipped one for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tangle {
    pub name: String,
    pub command: Option<String>,
    pub glue: Option<String>,
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
}
```

A comment marker only counts at the very start of a line. A `command`
value legitimately contains `#` (a shell redirect, a URL fragment).
`parse` never starts guessing partway through a value about what might be
a comment.

```rust name=parse path=config.rs
impl Config {
    pub fn parse(text: &str, diags: &mut Diags) -> Config {
        let mut sections: Vec<Section> = Vec::new();
        let mut current: Option<usize> = None;

        for (i, raw) in text.lines().enumerate() {
            let line = i as u32 + 1;
            let trimmed = raw.trim();

            // A comment marker only counts at the start of a line.
            // `command` values legitimately contain `#`. A value is not a
            // place to start guessing.
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
}
```

```rust name=lookups path=config.rs
impl Config {
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

    /// `[tui] depth`, or `DEFAULT_TUI_DEPTH` -- a deliberately smaller,
    /// separate default from `[graph] depth`'s own. A reader who wants
    /// the TUI to match `dankg graph`'s default sets this key; `--depth`/
    /// `--all` on the command line still override either default the
    /// same way.
    pub fn tui_depth(&self, diags: &mut Diags) -> u32 {
        let Some(raw) = self.get("tui", "depth") else {
            return DEFAULT_TUI_DEPTH;
        };
        match raw.parse::<u32>() {
            Ok(depth) => depth,
            Err(_) => {
                let where_ = self.source.clone().unwrap_or_else(|| DIR.to_string());
                diags.warn_in(
                    where_,
                    0,
                    format!("`depth = {raw}` is not a number; using {DEFAULT_TUI_DEPTH}"),
                );
                DEFAULT_TUI_DEPTH
            }
        }
    }

    /// `[tui] breadcrumb`, or `true`. The origin breadcrumb (*Terminal
    /// UI*'s own *Origin breadcrumb*) is on by default. A reader who never
    /// wants it can turn it off for good here, rather than pressing
    /// `keys.breadcrumb` every session. `keys.breadcrumb` still toggles it
    /// for the running session either way, regardless of this default.
    pub fn tui_breadcrumb(&self, diags: &mut Diags) -> bool {
        let Some(raw) = self.get("tui", "breadcrumb") else {
            return true;
        };
        match raw {
            "true" => true,
            "false" => false,
            _ => {
                let where_ = self.source.clone().unwrap_or_else(|| DIR.to_string());
                diags.warn_in(where_, 0, format!("`breadcrumb = {raw}` in `[tui]` is not `true` or `false`; using true"));
                true
            }
        }
    }
}
```

Unlike `[lang.*]`, a missing `[tangle.*]` section does not refuse
anything by itself. `tangle` still needs *a* section to know `--lang`'s
fence tag is real. A section with no `command` at all is a complete,
valid configuration on its own (decision 25). Materializing the tree is
the whole operation for a language with no separate build step.

```rust name=langs_and_tangle_lookup path=config.rs
impl Config {
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

    /// Every configured `[db.*]` section, in config order (decision 16).
    pub fn dbs(&self) -> Vec<Db> {
        self.sections
            .iter()
            .filter_map(|s| {
                let name = s.name.strip_prefix("db.")?;
                Some(Db {
                    name: name.to_string(),
                    command: s.get("command")?.to_string(),
                    path: s.get("path").map(str::to_string),
                    list: s.get("list").map(str::to_string),
                })
            })
            .collect()
    }

    pub fn db(&self, name: &str) -> Option<Db> {
        self.dbs().into_iter().find(|d| d.name == name)
    }

    /// `[tangle.<name>]`, if configured. Unlike `[lang.*]`, its absence
    /// does not refuse anything by itself. `tangle` still needs a section
    /// to know `--lang`'s fence tag is real. A section with no `command`
    /// is a complete, valid configuration on its own (decision 25).
    pub fn tangle(&self, name: &str) -> Option<Tangle> {
        self.sections.iter().find_map(|s| {
            let n = s.name.strip_prefix("tangle.")?;
            (n == name).then(|| Tangle {
                name: n.to_string(),
                command: s.get("command").map(str::to_string),
                glue: s.get("glue").map(str::to_string),
                ext: s.get("ext").map(str::to_string),
            })
        })
    }

    /// `[editor] command`: the template used to jump to a node's source line,
    /// with `{file}` and `{line}` substituted. `None` when unconfigured.
    /// Falling back to `$EDITOR`/`$VISUAL` is the caller's decision, not
    /// this file's, since that is environment rather than config.
    pub fn editor(&self) -> Option<&str> {
        self.get("editor", "command")
    }
}
```

`keymap` falls back to the *whole* default map on a collision, not just
the one binding involved. Applying an ambiguous map silently would mean
one of the two colliding keys simply stops working, with nothing in the
UI to say which. That is worse than reverting everything and telling the
reader why.

```rust name=keymap_method path=config.rs
impl Config {
    /// `[keys]`, or the defaults. A value that is not exactly one character
    /// falls back to its default and warns. So does the whole map at once
    /// if two actions end up bound to the same character. Applying an
    /// ambiguous binding silently would mean one of the two keys stops
    /// working with no indication which.
    pub fn keymap(&self, diags: &mut Diags) -> Keymap {
        let default = Keymap::default();
        let Some(section) = self.section("keys") else { return default };
        let where_ = || self.source.clone().unwrap_or_else(|| DIR.to_string());

        let mut map = default;
        let mut slots: [(&str, &mut char); 8] = [
            ("up", &mut map.up),
            ("down", &mut map.down),
            ("left", &mut map.left),
            ("right", &mut map.right),
            ("quit", &mut map.quit),
            ("reset", &mut map.reset),
            ("eval", &mut map.eval),
            ("breadcrumb", &mut map.breadcrumb),
        ];
        for (name, slot) in &mut slots {
            let Some(raw) = section.get(name) else { continue };
            let mut chars = raw.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => **slot = c,
                _ => diags.warn_in(
                    where_(),
                    0,
                    format!("`{name} = {raw}` in `[keys]` is not a single character; kept as `{}`", **slot),
                ),
            }
        }
        drop(slots);

        let fields = map.fields();
        for i in 0..fields.len() {
            for j in (i + 1)..fields.len() {
                if fields[i].1 == fields[j].1 {
                    diags.warn_in(
                        where_(),
                        0,
                        format!(
                            "`[keys]` binds both `{}` and `{}` to `{}`; using the defaults instead",
                            fields[i].0, fields[j].0, fields[i].1
                        ),
                    );
                    return default;
                }
            }
        }
        map
    }
}
```

```rust name=parse_helpers path=config.rs
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

    // A repeated header continues the same section. This way, writing
    // `[graph]` twice is untidy rather than destructive.
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

/// Quotes are stripped when they wrap the whole value. This way, a
/// command with trailing spaces can be written down. They are not
/// otherwise meaningful.
fn unquote(value: &str) -> &str {
    let bytes = value.as_bytes();
    if bytes.len() >= 2 && (bytes[0] == b'"' || bytes[0] == b'\'') && bytes[0] == bytes[bytes.len() - 1]
    {
        &value[1..value.len() - 1]
    } else {
        value
    }
}
```

## Tests

```rust name=tests path=config.rs
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
    fn tui_depth_is_its_own_smaller_default_independent_of_graph_depth() {
        let (c, mut d) = parse("");
        assert_eq!(c.tui_depth(&mut d), DEFAULT_TUI_DEPTH);
        assert!(DEFAULT_TUI_DEPTH < DEFAULT_DEPTH, "tui's default should read narrower, not equal or wider");

        // Configuring [graph] depth alone must not move [tui]'s.
        let (c, mut d) = parse("[graph]\ndepth = 5\n");
        assert_eq!(c.tui_depth(&mut d), DEFAULT_TUI_DEPTH);
        assert_eq!(c.depth(&mut d), 5);
    }

    #[test]
    fn tui_depth_reads_its_own_section_and_falls_back_and_warns_on_nonsense() {
        let (c, mut d) = parse("[tui]\ndepth = 3\n");
        assert_eq!(c.tui_depth(&mut d), 3);
        assert!(d.is_empty(), "{:?}", d.items());

        let (c, mut d) = parse("[tui]\ndepth = lots\n");
        assert_eq!(c.tui_depth(&mut d), DEFAULT_TUI_DEPTH);
        assert!(d.items().iter().any(|i| i.message.contains("is not a number")));
    }

    #[test]
    fn tui_breadcrumb_defaults_to_true_and_reads_its_own_section() {
        let (c, mut d) = parse("");
        assert!(c.tui_breadcrumb(&mut d));

        let (c, mut d) = parse("[tui]\nbreadcrumb = false\n");
        assert!(!c.tui_breadcrumb(&mut d));
        assert!(d.is_empty(), "{:?}", d.items());
    }

    #[test]
    fn tui_breadcrumb_falls_back_and_warns_on_nonsense() {
        let (c, mut d) = parse("[tui]\nbreadcrumb = sometimes\n");
        assert!(c.tui_breadcrumb(&mut d));
        assert!(d.items().iter().any(|i| i.message.contains("is not `true` or `false`")));
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
    fn db_returns_the_named_section() {
        let (c, d) = parse("[db.warehouse]\ncommand = duckdb -csv {db} -f {file}\npath = data/w.duckdb\n");
        assert!(d.is_empty(), "{:?}", d.items());
        assert_eq!(
            c.db("warehouse"),
            Some(Db {
                name: "warehouse".into(),
                command: "duckdb -csv {db} -f {file}".into(),
                path: Some("data/w.duckdb".into()),
                list: None,
            })
        );
        assert_eq!(c.db("nope"), None);
    }

    #[test]
    fn db_list_key_parses_clean() {
        let (c, d) = parse(
            "[db.warehouse]\ncommand = duckdb -csv {db} -f {file}\nlist = duckdb -csv {db} -c \"select 1\"\n",
        );
        assert!(d.is_empty(), "{:?}", d.items());
        assert_eq!(c.db("warehouse").unwrap().list, Some("duckdb -csv {db} -c \"select 1\"".into()));
    }

    #[test]
    fn dbs_lists_every_section_in_config_order() {
        let (c, d) = parse("[db.a]\ncommand = x\n\n[db.b]\ncommand = y\n");
        assert!(d.is_empty(), "{:?}", d.items());
        assert_eq!(c.dbs().iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), vec!["a", "b"]);
    }

    #[test]
    fn tangle_sections_parse_with_an_optional_command() {
        let (c, d) = parse("[tangle.rust]\ncommand = cargo build --manifest-path {dir}/Cargo.toml\next = rs\n\n[tangle.python]\next = py\n");
        assert!(d.is_empty(), "{:?}", d.items());
        assert_eq!(
            c.tangle("rust"),
            Some(Tangle {
                name: "rust".into(),
                command: Some("cargo build --manifest-path {dir}/Cargo.toml".into()),
                glue: None,
                ext: Some("rs".into()),
            })
        );
        assert_eq!(
            c.tangle("python"),
            Some(Tangle { name: "python".into(), command: None, glue: None, ext: Some("py".into()) })
        );
        assert_eq!(c.tangle("ghost"), None);
    }

    #[test]
    fn tangle_glue_is_independent_of_command() {
        let (c, d) = parse("[tangle.rust]\nglue = dankg-glue-rust {dir}\next = rs\n");
        assert!(d.is_empty(), "{:?}", d.items());
        assert_eq!(
            c.tangle("rust"),
            Some(Tangle {
                name: "rust".into(),
                command: None,
                glue: Some("dankg-glue-rust {dir}".into()),
                ext: Some("rs".into()),
            })
        );
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
    fn keymap_defaults_with_no_keys_section() {
        let (c, mut d) = parse("");
        assert_eq!(c.keymap(&mut d), Keymap::default());
        assert!(d.is_empty());
    }

    #[test]
    fn keymap_reads_remapped_letters() {
        let (c, mut d) = parse("[keys]\nup = w\ndown = s\nleft = a\nright = d\n");
        assert!(d.is_empty(), "{:?}", d.items());
        let km = c.keymap(&mut d);
        assert_eq!(km.up, 'w');
        assert_eq!(km.down, 's');
        assert_eq!(km.left, 'a');
        assert_eq!(km.right, 'd');
        assert_eq!(km.quit, 'q', "unmentioned actions keep their default");
    }

    #[test]
    fn keymap_remaps_eval_too() {
        let (c, mut d) = parse("[keys]\neval = x\n");
        assert!(d.is_empty(), "{:?}", d.items());
        assert_eq!(c.keymap(&mut d).eval, 'x');
    }

    #[test]
    fn keymap_remaps_breadcrumb_too() {
        let (c, mut d) = parse("[keys]\nbreadcrumb = x\n");
        assert!(d.is_empty(), "{:?}", d.items());
        assert_eq!(c.keymap(&mut d).breadcrumb, 'x');
    }

    #[test]
    fn keymap_rejects_a_multi_character_binding() {
        let (c, mut d) = parse("[keys]\nup = wa\n");
        assert_eq!(c.keymap(&mut d).up, 'k', "falls back to the default");
        assert!(d.items().iter().any(|i| i.message.contains("not a single character")));
    }

    #[test]
    fn keymap_falls_back_entirely_on_a_collision() {
        let (c, mut d) = parse("[keys]\nup = j\n"); // now collides with the default `down`
        assert_eq!(c.keymap(&mut d), Keymap::default(), "an ambiguous map reverts wholesale");
        assert!(d.items().iter().any(|i| i.message.contains("binds both")));
    }

    #[test]
    fn absence_and_emptiness_hash_differently() {
        let (empty, _) = parse("");
        assert_ne!(Config::none().hash(), empty.hash());
    }
}
```
