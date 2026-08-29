//! The parse cache.
//!
//! Strictly an optimisation. Deleting `.dankg/cache/` changes nothing but
//! runtime, and every failure in this module is a warning at worst -- a cache
//! that can break a build is worse than no cache. `--no-cache` exists so that
//! claim is testable rather than merely asserted.
//!
//! An entry is keyed on `(mtime, len)` and verified by a content hash. The
//! first pair is a cheap rejection; the hash is what closes the window where a
//! file is rewritten within one filesystem timestamp tick. The config hash is
//! stamped in too, because a config change can change what a file means.
//!
//! What is stored is the *index* contribution of a file -- its nodes,
//! containment edges, raw links and aliases -- rather than the markdown AST.
//! That is exactly what steps 4-6 of the pipeline consume, and it is a far
//! smaller thing to write a codec for. `dankg fmt`, which needs the whole AST,
//! does not use the cache and does not want to: it reads every file it is
//! given anyway.

use super::build::{ParsedFile, RawLink, Target};
use super::model::{Edge, EdgeKind, Node, NodeId};
use crate::config;
use crate::diag::{Diagnostic, Level};
use crate::hash;
use std::fs;
use std::path::{Path, PathBuf};

const MAGIC: &str = "!dankg-cache";
/// Bumped whenever the record format changes. An entry from another version is
/// a miss, not an error.
const VERSION: u32 = 1;
/// Separates the items of a list field. `escape` guarantees it never survives
/// inside one, so splitting on it is exact.
const UNIT: char = '\u{1f}';

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stats {
    pub hits: usize,
    pub misses: usize,
    pub writes: usize,
    /// Unreadable or unwritable entries. Counted, never fatal.
    pub errors: usize,
}

#[derive(Debug, Clone)]
pub struct Cache {
    /// `None` when caching is off, or when the root has no `.dankg/` to put it
    /// in -- DanKG does not create one in a directory the user never marked.
    dir: Option<PathBuf>,
    config_hash: u64,
    pub stats: Stats,
}

impl Cache {
    pub fn open(root: &Path, config_hash: u64, enabled: bool) -> Cache {
        let dir = (enabled && root.join(config::DIR).is_dir())
            .then(|| root.join(config::DIR).join("cache"));
        Cache { dir, config_hash, stats: Stats::default() }
    }

    /// A cache that never hits and never writes.
    pub fn off() -> Cache {
        Cache { dir: None, config_hash: 0, stats: Stats::default() }
    }

    pub fn is_enabled(&self) -> bool {
        self.dir.is_some()
    }

    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    /// Returns the file's index contribution and the diagnostics its parse
    /// raised. Both, always: a cached run has to say what a cold run said.
    pub fn lookup(
        &mut self,
        rel: &str,
        len: u64,
        mtime: u128,
        content: &str,
    ) -> Option<(ParsedFile, Vec<Diagnostic>)> {
        let dir = self.dir.as_ref()?;
        let Ok(text) = fs::read_to_string(entry_path(dir, rel)) else {
            self.stats.misses += 1;
            return None;
        };
        let stamp = Stamp { config: self.config_hash, len, mtime, content: content_hash(content) };
        match decode(&text, &stamp) {
            Some(entry) => {
                self.stats.hits += 1;
                Some(entry)
            }
            None => {
                self.stats.misses += 1;
                None
            }
        }
    }

    pub fn store(
        &mut self,
        rel: &str,
        len: u64,
        mtime: u128,
        content: &str,
        file: &ParsedFile,
        diags: &[Diagnostic],
    ) {
        let Some(dir) = self.dir.clone() else { return };
        if fs::create_dir_all(&dir).is_err() {
            self.stats.errors += 1;
            return;
        }

        let stamp = Stamp { config: self.config_hash, len, mtime, content: content_hash(content) };
        let text = encode(&stamp, file, diags);
        let path = entry_path(&dir, rel);
        // Write then rename: a half-written entry must never be readable as a
        // whole one, and two concurrent runs must not interleave.
        let tmp = path.with_extension(format!("tmp{}", std::process::id()));

        match fs::write(&tmp, text).and_then(|()| fs::rename(&tmp, &path)) {
            Ok(()) => self.stats.writes += 1,
            Err(_) => {
                self.stats.errors += 1;
                let _ = fs::remove_file(&tmp);
            }
        }
    }

    /// Entries on disk that no longer correspond to a file in the corpus.
    /// Reported by `dankg index`; never deleted behind the user's back.
    pub fn orphans(&self, live: &[String]) -> usize {
        let Some(dir) = &self.dir else { return 0 };
        let Ok(entries) = fs::read_dir(dir) else { return 0 };
        let names: Vec<String> = live.iter().map(|rel| entry_name(rel)).collect();
        entries
            .flatten()
            .filter(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                name.ends_with(".idx") && !names.contains(&name)
            })
            .count()
    }
}

fn content_hash(content: &str) -> u64 {
    hash::fnv1a(content.as_bytes())
}

/// Entries are named by a hash of the path, so a nested source file does not
/// need a nested cache directory and no path separator has to be encoded.
fn entry_name(rel: &str) -> String {
    format!("{}.idx", hash::hex(hash::fnv1a(rel.as_bytes())))
}

fn entry_path(dir: &Path, rel: &str) -> PathBuf {
    dir.join(entry_name(rel))
}

/// Everything an entry must agree with to be usable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stamp {
    config: u64,
    len: u64,
    mtime: u128,
    content: u64,
}

fn encode(stamp: &Stamp, file: &ParsedFile, diags: &[Diagnostic]) -> String {
    let mut out = String::new();
    row(&mut out, MAGIC, &[VERSION.to_string()]);
    row(
        &mut out,
        "meta",
        &[
            hash::hex(stamp.config),
            stamp.len.to_string(),
            stamp.mtime.to_string(),
            hash::hex(stamp.content),
        ],
    );
    row(&mut out, "file", &[escape(&file.path), escape(&file.key)]);

    for alias in &file.aliases {
        row(&mut out, "alias", &[escape(alias)]);
    }
    for node in &file.nodes {
        let (parent_file, parent_slug) = match &node.parent {
            Some(p) => (p.file.as_str(), p.slug.as_str()),
            None => ("", ""),
        };
        row(
            &mut out,
            "node",
            &[
                escape(&node.id.file),
                escape(&node.id.slug),
                escape(&node.title),
                escape(&node.file),
                node.line.to_string(),
                node.end_line.to_string(),
                node.level.to_string(),
                escape(parent_file),
                escape(parent_slug),
                list(&node.tags),
                list(&node.external),
                flag(node.resolved),
            ],
        );
    }
    for edge in &file.containment {
        row(
            &mut out,
            "edge",
            &[
                escape(&edge.from.file),
                escape(&edge.from.slug),
                escape(&edge.to.file),
                escape(&edge.to.slug),
                edge.kind.as_str().to_string(),
                edge.line.to_string(),
                flag(edge.reciprocated),
            ],
        );
    }
    for link in &file.links {
        let (kind, target) = match &link.target {
            Target::Path(t) => ("p", t),
            Target::Wiki(t) => ("w", t),
        };
        row(
            &mut out,
            "link",
            &[
                escape(&link.from.file),
                escape(&link.from.slug),
                kind.to_string(),
                escape(target),
                link.line.to_string(),
            ],
        );
    }
    for d in diags {
        row(
            &mut out,
            "diag",
            &[
                d.level.as_str().to_string(),
                escape(&d.file),
                d.line.to_string(),
                escape(&d.message),
            ],
        );
    }
    out
}

/// `None` for anything stale, malformed, or from another version. Every caller
/// treats that as "parse it properly", so there is no failure to report.
fn decode(text: &str, stamp: &Stamp) -> Option<(ParsedFile, Vec<Diagnostic>)> {
    let mut lines = text.lines();

    let (magic, version) = lines.next()?.split_once('\t')?;
    if magic != MAGIC || version.parse::<u32>().ok()? != VERSION {
        return None;
    }

    let meta: Vec<&str> = lines.next()?.split('\t').collect();
    if meta.len() != 5 || meta[0] != "meta" {
        return None;
    }
    // Cheapest first: a changed length or timestamp rejects without hashing
    // anything, and is how almost every real miss is found.
    if meta[2].parse::<u64>().ok()? != stamp.len || meta[3].parse::<u128>().ok()? != stamp.mtime {
        return None;
    }
    if hash::parse_hex(meta[1])? != stamp.config || hash::parse_hex(meta[4])? != stamp.content {
        return None;
    }

    let mut path = None;
    let mut key = String::new();
    let mut aliases = Vec::new();
    let mut nodes = Vec::new();
    let mut containment = Vec::new();
    let mut links = Vec::new();
    let mut reported = Vec::new();

    for line in lines {
        let mut fields = line.split('\t');
        let kind = fields.next()?;
        let f: Vec<&str> = fields.collect();
        match kind {
            "file" if f.len() == 2 => {
                path = Some(unescape(f[0]));
                key = unescape(f[1]);
            }
            "alias" if f.len() == 1 => aliases.push(unescape(f[0])),
            "node" if f.len() == 12 => nodes.push(Node {
                id: NodeId::new(unescape(f[0]), unescape(f[1])),
                title: unescape(f[2]),
                file: unescape(f[3]),
                line: f[4].parse().ok()?,
                end_line: f[5].parse().ok()?,
                level: f[6].parse().ok()?,
                parent: (!f[7].is_empty() || !f[8].is_empty())
                    .then(|| NodeId::new(unescape(f[7]), unescape(f[8]))),
                tags: unlist(f[9]),
                external: unlist(f[10]),
                resolved: unflag(f[11])?,
            }),
            "edge" if f.len() == 7 => containment.push(Edge {
                from: NodeId::new(unescape(f[0]), unescape(f[1])),
                to: NodeId::new(unescape(f[2]), unescape(f[3])),
                kind: match f[4] {
                    "contains" => EdgeKind::Contains,
                    "link" => EdgeKind::Link,
                    _ => return None,
                },
                line: f[5].parse().ok()?,
                reciprocated: unflag(f[6])?,
            }),
            "link" if f.len() == 5 => links.push(RawLink {
                from: NodeId::new(unescape(f[0]), unescape(f[1])),
                target: match f[2] {
                    "p" => Target::Path(unescape(f[3])),
                    "w" => Target::Wiki(unescape(f[3])),
                    _ => return None,
                },
                line: f[4].parse().ok()?,
            }),
            "diag" if f.len() == 4 => reported.push(Diagnostic {
                level: Level::parse(f[0])?,
                file: unescape(f[1]),
                line: f[2].parse().ok()?,
                message: unescape(f[3]),
            }),
            _ => return None,
        }
    }

    Some((ParsedFile { path: path?, key, nodes, containment, links, aliases }, reported))
}

fn row(out: &mut String, kind: &str, fields: &[String]) {
    out.push_str(kind);
    for field in fields {
        out.push('\t');
        out.push_str(field);
    }
    out.push('\n');
}

fn flag(value: bool) -> String {
    if value { "1" } else { "0" }.to_string()
}

fn unflag(field: &str) -> Option<bool> {
    match field {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
}

/// Items are escaped before joining, so the separator cannot occur inside one.
fn list(items: &[String]) -> String {
    items.iter().map(|i| escape(i)).collect::<Vec<_>>().join(&UNIT.to_string())
}

fn unlist(field: &str) -> Vec<String> {
    if field.is_empty() {
        return Vec::new();
    }
    field.split(UNIT).map(unescape).collect()
}

/// The record format is line- and tab-delimited, so both have to go, along
/// with the list separator and the escape character itself.
fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            UNIT => out.push_str("\\u"),
            c => out.push(c),
        }
    }
    out
}

fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('\\') => out.push('\\'),
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('u') => out.push(UNIT),
            // Unknown escapes are kept verbatim rather than dropped: the
            // decoder's job is to round-trip, not to editorialise.
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::Diags;
    use crate::graph::build;
    use crate::md::Document;

    const SOURCE: &str = "---\ntags: [rust, graphs]\nalias: arch\n---\n# One\n\nSee [x](b.md#two) and [[Three]] and [s](https://example.com/a?b=c).\n\n## Nested\ttab\n";

    fn parsed() -> ParsedFile {
        let mut d = Diags::new("a.md");
        let doc = Document::parse(SOURCE, &mut d);
        build::build("notes/a.md", &doc, SOURCE.lines().count() as u32)
    }

    fn reported() -> Vec<Diagnostic> {
        let mut d = Diags::new("notes/a.md");
        d.warn(4, "a warning\twith a tab");
        d.error(9, "and an error");
        d.items().to_vec()
    }

    fn stamp() -> Stamp {
        Stamp { config: 7, len: 42, mtime: 99, content: content_hash(SOURCE) }
    }

    #[test]
    fn a_parsed_file_survives_a_round_trip() {
        let file = parsed();
        let text = encode(&stamp(), &file, &reported());
        let (decoded, diags) = decode(&text, &stamp()).expect("entry is fresh");

        assert_eq!(decoded.path, file.path);
        assert_eq!(decoded.key, file.key);
        assert_eq!(decoded.aliases, file.aliases);
        assert_eq!(decoded.nodes, file.nodes);
        assert_eq!(decoded.containment, file.containment);
        assert_eq!(decoded.links, file.links);
        assert!(!file.nodes.is_empty() && !file.links.is_empty(), "the fixture exercises both");
        assert_eq!(diags, reported(), "a hit must report what the parse reported");
    }

    #[test]
    fn every_part_of_the_stamp_can_invalidate_it() {
        let text = encode(&stamp(), &parsed(), &reported());
        for changed in [
            Stamp { config: 8, ..stamp() },
            Stamp { len: 43, ..stamp() },
            Stamp { mtime: 100, ..stamp() },
            Stamp { content: content_hash("# Different\n"), ..stamp() },
        ] {
            assert!(decode(&text, &changed).is_none(), "{changed:?} should be a miss");
        }
    }

    #[test]
    fn another_version_is_a_miss_not_a_failure() {
        let text = encode(&stamp(), &parsed(), &[]).replacen("\t1\n", "\t2\n", 1);
        assert!(decode(&text, &stamp()).is_none());
    }

    #[test]
    fn truncated_and_corrupt_entries_decode_to_nothing() {
        let text = encode(&stamp(), &parsed(), &reported());
        assert!(decode("", &stamp()).is_none());
        assert!(decode("garbage", &stamp()).is_none());
        assert!(decode(&text[..text.len() / 2], &stamp()).is_none());
        assert!(decode(&text.replace("node", "nodes"), &stamp()).is_none());
    }

    #[test]
    fn tabs_newlines_and_the_list_separator_survive_escaping() {
        for value in ["a\tb", "a\nb", "a\\b", "a\u{1f}b", "", "plain", "\\t not a tab"] {
            assert_eq!(unescape(&escape(value)), value, "{value:?}");
        }
    }

    #[test]
    fn a_list_item_containing_the_separator_is_not_split() {
        let items = vec!["a\u{1f}b".to_string(), "c".to_string()];
        assert_eq!(unlist(&list(&items)), items);
        assert!(unlist(&list(&[])).is_empty());
    }

    #[test]
    fn entry_names_are_fixed_width_and_path_free() {
        let name = entry_name("notes/deep/a.md");
        assert_eq!(name.len(), 20);
        assert!(!name.contains('/'));
        assert_ne!(name, entry_name("notes/deep/b.md"));
    }

    #[test]
    fn a_cache_that_is_off_never_hits_or_writes() {
        let mut cache = Cache::off();
        assert!(!cache.is_enabled());
        assert!(cache.lookup("a.md", 1, 1, "x").is_none());
        cache.store("a.md", 1, 1, "x", &parsed(), &[]);
        assert_eq!(cache.stats, Stats::default(), "a disabled cache records nothing");
    }
}
