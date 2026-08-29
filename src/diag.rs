//! Diagnostics.
//!
//! Every construct DanKG drops, skips, or cannot resolve is reported here with
//! a source location. stdout is reserved for requested output, so everything in
//! this module goes to stderr.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Warn,
    Error,
}

impl Level {
    pub fn as_str(&self) -> &'static str {
        match self {
            Level::Warn => "warn",
            Level::Error => "error",
        }
    }

    /// The inverse of [`Level::as_str`]. The cache stores diagnostics so that a
    /// cached run reports what a cold one did, and has to read them back.
    pub fn parse(text: &str) -> Option<Level> {
        match text {
            "warn" => Some(Level::Warn),
            "error" => Some(Level::Error),
            _ => None,
        }
    }
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub level: Level,
    pub file: String,
    pub line: u32,
    pub message: String,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}:{} {}", self.level, self.file, self.line, self.message)
    }
}

/// Collector for one file's worth of diagnostics.
///
/// Passed by `&mut` through the parsers rather than living in a global, so that
/// parsing stays a pure function of its input and tests can assert on exactly
/// what a given document produced.
#[derive(Debug, Clone, Default)]
pub struct Diags {
    file: String,
    items: Vec<Diagnostic>,
}

impl Diags {
    pub fn new(file: impl Into<String>) -> Self {
        Diags { file: file.into(), items: Vec::new() }
    }

    pub fn warn(&mut self, line: u32, message: impl Into<String>) {
        self.push(Level::Warn, line, message);
    }

    /// Warn about a file other than this collector's own. Resolution spans the
    /// whole corpus, so a diagnostic raised while resolving one file routinely
    /// concerns another.
    pub fn warn_in(&mut self, file: impl Into<String>, line: u32, message: impl Into<String>) {
        self.items.push(Diagnostic {
            level: Level::Warn,
            file: file.into(),
            line,
            message: message.into(),
        });
    }

    pub fn error(&mut self, line: u32, message: impl Into<String>) {
        self.push(Level::Error, line, message);
    }

    fn push(&mut self, level: Level, line: u32, message: impl Into<String>) {
        self.items.push(Diagnostic {
            level,
            file: self.file.clone(),
            line,
            message: message.into(),
        });
    }

    /// Add a diagnostic that was made elsewhere, keeping its own location.
    pub fn add(&mut self, item: Diagnostic) {
        self.items.push(item);
    }

    pub fn items(&self) -> &[Diagnostic] {
        &self.items
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn count(&self, level: Level) -> usize {
        self.items.iter().filter(|d| d.level == level).count()
    }

    /// Merge another file's diagnostics in, preserving their own file names.
    pub fn absorb(&mut self, other: Diags) {
        self.items.extend(other.items);
    }

    /// Order by location, so that the same corpus reports the same things in
    /// the same order however the walk or the resolver happened to reach them.
    /// Stable, so several diagnostics on one line keep the order they were
    /// raised in.
    pub fn sort(&mut self) {
        self.items.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
    }

    /// Write every diagnostic to stderr.
    pub fn emit(&self) {
        for d in &self.items {
            eprintln!("{d}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_with_location() {
        let mut d = Diags::new("notes.md");
        d.warn(41, "unresolved link");
        assert_eq!(d.items()[0].to_string(), "warn: notes.md:41 unresolved link");
    }

    #[test]
    fn counts_by_level() {
        let mut d = Diags::new("a.md");
        d.warn(1, "x");
        d.warn(2, "y");
        d.error(3, "z");
        assert_eq!(d.count(Level::Warn), 2);
        assert_eq!(d.count(Level::Error), 1);
    }

    #[test]
    fn levels_round_trip_through_text() {
        for level in [Level::Warn, Level::Error] {
            assert_eq!(Level::parse(level.as_str()), Some(level));
        }
        assert_eq!(Level::parse("shout"), None);
    }

    #[test]
    fn sort_orders_by_location_and_is_stable() {
        let mut d = Diags::new("b.md");
        d.warn(2, "second");
        d.warn(1, "first");
        d.warn(1, "also first");
        d.warn_in("a.md", 9, "other file");
        d.sort();
        let seen: Vec<(&str, u32, &str)> =
            d.items().iter().map(|i| (i.file.as_str(), i.line, i.message.as_str())).collect();
        assert_eq!(
            seen,
            vec![
                ("a.md", 9, "other file"),
                ("b.md", 1, "first"),
                ("b.md", 1, "also first"),
                ("b.md", 2, "second"),
            ]
        );
    }

    #[test]
    fn absorb_keeps_original_file_names() {
        let mut a = Diags::new("a.md");
        a.warn(1, "from a");
        let mut b = Diags::new("b.md");
        b.warn(2, "from b");
        a.absorb(b);
        assert_eq!(a.items()[1].file, "b.md");
    }
}
