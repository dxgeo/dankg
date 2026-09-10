# Init

`dankg init [<path>]` scaffolds a brand-new corpus: a `.dankg/`
directory with a commented starter `config`, a `.dankgignore`, and an
`index.md`. It turns the README's own *Quick start* -- several
separate steps by hand -- into one command. `path` defaults to `.`,
the same "the common case is right here" reasoning `index` already
uses. It need not exist yet. `init` creates it.

**Refusal, scoped to exactly `path`, not upward.**
`graph::index::discover_root` walks *upward* from wherever a command
points, looking for a `.dankg/` directory. This codebase already
nests one corpus inside another that way: `example/example_1` has
its own `.dankg/config`, declaring its own root, well inside this
repository's own. Nothing sits between them -- `example/` itself
carries no `.dankg/` of its own. Nesting like that is not a special
case; it falls straight out of `discover_root`'s own upward walk.
`init` only ever checks `path` itself. If `path/.dankg/` already
exists *there*, it refuses outright rather than silently rewriting an
existing corpus's config. A `.dankg/` above `path`, declaring some
*other* corpus, is not this command's concern at all -- running
`init` inside one on purpose is exactly how a sub-corpus like
`example_1` gets made.

**Never overwrites content it did not just create.** `path` is not
required to be empty. A reader retrofitting an existing folder of
real notes into a corpus is exactly as valid a use as an empty
directory. `index.md` and `.dankgignore` are each written only when
absent; an existing one is left completely alone, reported as such
rather than silently skipped. `.dankg/config` never has this question
to answer: by the time `init` writes it, the refusal check above has
already established that `path/.dankg/` did not exist a moment ago.

**Deliberately unopinionated about what goes in `.dankg/config`.**
`Config::load` already treats a missing file as `Config::none()`, no
warning (`config.md`). An *empty* `.dankg/config` would do exactly
the same job as no file at all. The one thing that actually changes
behavior is the `.dankg/` directory's own existence -- the root
marker `discover_root` looks for -- not anything inside the file. So
the generated file carries a short explanation of that, plus a couple
of commented-out example sections (`[lang.sh]`, `[tui] commands`) for
a reader to uncomment. Real content is never guessed: `init` has no
way to know what language a fresh corpus's blocks will even be
written in.

```rust name=module_doc path=init.rs
//! `dankg init`: scaffolds a brand-new corpus -- `.dankg/config`,
//! `.dankgignore`, and `index.md` -- in a directory that is not
//! already one, creating the directory itself if it does not exist
//! yet. Refuses outright rather than overwriting an existing corpus;
//! never touches a file that is already there.

use crate::config;
use crate::graph::ignore;
use std::fs;
use std::path::{Path, PathBuf};
```

## Templates

None of these guess at the reader's own content. `CONFIG_TEMPLATE`
explains the mechanism and offers commented examples rather than real
sections. `IGNORE_TEMPLATE` explains the pattern language
`graph::ignore` already implements, rather than guessing what a
fresh corpus might want excluded. `INDEX_TEMPLATE` reuses the
README's own *Quick start* heading verbatim (`# Index`) rather than
deriving one from the directory name. A reader who already read the
README sees the same word twice, not two different conventions for
the same thing.

```rust name=templates path=init.rs
const CONFIG_TEMPLATE: &str = "\
# This directory is a DanKG corpus root. `dankg graph`/`tui`/`check`
# discover it by walking up for a `.dankg/` directory; everything
# below here belongs to this corpus, unless a subdirectory declares
# its own `.dankg/config`, which starts a nested corpus instead (see
# example/example_1 in dankg's own repository for a worked example).
#
# Uncomment and edit the sections below as needed -- see the README
# for the full list of sections `dankg` reads here.

# [lang.sh]
# command = sh {file}

# [tui]
# commands = commands.md
";

const IGNORE_TEMPLATE: &str = "\
# One pattern per line, a small gitignore subset: `#` comments, `!`
# un-ignores, a leading `/` anchors to this root, a trailing `/`
# matches directories only, `*`/`?` stay within one path segment,
# `**` crosses them. Dot-files and dot-directories (`.git/`, this
# `.dankg/` itself) are already skipped by the corpus walk and need
# no pattern here.
";

const INDEX_TEMPLATE: &str = "# Index\n";
```

## Writing it

`run` never writes a file it cannot immediately account for in
`Report` -- every path the caller might report on is either freshly
written this call or was already there before it, never something in
between.

```rust name=report_and_run path=init.rs
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub dankg_dir: PathBuf,
    pub config: PathBuf,
    pub ignore: PathBuf,
    pub index: PathBuf,
    /// `false` when `.dankgignore` already existed and was left
    /// untouched.
    pub wrote_ignore: bool,
    /// `false` when `index.md` already existed and was left
    /// untouched.
    pub wrote_index: bool,
}

/// Scaffolds a corpus at `path`, creating `path` itself first if it
/// does not exist yet. Refuses -- writing nothing at all -- when
/// `path/.dankg/` is already there; see this module's own header
/// comment for why that check never looks any further than `path`.
pub fn run(path: &str) -> Result<Report, String> {
    let root = Path::new(path);
    fs::create_dir_all(root).map_err(|e| format!("could not create `{path}`: {e}"))?;

    let dankg_dir = root.join(config::DIR);
    if dankg_dir.is_dir() {
        return Err(format!("`{path}` is already a dankg corpus root (`{}` exists)", dankg_dir.display()));
    }
    fs::create_dir_all(&dankg_dir).map_err(|e| format!("could not create `{}`: {e}", dankg_dir.display()))?;

    let config_path = dankg_dir.join(config::FILE);
    fs::write(&config_path, CONFIG_TEMPLATE).map_err(|e| format!("could not write `{}`: {e}", config_path.display()))?;

    let ignore_path = root.join(ignore::FILE);
    let wrote_ignore = write_if_absent(&ignore_path, IGNORE_TEMPLATE)?;

    let index_path = root.join("index.md");
    let wrote_index = write_if_absent(&index_path, INDEX_TEMPLATE)?;

    Ok(Report { dankg_dir, config: config_path, ignore: ignore_path, index: index_path, wrote_ignore, wrote_index })
}

/// `false`, writing nothing, when `path` already exists -- the one
/// shared rule both `.dankgignore` and `index.md` follow, unlike
/// `.dankg/config`, which `run` above already knows cannot exist yet.
fn write_if_absent(path: &Path, content: &str) -> Result<bool, String> {
    if path.exists() {
        return Ok(false);
    }
    fs::write(path, content).map_err(|e| format!("could not write `{}`: {e}", path.display()))?;
    Ok(true)
}
```

## Tests

```rust name=tests path=init.rs
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn scratch_dir() -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("dankg-init-test-{}-{n}", std::process::id()))
    }

    #[test]
    fn init_creates_a_directory_that_does_not_exist_yet() {
        let dir = scratch_dir();
        assert!(!dir.exists());
        let report = run(dir.to_str().unwrap()).unwrap();
        assert!(dir.is_dir());
        assert!(report.dankg_dir.is_dir());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn init_writes_config_ignore_and_index() {
        let dir = scratch_dir();
        let report = run(dir.to_str().unwrap()).unwrap();

        assert!(report.config.is_file());
        assert!(fs::read_to_string(&report.config).unwrap().contains(".dankg/"));
        assert!(report.wrote_ignore);
        assert!(report.ignore.is_file());
        assert!(report.wrote_index);
        assert_eq!(fs::read_to_string(&report.index).unwrap(), "# Index\n");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn init_refuses_when_dankg_already_exists_at_the_exact_path() {
        let dir = scratch_dir();
        fs::create_dir_all(dir.join(".dankg")).unwrap();
        let err = run(dir.to_str().unwrap()).unwrap_err();
        assert!(err.contains("already a dankg corpus root"), "{err:?}");
        assert!(!dir.join(".dankg/config").exists(), "nothing written on refusal");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn init_inside_an_existing_corpus_makes_a_nested_one_rather_than_refusing() {
        // The outer .dankg/ sits at `dir`, not at `dir/inner` -- the
        // exact-path check never walks upward to find it, so this
        // succeeds and creates a second, nested corpus root, the same
        // way `example/example_1` nests inside this repository's own.
        let dir = scratch_dir();
        fs::create_dir_all(dir.join(".dankg")).unwrap();
        let inner = dir.join("inner");
        let report = run(inner.to_str().unwrap()).unwrap();
        assert!(report.dankg_dir.is_dir());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn init_leaves_an_existing_index_and_ignore_untouched() {
        let dir = scratch_dir();
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("index.md"), "# Kept\n").unwrap();
        fs::write(dir.join(".dankgignore"), "/scratch/\n").unwrap();

        let report = run(dir.to_str().unwrap()).unwrap();

        assert!(!report.wrote_index);
        assert!(!report.wrote_ignore);
        assert_eq!(fs::read_to_string(&report.index).unwrap(), "# Kept\n");
        assert_eq!(fs::read_to_string(&report.ignore).unwrap(), "/scratch/\n");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn init_leaves_other_existing_content_alone() {
        let dir = scratch_dir();
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("notes.md"), "# Notes\n\nReal content.\n").unwrap();

        run(dir.to_str().unwrap()).unwrap();

        assert_eq!(fs::read_to_string(dir.join("notes.md")).unwrap(), "# Notes\n\nReal content.\n");
        let _ = fs::remove_dir_all(&dir);
    }
}
```
