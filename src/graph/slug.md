# Graph slug

This is the anchor half of every link in this codebase. `[text](file.md#slug)`
only jumps to the right place if the slug DanKG assigns a heading
matches what GitHub (and every other renderer that already implements
this same scheme) would assign it. `slugify` therefore follows that
convention exactly, rather than inventing DanKG's own. `char::to_lowercase`
and `char::is_alphanumeric` are both Unicode-aware in `std`. That keeps
this lookup-table-free, in keeping with
[decision 1](../../architecture.md#decision-1-dependency-policy).

```rust name=module_doc path=graph/slug.rs
//! Heading slugs.
//!
//! GitHub-compatible so that `[text](file.md#slug)` written for DanKG still
//! jumps to the right place when the file is viewed anywhere else.
//!
//! Unicode works without lookup tables. `char::to_lowercase` and
//! `char::is_alphanumeric` are both Unicode-aware in std.

use std::collections::HashMap;
```

```rust name=slugify path=graph/slug.rs
/// Slugify one heading title. Not collision-aware on its own. Use
/// [`Slugger`] when processing a whole document.
pub fn slugify(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut pending_dash = false;

    for c in title.chars() {
        if c.is_whitespace() {
            // Whitespace runs collapse to a single dash. Leading runs are
            // dropped entirely.
            pending_dash = !out.is_empty();
            continue;
        }
        if c.is_alphanumeric() || c == '-' || c == '_' {
            if pending_dash {
                out.push('-');
                pending_dash = false;
            }
            out.extend(c.to_lowercase());
        }
        // Everything else is punctuation and is dropped without becoming a dash,
        // matching GitHub: "Don't Panic!" -> "dont-panic".
    }

    out
}

/// Whether a declared slug is one every reader of it can carry: one
/// `slugify` leaves alone. That is what lets an artifact slug, a block
/// name and a heading slug share a single fragment namespace
/// (decisions 64 and 70). Typst itself allows `.` and `:` too, and
/// both are left out because either one changes what a CSS selector
/// means.
///
/// Stated as a fixed point rather than as a charset, because the
/// charset alone was not the whole rule. `slugify` also lowercases,
/// and decision 70 routes a declared slug through `Slugger` to
/// reserve it. `artifact=Chart` passed a charset check, came back
/// from `Slugger` as `chart`, and left `[[#Chart]]` looking up a
/// fragment nothing carried -- a fragment is compared verbatim. A
/// silently repaired slug is exactly what decision 63 refuses, so the
/// repair is refused here instead: the check rejects anything
/// `slugify` would rewrite, and the author gets a diagnostic naming
/// the one they wrote.
///
/// Lived in `weave.rs` as `usable_label` through 0.9.0. It moved here
/// when decision 69 gave the graph a second caller for it. Two copies
/// of one rule is two chances for `dankg weave` and `dankg graph` to
/// disagree about which slug a document declares.
pub fn usable_slug(slug: &str) -> bool {
    !slug.is_empty() && slugify(slug) == slug
}
```

A raw slug is only unique within the heading that produced it. A whole
document can repeat a title, so `Slugger` is the stateful half that keeps
every slug it hands out distinct, suffixing repeats in the order they
appear.

```rust name=slugger path=graph/slug.rs
/// Assigns unique slugs within one document, suffixing repeats in the order
/// they appear: `overview`, `overview-1`, `overview-2`.
#[derive(Debug, Default)]
pub struct Slugger {
    seen: HashMap<String, u32>,
}

impl Slugger {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether `slug` is already spoken for in this document. Decision
    /// 70 refuses to suffix a *declared* artifact slug, so its caller
    /// has to ask before assigning rather than react afterwards --
    /// `assign` alone cannot say no. Takes the slug verbatim, because
    /// the only caller already rejected anything `slugify` would
    /// rewrite (`usable_slug`, below).
    pub fn taken(&self, slug: &str) -> bool {
        self.seen.contains_key(slug)
    }

    /// Reserves `slug` verbatim, suffixing nothing. `assign` is the
    /// wrong tool for a slug that is already final: it slugifies its
    /// argument first, and a caller that has to compare the result
    /// against what it passed in is asking this question instead.
    pub fn reserve(&mut self, slug: &str) {
        self.seen.insert(slug.to_string(), 0);
    }

    pub fn assign(&mut self, title: &str) -> String {
        let base = slugify(title);
        // A heading of only punctuation slugifies to nothing. It still needs an
        // addressable identity.
        let base = if base.is_empty() { "section".to_string() } else { base };

        match self.seen.get_mut(&base) {
            None => {
                self.seen.insert(base.clone(), 0);
                base
            }
            Some(count) => {
                *count += 1;
                let mut candidate = format!("{base}-{count}");
                // A document containing both "Overview" twice and a literal
                // "Overview 1" must still end up with distinct slugs.
                while self.seen.contains_key(&candidate) {
                    *self.seen.get_mut(&base).expect("base was just found") += 1;
                    let n = self.seen[&base];
                    candidate = format!("{base}-{n}");
                }
                self.seen.insert(candidate.clone(), 0);
                candidate
            }
        }
    }
}
```

## Tests

```rust name=tests path=graph/slug.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowercases_and_hyphenates() {
        assert_eq!(slugify("Key Features"), "key-features");
    }

    #[test]
    fn drops_punctuation_without_leaving_dashes() {
        assert_eq!(slugify("Don't Panic!"), "dont-panic");
        assert_eq!(slugify("Graphs: A Study"), "graphs-a-study");
    }

    #[test]
    fn collapses_whitespace_runs() {
        assert_eq!(slugify("a   b"), "a-b");
        assert_eq!(slugify("  leading"), "leading");
        assert_eq!(slugify("trailing  "), "trailing");
    }

    #[test]
    fn keeps_hyphens_and_underscores() {
        assert_eq!(slugify("snake_case and kebab-case"), "snake_case-and-kebab-case");
    }

    #[test]
    fn handles_unicode_without_tables() {
        assert_eq!(slugify("Größe"), "größe");
        assert_eq!(slugify("Ünicode Headings"), "ünicode-headings");
    }

    #[test]
    fn collisions_get_ordinal_suffixes() {
        let mut s = Slugger::new();
        assert_eq!(s.assign("Overview"), "overview");
        assert_eq!(s.assign("Overview"), "overview-1");
        assert_eq!(s.assign("Overview"), "overview-2");
    }

    #[test]
    fn suffix_skips_a_slug_already_taken_literally() {
        let mut s = Slugger::new();
        assert_eq!(s.assign("Overview"), "overview");
        assert_eq!(s.assign("Overview 1"), "overview-1");
        // The natural suffix is taken, so the next one is used.
        assert_eq!(s.assign("Overview"), "overview-2");
    }

    #[test]
    fn punctuation_only_heading_still_gets_an_identity() {
        let mut s = Slugger::new();
        assert_eq!(s.assign("???"), "section");
        assert_eq!(s.assign("!!!"), "section-1");
    }
}
```
