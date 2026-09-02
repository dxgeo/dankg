# Graph ignore

`.dankgignore` is a deliberately small subset of gitignore. One pattern
per line. `#` comments. `!` un-ignores. A leading `/` anchors to the root.
A trailing `/` matches directories only. `*` and `?` stay within one path
segment. `**` crosses them. Later rules win. Small on purpose. The whole
point of a knowledge-base root is that its contents are predictable. A
reader cannot judge that by eye if the exclusion language itself grows
edge cases. Dot-files and dot-directories need no pattern at all. The
corpus walk skips them structurally. That is what keeps `.dankg/` and
`.git/` out without this file ever mentioning either.

```rust name=module_doc path=graph/ignore.rs
//! `.dankgignore`: which files under the root are not part of the corpus.
//!
//! A deliberately small subset of gitignore, because the whole point of a root
//! is that its contents are predictable. One pattern per line. `#` comments.
//! `!` un-ignores. A leading `/` anchors to the root. A trailing `/` matches
//! directories only. `*` and `?` stay within one path segment. `**` crosses
//! them. Later rules win, so an exception can follow the rule it excepts.
//!
//! Dot-files and dot-directories are skipped by the walk itself and need no
//! pattern. That is what keeps `.dankg/` and `.git/` out of the corpus.

use crate::diag::Diags;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

pub const FILE: &str = ".dankgignore";

#[derive(Debug, Clone, PartialEq, Eq)]
struct Rule {
    pattern: Vec<char>,
    /// Contains a `/`, so it is matched against the whole root-relative path
    /// rather than against any single segment.
    anchored: bool,
    dir_only: bool,
    negated: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ignore {
    rules: Vec<Rule>,
    /// Root-relative path it was read from; `None` when there is no file.
    pub source: Option<String>,
}
```

A missing file is not an error. Most corpora will never have one.
"Nothing ignored" is exactly the right behavior for that case, not a
diagnostic. An unreadable file (permissions, not absence) does warn,
since that is a surprise the reader should hear about.

```rust name=load_and_parse path=graph/ignore.rs
impl Ignore {
    pub fn none() -> Ignore {
        Ignore::default()
    }

    pub fn load(root: &Path, diags: &mut Diags) -> Ignore {
        match fs::read_to_string(root.join(FILE)) {
            Ok(text) => {
                let mut file_diags = Diags::new(FILE);
                let mut ignore = Ignore::parse(&text, &mut file_diags);
                diags.absorb(file_diags);
                ignore.source = Some(FILE.to_string());
                ignore
            }
            Err(e) if e.kind() == ErrorKind::NotFound => Ignore::none(),
            Err(e) => {
                diags.warn_in(FILE, 0, format!("could not be read ({e}); nothing ignored"));
                Ignore::none()
            }
        }
    }

    pub fn parse(text: &str, diags: &mut Diags) -> Ignore {
        let mut rules = Vec::new();

        for (i, raw) in text.lines().enumerate() {
            let line = i as u32 + 1;
            let trimmed = raw.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            let (negated, body) = match trimmed.strip_prefix('!') {
                Some(rest) => (true, rest.trim()),
                None => (false, trimmed),
            };
            let (dir_only, body) = match body.strip_suffix('/') {
                Some(rest) => (true, rest),
                None => (false, body),
            };
            let body = body.strip_prefix('/').unwrap_or(body);

            if body.is_empty() {
                diags.warn(line, format!("`{trimmed}` has no pattern; skipped"));
                continue;
            }
            // `/` inside the pattern is what makes it a path rather than a
            // name, which is the same rule gitignore uses.
            let anchored = trimmed.starts_with('/') || body.contains('/');
            rules.push(Rule { pattern: body.chars().collect(), anchored, dir_only, negated });
        }

        Ignore { rules, source: None }
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Whether `rel` -- a root-relative, `/`-separated path -- is excluded.
    pub fn matches(&self, rel: &str, is_dir: bool) -> bool {
        let path: Vec<char> = rel.chars().collect();
        let mut excluded = false;

        for rule in &self.rules {
            if rule.dir_only && !is_dir {
                continue;
            }
            if rule_matches(rule, rel, &path) {
                excluded = !rule.negated;
            }
        }
        excluded
    }
}

fn rule_matches(rule: &Rule, rel: &str, path: &[char]) -> bool {
    if matches(&rule.pattern, path) {
        return true;
    }
    if rule.anchored {
        return false;
    }
    // An unanchored name applies at every depth, so try each segment tail.
    rel.match_indices('/').any(|(i, _)| {
        let tail: Vec<char> = rel[i + 1..].chars().collect();
        matches(&rule.pattern, &tail)
    })
}
```

```rust name=matches path=graph/ignore.rs
/// Glob matching. `*` and `?` stop at a separator. `**` does not.
fn matches(pattern: &[char], text: &[char]) -> bool {
    if pattern.is_empty() {
        return text.is_empty();
    }
    match pattern[0] {
        '*' if pattern.get(1) == Some(&'*') => {
            let rest = &pattern[2..];
            // `**/x` has to match a bare `x` too. Otherwise it would only ever
            // find things at least one directory deep.
            if rest.first() == Some(&'/') && matches(&rest[1..], text) {
                return true;
            }
            (0..=text.len()).any(|i| matches(rest, &text[i..]))
        }
        '*' => {
            let rest = &pattern[1..];
            for i in 0..=text.len() {
                if matches(rest, &text[i..]) {
                    return true;
                }
                if text.get(i) == Some(&'/') {
                    break;
                }
            }
            false
        }
        '?' => !text.is_empty() && text[0] != '/' && matches(&pattern[1..], &text[1..]),
        c => !text.is_empty() && text[0] == c && matches(&pattern[1..], &text[1..]),
    }
}
```

## Tests

```rust name=tests path=graph/ignore.rs
#[cfg(test)]
mod tests {
    use super::*;

    fn ignore(text: &str) -> Ignore {
        let mut d = Diags::new(FILE);
        let i = Ignore::parse(text, &mut d);
        assert!(d.is_empty(), "{:?}", d.items());
        i
    }

    #[test]
    fn a_bare_name_matches_at_every_depth() {
        let i = ignore("drafts/\n");
        assert!(i.matches("drafts", true));
        assert!(i.matches("notes/drafts", true));
        assert!(!i.matches("drafts", false), "a trailing slash means directories only");
    }

    #[test]
    fn a_leading_slash_anchors_to_the_root() {
        let i = ignore("/notes/scratch.md\n");
        assert!(i.matches("notes/scratch.md", false));
        assert!(!i.matches("deep/notes/scratch.md", false));
    }

    #[test]
    fn star_stays_inside_one_segment() {
        let i = ignore("*.tmp.md\n");
        assert!(i.matches("a.tmp.md", false));
        assert!(i.matches("notes/a.tmp.md", false));

        let i = ignore("notes/*.md\n");
        assert!(i.matches("notes/a.md", false));
        assert!(!i.matches("notes/deep/a.md", false), "`*` does not cross a separator");
    }

    #[test]
    fn double_star_crosses_segments_including_none() {
        let i = ignore("notes/**/old.md\n");
        assert!(i.matches("notes/old.md", false));
        assert!(i.matches("notes/a/b/old.md", false));
        assert!(!i.matches("other/old.md", false));
    }

    #[test]
    fn question_mark_matches_exactly_one_character() {
        let i = ignore("draft?.md\n");
        assert!(i.matches("draft1.md", false));
        assert!(!i.matches("draft.md", false));
        assert!(!i.matches("draft12.md", false));
    }

    #[test]
    fn a_later_negation_wins() {
        let i = ignore("drafts/\n!drafts/keep/\n");
        assert!(i.matches("drafts", true));
        assert!(!i.matches("drafts/keep", true));
    }

    #[test]
    fn comments_and_blank_lines_are_skipped() {
        let i = ignore("# scratch space\n\n  \nscratch/\n");
        assert_eq!(i.rules.len(), 1);
    }

    #[test]
    fn an_empty_pattern_warns_rather_than_matching_everything() {
        let mut d = Diags::new(FILE);
        let i = Ignore::parse("!\n/\n", &mut d);
        assert!(i.is_empty());
        assert_eq!(d.items().len(), 2);
    }

    #[test]
    fn nothing_is_ignored_without_a_file() {
        assert!(!Ignore::none().matches("anything.md", false));
    }
}
```
