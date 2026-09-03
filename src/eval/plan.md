# Eval plan

The dependency DAG: which named top-level code blocks a target needs, in
the order `eval::run` must concatenate them. It is deliberately scoped to
*top-level* blocks only, meaning `doc.blocks` directly. It never recurses
into a list the way `Document::named_blocks` does for the graph.
`eval::result`'s write-back needs a block's position in that same flat
list, to find (or place) its result marker right after it. Keeping
eval's whole view of "what blocks exist" to that one flat list means
there is only one notion of "the code block named `index`" to keep in
step, rather than two. A named block nested in a list is invisible to
eval entirely: it is not planned, and not a valid `deps=` target. This is
a documented gap, not a silently different rule for write-back versus
everything else.

A block's name is unique across the *whole file*. `deps=` resolves
against the whole file too, flat rather than scoped to a heading.
Decision 22 tried heading-scoped uniqueness with lexical, ancestor-only
`deps=` resolution. It was reverted once real use showed it broke the
single most natural literate-pipeline shape: a sequence of sibling
sections each depending on the last. A lexical walk that only ever looks
*upward* cannot reach that shape. Every block, in any heading, must be
reachable from any other. Flat resolution is what makes that possible.
Heading-scoped identity was never load-bearing for anything but tangle's
own placement. Tangle groups by heading directly, without needing eval's
notion of a name to agree.

```rust name=module_doc path=eval/plan.rs
//! The dependency DAG: which named top-level code blocks a target needs,
//! in the order `run.rs` must concatenate them.
//!
//! Deliberately scoped to *top-level* blocks only, meaning `doc.blocks`
//! directly. It never recurses into a list the way `Document::named_blocks`
//! does for the graph. `result.rs`'s write-back needs a block's position
//! in that same flat list, to find (or place) its result marker right
//! after it. Keeping eval's whole view of "what blocks exist" to that
//! flat list means there is only one notion of "the code block named
//! `index`" to keep in step, rather than two. A named block nested in a
//! list is invisible to eval entirely: it is not planned, and not a valid
//! `deps=` target. This is a documented gap, not a silently different
//! rule for write-back versus everything else.
//!
//! A block's name is unique across the whole file. `deps=` resolves
//! against the whole file too, flat rather than scoped to a heading.
//! Decision 22 tried heading-scoped uniqueness with lexical, ancestor-only
//! `deps=` resolution. It was reverted (see architecture.md) once real
//! use showed it broke the single most natural literate-pipeline shape: a
//! sequence of sibling sections each depending on the last. A lexical
//! walk that only ever looks *upward* cannot reach that shape. Every
//! block, in any heading, must be reachable from any other. Flat
//! resolution is what makes that possible. Heading-scoped identity was
//! never load-bearing for anything but tangle's placement
//! (`containing_heading`/`root_heading`, below). Tangle groups by heading
//! directly, without needing eval's notion of a name to agree.

use crate::md::{Block, Document, InfoString, Inline};
use std::collections::{HashMap, HashSet};
use std::fmt;

/// One heading as `Document::headings` reports it: level, title inlines,
/// and the line its `#` marker opens on. Only the level and line matter
/// to `tangle`'s placement. Title text is read separately, from the same
/// tuple, only where a display string is actually needed.
pub type Heading<'a> = (u8, &'a [Inline], u32);

/// One block that eval can evaluate. It holds enough of `Block::Code`
/// and its `InfoString` to plan, evaluate, and locate for write-back. It
/// is borrowed straight from the parsed `Document` rather than copied.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockRef<'a> {
    /// Root-relative path of the file this block was parsed from. Needed
    /// once `deps=` can cross a file boundary: two blocks in different
    /// files may legally share a `name`, so identity is `(file, name)`,
    /// not `name` alone. Every same-file caller passes one literal string
    /// for every block it builds, so nothing about the common case changes.
    pub file: &'a str,
    /// Position in `doc.blocks`. `result::locate_existing` uses this to
    /// look immediately after this block for an existing result marker.
    pub index: usize,
    pub name: &'a str,
    pub lang: Option<&'a str>,
    pub source: &'a str,
    pub line: u32,
    pub end_line: u32,
    pub deps: Vec<&'a str>,
    /// Cross-language dependencies (decision: never concatenated). Resolved
    /// exactly like `deps`, but `visit`/`plan_from` never walk into them:
    /// a target with an `xdeps` entry still runs alone. `eval::result`
    /// folds each resolved target's own last recorded hash into the
    /// target's staleness hash instead of its source.
    pub xdeps: Vec<&'a str>,
    pub timeout: Option<u64>,
    /// `tangle`'s placement override (decision 24). Ignored by eval
    /// entirely. A block's `path=` has nothing to do with evaluating it.
    pub path: Option<&'a str>,
    /// `produces=file:PATH` (decision 33), raw. Read only by
    /// `check_file_deps`, below, when some other block's own `reads=`
    /// names it as a dependency.
    pub produces: Option<&'a str>,
    /// `reads=file:PATH` (decision 33), raw. Checked against a resolved
    /// `deps=`/`xdeps=` target's own `produces=`, never resolved on its
    /// own: see `check_file_deps`.
    pub reads: Option<&'a str>,
}
```

```rust name=plan_error path=eval/plan.rs
#[derive(Debug, Clone, PartialEq)]
pub enum PlanError {
    /// Two top-level blocks share a name -- `deps=` and write-back would
    /// not know which one they meant.
    DuplicateName(String),
    /// The requested `--block` target does not exist among this file's
    /// top-level named blocks.
    UnknownTarget(String),
    /// `block`'s `deps=` names something that is not a top-level named
    /// block in this file.
    UnknownDep { block: String, dep: String },
    /// `deps=` forms a cycle; `path` names it in the order discovered,
    /// ending back where it started.
    Cycle(Vec<String>),
    /// `block` declares a different `lang` than `target`, the block whose
    /// chain it was pulled into. This is refused because the whole chain
    /// is concatenated into one file and run through `target`'s one
    /// interpreter. Code in the wrong language would just fail there.
    MixedLang { target: String, target_lang: String, block: String, block_lang: String },
    /// `block`'s `deps=dep` named a cross-file path that climbs above the
    /// root. This is the same refusal a written link's target already
    /// gets (`graph::resolve::join_normalize`), reused rather than
    /// reimplemented.
    DepEscapesRoot { block: String, dep: String },
    /// `block`'s `xdeps=dep` names something that is not a top-level
    /// named block. The same "not found" as `UnknownDep`, kept as its
    /// own variant so an error naming an `xdeps` entry never reads as
    /// though it named a concatenated dependency instead.
    UnknownXDep { block: String, dep: String },
    /// `block`'s `xdeps=dep` named a cross-file path that climbs above
    /// the root. `DepEscapesRoot`'s counterpart for `xdeps`.
    XDepEscapesRoot { block: String, dep: String },
}

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlanError::DuplicateName(name) => {
                write!(f, "two top-level blocks are both named `{name}`")
            }
            PlanError::UnknownTarget(name) => {
                write!(f, "no top-level block is named `{name}`")
            }
            PlanError::UnknownDep { block, dep } => {
                write!(f, "`{block}` depends on `{dep}`, which is not a top-level named block")
            }
            PlanError::Cycle(path) => {
                write!(f, "dependency cycle: {}", path.join(" -> "))
            }
            PlanError::MixedLang { target, target_lang, block, block_lang } => {
                write!(
                    f,
                    "`{block}` is `{block_lang}` but `{target}`, whose chain it is part of, is `{target_lang}`; \
                     one interpreter runs the whole concatenated file"
                )
            }
            PlanError::DepEscapesRoot { block, dep } => {
                write!(f, "`{block}` depends on `{dep}`, which escapes the root")
            }
            PlanError::UnknownXDep { block, dep } => {
                write!(f, "`{block}` xdeps on `{dep}`, which is not a top-level named block")
            }
            PlanError::XDepEscapesRoot { block, dep } => {
                write!(f, "`{block}` xdeps on `{dep}`, which escapes the root")
            }
        }
    }
}
```

```rust name=top_level_blocks path=eval/plan.rs
/// Every top-level named code block in `doc`, in document order, tagged
/// with `file` (its root-relative path). What `plan_for`/`plan_all`/
/// `plan_each` search and `main` reports "no named blocks" against when it
/// is empty. Every single-file caller already has its own path in hand
/// (`run_single`/`run_one` read it off disk to get here in the first
/// place), so this asks for it rather than defaulting to something that
/// would only be right by accident.
pub fn top_level_blocks<'a>(doc: &'a Document, file: &'a str) -> Vec<BlockRef<'a>> {
    doc.blocks
        .iter()
        .enumerate()
        .filter_map(|(index, b)| {
            let Block::Code { info, text, line, end_line, .. } = b else { return None };
            block_ref(file, index, info, text, *line, *end_line)
        })
        .collect()
}

fn block_ref<'a>(
    file: &'a str,
    index: usize,
    info: &'a InfoString,
    text: &'a str,
    line: u32,
    end_line: u32,
) -> Option<BlockRef<'a>> {
    let name = info.name()?;
    Some(BlockRef {
        file,
        index,
        name,
        lang: info.lang.as_deref(),
        source: text,
        line,
        end_line,
        deps: info.deps(),
        xdeps: info.xdeps(),
        timeout: info.timeout(),
        path: info.path(),
        produces: info.produces(),
        reads: info.reads(),
    })
}
```

`split_dep` mirrors a written link's own `other.md#heading` shape
exactly. `resolve_dep` reuses `graph::resolve::join_normalize` rather
than reimplementing it. `graph::resolve` shares it with this module for
the same reason: a dependency and a link should never disagree about
what a relative path means. A file nobody loaded and a name that does
not exist in a file that *was* loaded are deliberately indistinguishable
from here. Both are just "not found." That is exactly what
`PlanError::UnknownDep`'s own message already says, without needing to
explain why.

```rust name=split_dep_and_resolve path=eval/plan.rs
/// Splits a `deps=` entry into a cross-file path part (if any) and the
/// name, mirroring a written link's own `other.md#heading` shape exactly.
/// `deps=setup` is `(None, "setup")`. `deps=../lib.md#setup` is
/// `(Some("../lib.md"), "setup")`. A `#` with nothing before it (`#name`)
/// is treated as local, the same as a bare link fragment addressing the
/// current file.
pub fn split_dep(raw: &str) -> (Option<&str>, &str) {
    match raw.split_once('#') {
        Some((path, name)) if !path.trim().is_empty() => (Some(path.trim()), name.trim()),
        _ => (None, raw.trim()),
    }
}

enum DepLookup {
    Escapes,
    NotFound,
}

/// Resolves one `deps=` entry declared by a block living in `from_file`
/// to its index in `blocks`. A local reference (no `#`) stays within
/// `from_file`. A cross-file one resolves its path against `from_file`'s
/// own directory the same way a written link's target would
/// (`graph::resolve::join_normalize`, reused rather than reimplemented,
/// so a dependency and a link agree about what a relative path means). A
/// file nobody loaded and a name that does not exist in a file that
/// *was* loaded are indistinguishable from here on purpose. Both are
/// just "not found". That is exactly what `PlanError::UnknownDep`'s
/// existing message already says, without needing to say why.
fn resolve_dep(blocks: &[BlockRef], from_file: &str, raw: &str) -> Result<usize, DepLookup> {
    let (path_part, name) = split_dep(raw);
    let target_file: std::borrow::Cow<str> = match path_part {
        None => std::borrow::Cow::Borrowed(from_file),
        Some(rel) => std::borrow::Cow::Owned(
            crate::graph::resolve::join_normalize(crate::graph::resolve::dir_of(from_file), rel)
                .ok_or(DepLookup::Escapes)?,
        ),
    };
    blocks.iter().position(|b| b.file == target_file.as_ref() && b.name == name).ok_or(DepLookup::NotFound)
}

fn dep_error(block: String, dep: &str, err: DepLookup) -> PlanError {
    match err {
        DepLookup::Escapes => PlanError::DepEscapesRoot { block, dep: dep.to_string() },
        DepLookup::NotFound => PlanError::UnknownDep { block, dep: dep.to_string() },
    }
}

fn xdep_error(block: String, dep: &str, err: DepLookup) -> PlanError {
    match err {
        DepLookup::Escapes => PlanError::XDepEscapesRoot { block, dep: dep.to_string() },
        DepLookup::NotFound => PlanError::UnknownXDep { block, dep: dep.to_string() },
    }
}

/// `name` alone for a block belonging to `home` (today's messages,
/// unchanged for the common single-file case). `file#name` once a block
/// came from somewhere else, so a cycle or an error naming it says which
/// file it actually lives in.
fn label(b: &BlockRef, home: &str) -> String {
    if b.file == home { b.name.to_string() } else { format!("{}#{}", b.file, b.name) }
}
```

`containing_heading`/`root_heading` exist here, not in `graph::build`,
because `tangle` needs exactly this lookup and nothing more: no slug, no
file key, no graph. It uses them to decide which file a block belongs in
(decision 24). `root_heading`'s walk generalizes "a level-1 heading
becomes one file" to "the top of the containment tree becomes one file."
This way, a document that opens with a level-2 heading (no level-1
wrapper at all) still has a well-defined top, rather than one dictated by
a level number no ancestor of it actually has.

```rust name=heading_lookups path=eval/plan.rs
/// The line number of the heading that immediately contains a block
/// sitting at `block_line`: the nearest heading at or above it, in
/// document order. `None` is the file-level scope: before any heading, or
/// a file with none. This is the same approximation `session::list_blocks`
/// already uses to report a block's containing heading. `tangle` reuses
/// it (decision 24) to group blocks by heading, without a second
/// implementation of the same lookup.
pub(crate) fn containing_heading(headings: &[Heading], block_line: u32) -> Option<u32> {
    headings.iter().rfind(|(_, _, line)| *line <= block_line).map(|(_, _, line)| *line)
}

/// The topmost heading in `heading`'s own ancestor chain -- the one with no
/// parent of its own -- or `None` unchanged when `heading` already is the
/// file-level scope. This is `tangle`'s placement unit (decision 24): "a
/// level-1 heading becomes one file" generalizes to "the top of the
/// containment tree becomes one file" so a document that opens with a
/// level-2 heading (no level-1 wrapper) still has a well-defined top rather
/// than one dictated by a level number no ancestor of it actually has.
pub(crate) fn root_heading(headings: &[Heading], heading: Option<u32>) -> Option<u32> {
    let parents = heading_parents(headings);
    let mut current = heading?;
    loop {
        match parents.get(&current).copied().flatten() {
            Some(parent) => current = parent,
            None => return Some(current),
        }
    }
}

/// Every heading's own containing heading, by line number -- the same
/// `(level, id)` stack `graph/build.rs` walks to attach a node to its
/// `current` heading, over bare line numbers instead of `NodeId`s, since
/// `root_heading` needs no slug, no file key, and no graph.
fn heading_parents(headings: &[Heading]) -> HashMap<u32, Option<u32>> {
    let mut parents = HashMap::new();
    let mut stack: Vec<(u8, u32)> = Vec::new();
    for (level, _, line) in headings {
        while stack.last().is_some_and(|(l, _)| l >= level) {
            stack.pop();
        }
        parents.insert(*line, stack.last().map(|(_, l)| *l));
        stack.push((*level, *line));
    }
    parents
}
```

```rust name=unique_and_find path=eval/plan.rs
/// Two blocks sharing a name is only a conflict when they also share a
/// file: decision 22's whole-file flat uniqueness, unchanged, just no
/// longer implicitly whole-*corpus* now that `blocks` can hold more than
/// one file's worth. `lib.md#setup` and `main.md#setup` coexisting is the
/// entire point of a cross-file `deps=`.
fn check_unique_names(blocks: &[BlockRef]) -> Result<(), PlanError> {
    let mut seen: HashSet<(&str, &str)> = HashSet::new();
    for b in blocks {
        if !seen.insert((b.file, b.name)) {
            return Err(PlanError::DuplicateName(b.name.to_string()));
        }
    }
    Ok(())
}

fn find_target(blocks: &[BlockRef], file: &str, target: &str) -> Result<usize, PlanError> {
    blocks
        .iter()
        .position(|b| b.file == file && b.name == target)
        .ok_or_else(|| PlanError::UnknownTarget(target.to_string()))
}
```

`plan_for` and `plan_for_index` compute the identical result. They
only differ in how the starting block is identified: by name, for a
reader typing `--block foo` at a prompt, or by position, for a caller
(`dankg check`'s staleness loop, the TUI's cycle-and-run) that already
holds the exact block it means from a plan or a cycle it built moments
earlier. That caller has no reason to route back through a name
lookup that decision 22's whole-file uniqueness makes entirely
redundant for it.

```rust name=plan_for path=eval/plan.rs
/// `target`'s transitive `deps=`, topologically ordered, `target` itself
/// last -- exactly the list `run.rs` concatenates into one file and spawns
/// once. A dependency may live in `file` itself or, via `other.md#name`,
/// in another file already present in `blocks` (see `eval::files`, which
/// loads exactly the files a chain reaches and nothing more -- eval still
/// never walks the whole root). Every block in the result shares the
/// language of `target`'s own occurrence, if it has one at all: mixing an
/// unstated dependency in a different language into the one interpreter
/// that will run the whole concatenated file is far more likely a mistake
/// than an intentional multi-language pipeline, so it is refused rather
/// than run.
pub fn plan_for<'a>(blocks: &'a [BlockRef<'a>], file: &str, target: &str) -> Result<Vec<BlockRef<'a>>, PlanError> {
    check_unique_names(blocks)?;
    let index = find_target(blocks, file, target)?;
    plan_from(blocks, file, index)
}

/// The same result as `plan_for`, from a block whose identity is already
/// known by position -- among `file`'s own named top-level blocks, in
/// document order, exactly as `top_level_blocks(doc, file)` would enumerate
/// them -- rather than a name to search for. `dankg check`'s per-block
/// staleness loop, `run_single`'s own multi-target loop, and
/// `tui::eval::run` all already hold the exact block they mean -- from a
/// plan or a cycle list built moments earlier -- so there is no reason for
/// any of them to route back through a name lookup at all. `blocks` may
/// hold other files' blocks too (pulled in by a cross-file `deps=`); `index`
/// still counts only among `file`'s own, so a caller that built `blocks`
/// from just that one file sees no change at all.
pub fn plan_for_index<'a>(
    blocks: &'a [BlockRef<'a>],
    file: &str,
    index: usize,
) -> Result<Vec<BlockRef<'a>>, PlanError> {
    check_unique_names(blocks)?;
    let target_index = blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| b.file == file)
        .nth(index)
        .map(|(i, _)| i)
        .ok_or_else(|| PlanError::UnknownTarget(format!("<block #{index}>")))?;
    plan_from(blocks, file, target_index)
}
```

`xdeps=` reuses `resolve_dep` exactly, the same name and cross-file
lookup `deps=` already uses. It never calls `visit`: an `xdeps` target
is not walked into the chain, not checked for a consistent language,
and not part of cycle detection, because nothing here ever runs it.
`eval::result::xdep_hashes` is the only caller, and it only ever wants
an index to look up an already-recorded hash by.

```rust name=resolve_xdeps path=eval/plan.rs
/// Resolves `block`'s `xdeps=` entries to their indices in `blocks`, in
/// declaration order. Same lookup `deps=` uses (`resolve_dep`), but the
/// result is never fed to `visit`: an `xdeps` target is referenced, not
/// run, so there is nothing here for a cycle to loop through and no
/// reason to require it share `block`'s own language.
pub fn resolve_xdeps(blocks: &[BlockRef], block: &BlockRef) -> Result<Vec<usize>, PlanError> {
    block
        .xdeps
        .iter()
        .map(|dep| resolve_dep(blocks, block.file, dep).map_err(|e| xdep_error(block.name.to_string(), dep, e)))
        .collect()
}
```

## File dependencies

`produces=file:PATH`/`reads=file:PATH` (decision 33) are the one place
this module compares two blocks' own declared attributes to each other,
rather than walking the dependency graph the way everything above does.
`check_file_deps` reuses `resolve_dep` exactly, to find which block a
`deps=`/`xdeps=` entry actually names, but never calls `visit`: nothing
here is planned, ordered, or run. It answers one question per block
declaring `reads=`: does the artifact it names match `produces=` on at
least one of the dependencies it already declared? Only `file` is a
recognised artifact kind today. Anything else -- a bare path with no
prefix, or a future `table:` for milestone 9's own DuckDB relations --
is left unrecognised on purpose, a documented gap rather than a second,
narrower error path.

```rust name=file_deps path=eval/plan.rs
/// One `produces=`/`reads=file:PATH` mismatch (decision 33), reported by
/// `dankg check`. Unlike a `dankg:depends` prose marker, this is a hard
/// failure: two declared strings failing to agree is a far stronger
/// signal than a substring search over prose.
#[derive(Debug, Clone, PartialEq)]
pub enum FileDepIssue {
    /// `block`'s own `reads=file:PATH` climbs above the root.
    ReadsEscapesRoot { file: String, line: u32, block: String, path: String },
    /// None of `block`'s resolved `deps=`/`xdeps=` targets declare a
    /// `produces=file:PATH` that names the same artifact.
    NoMatchingProducer { file: String, line: u32, block: String, path: String },
}

/// Splits an attribute's whole value into its artifact kind and path.
/// Only `file` is recognised today; anything else -- a bare path with no
/// prefix, or a future `table:` for milestone 9's own DuckDB relations --
/// returns `None`, silently unchecked rather than reported as an error
/// of its own.
fn parse_artifact(raw: &str) -> Option<&str> {
    let (kind, path) = raw.split_once(':')?;
    if kind != "file" || path.is_empty() {
        return None;
    }
    Some(path)
}

/// Resolves `path`, declared by a block living in `from_file`, to a
/// root-relative, normalized form -- the same `join_normalize`/`dir_of` a
/// written link's own target already resolves through, so two blocks in
/// different directories naming the same artifact by different relative
/// spellings still compare equal.
fn resolve_artifact(from_file: &str, path: &str) -> Option<String> {
    crate::graph::resolve::join_normalize(crate::graph::resolve::dir_of(from_file), path)
}

/// Checks every `reads=file:PATH` in `blocks` against the `produces=` of
/// whatever it already names in its own `deps=`/`xdeps=`. Returns the
/// number of declarations actually checked (a recognised `file:` kind),
/// alongside every mismatch found. Neither number moves for a block with
/// no `reads=`, or one naming a kind other than `file`: decision 33 is
/// additive, and a block that never opts in is never counted either way.
pub fn check_file_deps(blocks: &[BlockRef]) -> (usize, Vec<FileDepIssue>) {
    let mut checked = 0usize;
    let mut issues = Vec::new();

    for block in blocks {
        let Some(raw) = block.reads else { continue };
        let Some(path) = parse_artifact(raw) else { continue };
        checked += 1;

        let Some(wanted) = resolve_artifact(block.file, path) else {
            issues.push(FileDepIssue::ReadsEscapesRoot {
                file: block.file.to_string(),
                line: block.line,
                block: block.name.to_string(),
                path: path.to_string(),
            });
            continue;
        };

        let satisfied = block.deps.iter().chain(block.xdeps.iter()).any(|dep| {
            let Ok(producer) = resolve_dep(blocks, block.file, dep) else { return false };
            let producer = &blocks[producer];
            let Some(praw) = producer.produces else { return false };
            let Some(ppath) = parse_artifact(praw) else { return false };
            resolve_artifact(producer.file, ppath).as_deref() == Some(wanted.as_str())
        });

        if !satisfied {
            issues.push(FileDepIssue::NoMatchingProducer {
                file: block.file.to_string(),
                line: block.line,
                block: block.name.to_string(),
                path: path.to_string(),
            });
        }
    }

    (checked, issues)
}
```

`plan_each` is decision 21's kept-under-its-own-name meaning of the
*old* `--all`: every block in `file` becomes its own target, dependency
or not, each with its own recorded result -- the right tool for
smoke-testing everything in isolation. Order matters here in a way it
does not for `plan_all` below: a merged topological pass decides the
*sequence* of targets, so a block that depends on another always runs
after it, even though each target's own chain is independently rebuilt
from scratch.

```rust name=plan_each path=eval/plan.rs
/// Every top-level named block *in `file`*, each preceded by its own
/// transitive `deps=`, in one document-consistent order: a merged
/// topological pass decides the *sequence* of `file`'s own targets, so
/// that by the time a block that depends on another is run, the one it
/// depends on already has been, even though each target's own chain is
/// then rebuilt from scratch independently. `--each` runs every one of
/// these targets (decision 21) -- the pre-decision-21 meaning of `--all`,
/// kept under its own name because it is still the right tool for
/// smoke-testing every block in isolation, dependency or not, each with
/// its own recorded result. A block pulled in from another file via
/// `deps=` is never itself a target here -- `--each`/`--all` stay scoped to
/// the blocks actually named in `file`, exactly as they were before a
/// chain could reach outside it; only the blocks *reachable from* those
/// targets ever cross a file boundary.
pub fn plan_each<'a>(blocks: &'a [BlockRef<'a>], file: &str) -> Result<Vec<Vec<BlockRef<'a>>>, PlanError> {
    check_unique_names(blocks)?;

    let mut order = Vec::new();
    let mut done = HashSet::new();
    let mut stack = Vec::new();
    for i in 0..blocks.len() {
        if blocks[i].file == file {
            visit(blocks, file, i, &mut stack, &mut done, &mut order)?;
        }
    }
    // `order` interleaves every block the walk above touched, cross-file
    // dependencies included; a subsequence of a topological order is still
    // consistent with the partial order restricted to it, so filtering back
    // down to `file`'s own blocks is enough to recover their own relative
    // order without a second pass.
    order.into_iter().filter(|&i| blocks[i].file == file).map(|i| plan_from(blocks, file, i)).collect()
}
```

`plan_all` is decision 21's *new* meaning: only the DAG's leaves, so a
pure dependency like `setup` no longer earns a redundant standalone run
just for having a name. Two distinct leaves can never depend on one
another -- that would make one of them not a leaf -- so their relative
order carries no meaning, and this iterates them in plain declaration
order rather than computing a second merged topological pass it does not
need.

```rust name=plan_all path=eval/plan.rs
/// `--all`'s targets (decision 21): only the DAG's *leaves* among `file`'s
/// own blocks -- nothing else in `file` names it in a `deps=` -- each still
/// pulling its own full transitive chain (cross-file or not) exactly as
/// `--each` would. A pure dependency like `setup` no longer also gets a
/// standalone chain merely because it happens to be named -- that
/// standalone run was never anything a reader asked to see independently,
/// only a side effect of treating every named block uniformly. Two distinct
/// leaves can never depend on one another (that would make one of them not
/// a leaf), so their relative order among each other carries no meaning and
/// this iterates them in declaration order rather than computing a second
/// merged topological pass.
pub fn plan_all<'a>(blocks: &'a [BlockRef<'a>], file: &str) -> Result<Vec<Vec<BlockRef<'a>>>, PlanError> {
    check_unique_names(blocks)?;

    let mut depended_on: HashSet<usize> = HashSet::new();
    for b in blocks.iter().filter(|b| b.file == file) {
        for dep in &b.deps {
            let dep_index = resolve_dep(blocks, file, dep).map_err(|e| dep_error(b.name.to_string(), dep, e))?;
            depended_on.insert(dep_index);
        }
    }

    (0..blocks.len())
        .filter(|&i| blocks[i].file == file && !depended_on.contains(&i))
        .map(|i| plan_from(blocks, file, i))
        .collect()
}
```

`plan_from` is the shared core every public entry point above eventually
calls; `check_consistent_lang` can find the target with no name lookup at
all, because `visit`'s own post-order guarantee already puts it last in
the chain.

```rust name=plan_from_and_lang_check path=eval/plan.rs
fn plan_from<'a>(blocks: &'a [BlockRef<'a>], home: &str, index: usize) -> Result<Vec<BlockRef<'a>>, PlanError> {
    let mut order = Vec::new();
    let mut done = HashSet::new();
    let mut stack = Vec::new();
    visit(blocks, home, index, &mut stack, &mut done, &mut order)?;
    let chain: Vec<BlockRef> = order.into_iter().map(|i| blocks[i].clone()).collect();
    check_consistent_lang(&chain)?;
    Ok(chain)
}

/// `chain`'s last element is always its own starting point (`visit` is
/// post-order: dependencies are pushed before the block that asked for
/// them), so the target needs no name lookup to find.
fn check_consistent_lang(chain: &[BlockRef]) -> Result<(), PlanError> {
    let Some((target, deps)) = chain.split_last() else { return Ok(()) };
    let Some(target_lang) = target.lang else { return Ok(()) };
    for b in deps {
        if let Some(lang) = b.lang
            && lang != target_lang
        {
            return Err(PlanError::MixedLang {
                target: target.name.to_string(),
                target_lang: target_lang.to_string(),
                block: b.name.to_string(),
                block_lang: lang.to_string(),
            });
        }
    }
    Ok(())
}
```

`visit` is the one place a cycle can actually be detected: `stack` is the
live call path from the outermost request down to here, in `deps=`
declaration order, which is both the proof a cycle exists and exactly
what names it in the error. Resolution inside the loop always uses the
*visited block's own* file, never `home` -- a block two hops into another
file resolves its own local names against itself, not against wherever
the walk originally started.

```rust name=visit path=eval/plan.rs
/// Depth-first, dependencies before dependents, recording each block only
/// once by index. `stack` is the path from the outermost call to here, in
/// `deps=` declaration order -- both what proves a cycle and what names it
/// in the error. `home` is `plan_from`'s own starting file, used only to
/// decide whether a label needs a `file#` qualifier (`label`, above) --
/// resolution itself always uses the *visited block's own* file, not `home`,
/// since a block two hops into another file resolves its own local names
/// against itself, not against wherever the walk started.
fn visit(
    blocks: &[BlockRef],
    home: &str,
    index: usize,
    stack: &mut Vec<usize>,
    done: &mut HashSet<usize>,
    order: &mut Vec<usize>,
) -> Result<(), PlanError> {
    if done.contains(&index) {
        return Ok(());
    }
    if let Some(pos) = stack.iter().position(|&i| i == index) {
        let mut cycle: Vec<String> = stack[pos..].iter().map(|&i| label(&blocks[i], home)).collect();
        cycle.push(label(&blocks[index], home));
        return Err(PlanError::Cycle(cycle));
    }
    stack.push(index);
    let from_file = blocks[index].file;
    for dep in &blocks[index].deps {
        let dep_index =
            resolve_dep(blocks, from_file, dep).map_err(|e| dep_error(label(&blocks[index], home), dep, e))?;
        visit(blocks, home, dep_index, stack, done, order)?;
    }
    stack.pop();
    done.insert(index);
    order.push(index);
    Ok(())
}
```

## Tests

Cross-file `deps=` tests build the multi-file `blocks` slice by hand,
exactly the shape `eval::files::Files::all_blocks` produces, since
`plan.rs` itself never touches the filesystem -- that lets them exercise
resolution and error-labelling in isolation from any I/O.

```rust name=tests path=eval/plan.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::Diags;

    const FILE: &str = "t.md";

    fn doc(src: &str) -> Document {
        Document::parse(src, &mut Diags::new(FILE))
    }

    #[test]
    fn a_single_block_with_no_deps_plans_to_itself() {
        let d = doc("```python name=only\nprint(1)\n```\n");
        let blocks = top_level_blocks(&d, FILE);
        let plan = plan_for(&blocks, FILE, "only").unwrap();
        assert_eq!(plan.iter().map(|b| b.name).collect::<Vec<_>>(), vec!["only"]);
    }

    #[test]
    fn transitive_deps_come_before_the_target_in_declaration_order() {
        let d = doc(
            "```python name=setup\na=1\n```\n\n```python name=fetch deps=setup\nb=2\n```\n\n```python name=index deps=fetch\nc=3\n```\n",
        );
        let blocks = top_level_blocks(&d, FILE);
        let plan = plan_for(&blocks, FILE, "index").unwrap();
        assert_eq!(plan.iter().map(|b| b.name).collect::<Vec<_>>(), vec!["setup", "fetch", "index"]);
    }

    #[test]
    fn deps_resolve_across_sibling_headings() {
        // The most natural literate-pipeline shape: each stage its own
        // heading, each depending on the last. Decision 22 tried scoping
        // `deps=` lexically (own heading, then ancestors only) and broke
        // exactly this; resolution is flat, whole-file, on purpose.
        let d = doc("# One\n\n```sh name=a\n:\n```\n\n# Two\n\n```sh name=b deps=a\n:\n```\n");
        let blocks = top_level_blocks(&d, FILE);
        let plan = plan_for(&blocks, FILE, "b").unwrap();
        assert_eq!(plan.iter().map(|b| b.name).collect::<Vec<_>>(), vec!["a", "b"]);
    }

    #[test]
    fn a_shared_dependency_appears_only_once() {
        let d = doc(
            "```sh name=setup\n:\n```\n\n```sh name=a deps=setup\n:\n```\n\n```sh name=b deps=setup\n:\n```\n\n```sh name=top deps=a,b\n:\n```\n",
        );
        let blocks = top_level_blocks(&d, FILE);
        let plan = plan_for(&blocks, FILE, "top").unwrap();
        assert_eq!(plan.iter().map(|b| b.name).collect::<Vec<_>>(), vec!["setup", "a", "b", "top"]);
    }

    #[test]
    fn an_unknown_target_is_reported() {
        let d = doc("```sh name=only\n:\n```\n");
        let blocks = top_level_blocks(&d, FILE);
        assert_eq!(plan_for(&blocks, FILE, "nope").unwrap_err(), PlanError::UnknownTarget("nope".into()));
    }

    #[test]
    fn an_unknown_dependency_names_both_ends() {
        let d = doc("```sh name=top deps=ghost\n:\n```\n");
        let blocks = top_level_blocks(&d, FILE);
        assert_eq!(
            plan_for(&blocks, FILE, "top").unwrap_err(),
            PlanError::UnknownDep { block: "top".into(), dep: "ghost".into() }
        );
    }

    #[test]
    fn a_two_block_cycle_is_reported_not_silently_broken() {
        let d = doc("```sh name=a deps=b\n:\n```\n\n```sh name=b deps=a\n:\n```\n");
        let blocks = top_level_blocks(&d, FILE);
        let err = plan_for(&blocks, FILE, "a").unwrap_err();
        assert!(matches!(err, PlanError::Cycle(_)), "{err:?}");
    }

    #[test]
    fn a_self_dependency_is_a_one_element_cycle() {
        let d = doc("```sh name=a deps=a\n:\n```\n");
        let blocks = top_level_blocks(&d, FILE);
        assert!(matches!(plan_for(&blocks, FILE, "a").unwrap_err(), PlanError::Cycle(_)));
    }

    #[test]
    fn duplicate_names_are_refused_even_when_unrelated_to_the_target() {
        let d = doc("```sh name=x\n:\n```\n\n```sh name=x\n:\n```\n");
        let blocks = top_level_blocks(&d, FILE);
        assert_eq!(plan_for(&blocks, FILE, "x").unwrap_err(), PlanError::DuplicateName("x".into()));
    }

    #[test]
    fn duplicate_names_under_different_headings_are_still_refused() {
        // Decision 22's relaxation (allowed once heading-scoped) is gone:
        // names are whole-file unique again.
        let d = doc("# One\n\n```sh name=setup\n:\n```\n\n# Two\n\n```sh name=setup\n:\n```\n");
        let blocks = top_level_blocks(&d, FILE);
        assert_eq!(plan_for(&blocks, FILE, "setup").unwrap_err(), PlanError::DuplicateName("setup".into()));
    }

    #[test]
    fn a_named_block_nested_in_a_list_is_invisible_to_eval() {
        let d = doc("- ```sh name=hidden\n  :\n  ```\n");
        assert!(top_level_blocks(&d, FILE).is_empty());
    }

    #[test]
    fn a_dependency_in_a_different_language_is_refused() {
        let d = doc("```sh name=setup\n:\n```\n\n```python name=top deps=setup\n:\n```\n");
        let blocks = top_level_blocks(&d, FILE);
        let err = plan_for(&blocks, FILE, "top").unwrap_err();
        assert_eq!(
            err,
            PlanError::MixedLang {
                target: "top".into(),
                target_lang: "python".into(),
                block: "setup".into(),
                block_lang: "sh".into(),
            }
        );
    }

    #[test]
    fn an_xdep_in_a_different_language_is_not_refused() {
        let d = doc("```sh name=setup\n:\n```\n\n```python name=top xdeps=setup\n:\n```\n");
        let blocks = top_level_blocks(&d, FILE);
        assert!(plan_for(&blocks, FILE, "top").is_ok(), "xdeps never triggers MixedLang");
    }

    #[test]
    fn an_xdep_is_never_concatenated_into_the_chain() {
        let d = doc("```sh name=setup\n:\n```\n\n```python name=top xdeps=setup\n:\n```\n");
        let blocks = top_level_blocks(&d, FILE);
        let plan = plan_for(&blocks, FILE, "top").unwrap();
        assert_eq!(plan.iter().map(|b| b.name).collect::<Vec<_>>(), vec!["top"], "setup is xdeps-only, not part of the run");
    }

    #[test]
    fn resolve_xdeps_finds_a_same_file_target() {
        let d = doc("```sh name=setup\n:\n```\n\n```python name=top xdeps=setup\n:\n```\n");
        let blocks = top_level_blocks(&d, FILE);
        let top = blocks.iter().find(|b| b.name == "top").unwrap();
        let indices = resolve_xdeps(&blocks, top).unwrap();
        assert_eq!(blocks[indices[0]].name, "setup");
    }

    #[test]
    fn resolve_xdeps_resolves_across_files() {
        let a = Document::parse("```python name=top xdeps=lib.md#setup\n:\n```\n", &mut Diags::new("a.md"));
        let lib = Document::parse("```sh name=setup\n:\n```\n", &mut Diags::new("lib.md"));
        let blocks = combined(&[("a.md", &a), ("lib.md", &lib)]);
        let top = blocks.iter().find(|b| b.name == "top").unwrap();
        let indices = resolve_xdeps(&blocks, top).unwrap();
        assert_eq!((blocks[indices[0]].file, blocks[indices[0]].name), ("lib.md", "setup"));
    }

    #[test]
    fn resolve_xdeps_reports_an_unknown_target() {
        let d = doc("```sh name=top xdeps=ghost\n:\n```\n");
        let blocks = top_level_blocks(&d, FILE);
        let top = blocks.iter().find(|b| b.name == "top").unwrap();
        assert_eq!(
            resolve_xdeps(&blocks, top).unwrap_err(),
            PlanError::UnknownXDep { block: "top".into(), dep: "ghost".into() }
        );
    }

    #[test]
    fn resolve_xdeps_reports_a_dependency_that_escapes_the_root() {
        let d = doc("```sh name=top xdeps=../../etc.md#x\n:\n```\n");
        let blocks = top_level_blocks(&d, FILE);
        let top = blocks.iter().find(|b| b.name == "top").unwrap();
        assert_eq!(
            resolve_xdeps(&blocks, top).unwrap_err(),
            PlanError::XDepEscapesRoot { block: "top".into(), dep: "../../etc.md#x".into() }
        );
    }

    // -- `produces=`/`reads=file:PATH` (decision 33) --------------------

    #[test]
    fn a_block_with_no_reads_is_not_checked() {
        let d = doc("```sh name=only\n:\n```\n");
        let blocks = top_level_blocks(&d, FILE);
        let (checked, issues) = check_file_deps(&blocks);
        assert_eq!(checked, 0);
        assert!(issues.is_empty());
    }

    #[test]
    fn matching_produces_and_reads_via_deps_is_not_an_issue() {
        let d = doc(
            "```sh name=writer produces=file:out.csv\n:\n```\n\n```sh name=reader deps=writer reads=file:out.csv\n:\n```\n",
        );
        let blocks = top_level_blocks(&d, FILE);
        let (checked, issues) = check_file_deps(&blocks);
        assert_eq!(checked, 1);
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn matching_produces_and_reads_via_xdeps_across_languages_is_not_an_issue() {
        let d = doc(
            "```sh name=writer produces=file:out.csv\n:\n```\n\n```python name=reader xdeps=writer reads=file:out.csv\n:\n```\n",
        );
        let blocks = top_level_blocks(&d, FILE);
        let (checked, issues) = check_file_deps(&blocks);
        assert_eq!(checked, 1);
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn a_mismatched_path_is_reported() {
        let d = doc(
            "```sh name=writer produces=file:out.csv\n:\n```\n\n```sh name=reader deps=writer reads=file:different.csv\n:\n```\n",
        );
        let blocks = top_level_blocks(&d, FILE);
        let (checked, issues) = check_file_deps(&blocks);
        assert_eq!(checked, 1);
        assert_eq!(
            issues,
            vec![FileDepIssue::NoMatchingProducer {
                file: FILE.into(),
                line: 5,
                block: "reader".into(),
                path: "different.csv".into(),
            }]
        );
    }

    #[test]
    fn a_dependency_that_declares_no_produces_is_reported() {
        let d = doc("```sh name=writer\n:\n```\n\n```sh name=reader deps=writer reads=file:out.csv\n:\n```\n");
        let blocks = top_level_blocks(&d, FILE);
        let (checked, issues) = check_file_deps(&blocks);
        assert_eq!(checked, 1);
        assert!(matches!(issues.as_slice(), [FileDepIssue::NoMatchingProducer { .. }]), "{issues:?}");
    }

    #[test]
    fn reads_with_no_dependency_edge_at_all_is_reported() {
        let d = doc("```sh name=reader reads=file:out.csv\n:\n```\n");
        let blocks = top_level_blocks(&d, FILE);
        let (checked, issues) = check_file_deps(&blocks);
        assert_eq!(checked, 1);
        assert!(matches!(issues.as_slice(), [FileDepIssue::NoMatchingProducer { .. }]), "{issues:?}");
    }

    #[test]
    fn a_reads_path_escaping_the_root_is_reported() {
        let d = doc("```sh name=reader reads=file:../../etc/passwd\n:\n```\n");
        let blocks = top_level_blocks(&d, FILE);
        let (checked, issues) = check_file_deps(&blocks);
        assert_eq!(checked, 1);
        assert_eq!(
            issues,
            vec![FileDepIssue::ReadsEscapesRoot {
                file: FILE.into(),
                line: 1,
                block: "reader".into(),
                path: "../../etc/passwd".into(),
            }]
        );
    }

    #[test]
    fn an_unrecognized_artifact_kind_is_not_checked() {
        // `table:` is reserved for milestone 9's own DuckDB relations; it
        // is not an error today, just silently unchecked.
        let d = doc("```sh name=reader reads=table:orders\n:\n```\n");
        let blocks = top_level_blocks(&d, FILE);
        let (checked, issues) = check_file_deps(&blocks);
        assert_eq!(checked, 0);
        assert!(issues.is_empty());
    }

    #[test]
    fn a_match_among_several_dependencies_is_enough() {
        let d = doc(
            "```sh name=other\n:\n```\n\n```sh name=writer produces=file:out.csv\n:\n```\n\n```sh name=reader deps=other,writer reads=file:out.csv\n:\n```\n",
        );
        let blocks = top_level_blocks(&d, FILE);
        let (checked, issues) = check_file_deps(&blocks);
        assert_eq!(checked, 1);
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn different_relative_spellings_of_the_same_artifact_still_match() {
        // Producer and reader sit in different directories and spell the
        // same repo-relative path differently. Only normalization, not a
        // raw string compare, can see they agree.
        let producer =
            Document::parse("```sh name=producer produces=file:../data/out.csv\n:\n```\n", &mut Diags::new("cross/prod.md"));
        let reader = Document::parse(
            "```sh name=reader deps=../../cross/prod.md#producer reads=file:../../data/out.csv\n:\n```\n",
            &mut Diags::new("cross/deep/read.md"),
        );
        let blocks = combined(&[("cross/prod.md", &producer), ("cross/deep/read.md", &reader)]);
        let (checked, issues) = check_file_deps(&blocks);
        assert_eq!(checked, 1);
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn plan_each_runs_a_dependency_before_whatever_declares_it_first() {
        // `b` is declared before `a` in the file, but `b` depends on `a`, so
        // `--each` must still run `a` first -- topological order wins over
        // declaration order.
        let d = doc("```sh name=b deps=a\n:\n```\n\n```sh name=a\n:\n```\n");
        let blocks = top_level_blocks(&d, FILE);
        let chains = plan_each(&blocks, FILE).unwrap();
        let targets: Vec<&str> = chains.iter().map(|c| c.last().unwrap().name).collect();
        assert_eq!(targets, vec!["a", "b"]);
        assert_eq!(chains[1].iter().map(|b| b.name).collect::<Vec<_>>(), vec!["a", "b"], "b's own chain still includes its dependency");
    }

    #[test]
    fn plan_each_reports_the_same_errors_as_plan_for() {
        let d = doc("```sh name=a deps=ghost\n:\n```\n");
        let blocks = top_level_blocks(&d, FILE);
        assert!(matches!(plan_each(&blocks, FILE).unwrap_err(), PlanError::UnknownDep { .. }));
    }

    #[test]
    fn plan_each_gives_every_block_its_own_chain_dependency_or_not() {
        let d = doc("```sh name=setup\n:\n```\n\n```sh name=go deps=setup\n:\n```\n");
        let blocks = top_level_blocks(&d, FILE);
        let chains = plan_each(&blocks, FILE).unwrap();
        let targets: Vec<&str> = chains.iter().map(|c| c.last().unwrap().name).collect();
        assert_eq!(targets, vec!["setup", "go"], "setup still gets its own standalone chain under --each");
    }

    #[test]
    fn plan_all_only_reports_leaves() {
        let d = doc("```sh name=setup\n:\n```\n\n```sh name=go deps=setup\n:\n```\n");
        let blocks = top_level_blocks(&d, FILE);
        let chains = plan_all(&blocks, FILE).unwrap();
        let targets: Vec<&str> = chains.iter().map(|c| c.last().unwrap().name).collect();
        assert_eq!(targets, vec!["go"], "setup is a pure dependency, not a leaf");
        assert_eq!(chains[0].iter().map(|b| b.name).collect::<Vec<_>>(), vec!["setup", "go"], "go's own chain still includes its dependency");
    }

    #[test]
    fn plan_all_reports_every_leaf_when_there_are_several() {
        let d = doc("```sh name=setup\n:\n```\n\n```sh name=a deps=setup\n:\n```\n\n```sh name=b deps=setup\n:\n```\n");
        let blocks = top_level_blocks(&d, FILE);
        let chains = plan_all(&blocks, FILE).unwrap();
        let targets: Vec<&str> = chains.iter().map(|c| c.last().unwrap().name).collect();
        assert_eq!(targets, vec!["a", "b"], "setup runs twice, once inside each leaf's own chain, never standalone");
    }

    #[test]
    fn plan_all_reports_the_same_errors_as_plan_for() {
        let d = doc("```sh name=a deps=ghost\n:\n```\n");
        let blocks = top_level_blocks(&d, FILE);
        assert!(matches!(plan_all(&blocks, FILE).unwrap_err(), PlanError::UnknownDep { .. }));
    }

    #[test]
    fn plan_for_index_runs_the_block_at_that_position() {
        let d = doc("```sh name=a\necho a\n```\n\n```sh name=b\necho b\n```\n");
        let blocks = top_level_blocks(&d, FILE);
        let p = plan_for_index(&blocks, FILE, 1).unwrap();
        assert_eq!(p.last().unwrap().name, "b");
    }

    // -- Cross-file `deps=` (architecture.md, Code evaluation) --------
    //
    // `plan.rs` itself never touches the filesystem (`eval::files` does the
    // loading); these tests build the multi-file `blocks` slice by hand,
    // exactly the shape `files::Files::all_blocks` produces, to exercise the
    // resolution and error-labelling logic in isolation from any I/O.

    fn combined<'a>(files: &[(&'a str, &'a Document)]) -> Vec<BlockRef<'a>> {
        files.iter().flat_map(|(file, d)| top_level_blocks(d, file)).collect()
    }

    #[test]
    fn a_dependency_resolves_into_another_file() {
        let a = Document::parse("```sh name=go deps=lib.md#helper\necho go\n```\n", &mut Diags::new("a.md"));
        let lib = Document::parse("```sh name=helper\necho help\n```\n", &mut Diags::new("lib.md"));
        let blocks = combined(&[("a.md", &a), ("lib.md", &lib)]);
        let plan = plan_for(&blocks, "a.md", "go").unwrap();
        assert_eq!(plan.iter().map(|b| (b.file, b.name)).collect::<Vec<_>>(), vec![("lib.md", "helper"), ("a.md", "go")]);
    }

    #[test]
    fn a_relative_cross_file_dependency_resolves_against_the_declaring_files_own_directory() {
        let a = Document::parse(
            "```sh name=go deps=../lib.md#helper\necho go\n```\n",
            &mut Diags::new("notes/a.md"),
        );
        let lib = Document::parse("```sh name=helper\necho help\n```\n", &mut Diags::new("lib.md"));
        let blocks = combined(&[("notes/a.md", &a), ("lib.md", &lib)]);
        let plan = plan_for(&blocks, "notes/a.md", "go").unwrap();
        assert_eq!(plan.last().unwrap().name, "go");
        assert_eq!(plan[0].file, "lib.md");
    }

    #[test]
    fn the_same_name_in_two_files_is_not_a_duplicate() {
        let a = Document::parse("```sh name=setup\n:\n```\n", &mut Diags::new("a.md"));
        let b = Document::parse("```sh name=setup\n:\n```\n", &mut Diags::new("b.md"));
        let blocks = combined(&[("a.md", &a), ("b.md", &b)]);
        assert!(plan_for(&blocks, "a.md", "setup").is_ok());
    }

    #[test]
    fn a_local_dependency_declared_by_a_cross_file_block_resolves_within_its_own_file_not_the_entry_files() {
        // `lib.md`'s own `helper` depends on `base`, a name that also exists
        // in `a.md` -- it must resolve to `lib.md#base`, not `a.md#base`.
        let a = Document::parse(
            "```sh name=go deps=lib.md#helper\necho go\n```\n\n```sh name=base\necho wrong\n```\n",
            &mut Diags::new("a.md"),
        );
        let lib = Document::parse(
            "```sh name=base\necho right\n```\n\n```sh name=helper deps=base\necho help\n```\n",
            &mut Diags::new("lib.md"),
        );
        let blocks = combined(&[("a.md", &a), ("lib.md", &lib)]);
        let plan = plan_for(&blocks, "a.md", "go").unwrap();
        let base = plan.iter().find(|b| b.name == "base").unwrap();
        assert_eq!(base.file, "lib.md");
    }

    #[test]
    fn a_cross_file_dependency_that_climbs_above_the_root_is_refused() {
        let a = Document::parse(
            "```sh name=go deps=../../etc.md#x\necho go\n```\n",
            &mut Diags::new("a.md"),
        );
        let blocks = top_level_blocks(&a, "a.md");
        assert_eq!(
            plan_for(&blocks, "a.md", "go").unwrap_err(),
            PlanError::DepEscapesRoot { block: "go".into(), dep: "../../etc.md#x".into() }
        );
    }

    #[test]
    fn a_dependency_naming_a_file_never_loaded_is_an_ordinary_unknown_dep() {
        // `plan.rs` cannot tell "the file was never loaded" apart from "the
        // name is not in it" -- both look like a missing block from here,
        // which is `eval::files`'s job to have prevented by loading whatever
        // a chain actually reaches.
        let a = Document::parse(
            "```sh name=go deps=ghost.md#x\necho go\n```\n",
            &mut Diags::new("a.md"),
        );
        let blocks = top_level_blocks(&a, "a.md");
        assert_eq!(
            plan_for(&blocks, "a.md", "go").unwrap_err(),
            PlanError::UnknownDep { block: "go".into(), dep: "ghost.md#x".into() }
        );
    }

    #[test]
    fn a_cross_file_cycle_is_reported_with_qualified_labels() {
        let a = Document::parse("```sh name=go deps=b.md#back\necho a\n```\n", &mut Diags::new("a.md"));
        let b = Document::parse("```sh name=back deps=a.md#go\necho b\n```\n", &mut Diags::new("b.md"));
        let blocks = combined(&[("a.md", &a), ("b.md", &b)]);
        let err = plan_for(&blocks, "a.md", "go").unwrap_err();
        let PlanError::Cycle(path) = err else { panic!("expected a cycle, got {err:?}") };
        assert_eq!(path, vec!["go".to_string(), "b.md#back".to_string(), "go".to_string()]);
    }

    #[test]
    fn plan_all_and_plan_each_never_treat_a_pulled_in_cross_file_block_as_its_own_target() {
        let a = Document::parse(
            "```sh name=go deps=lib.md#helper\necho go\n```\n",
            &mut Diags::new("a.md"),
        );
        let lib = Document::parse("```sh name=helper\necho help\n```\n", &mut Diags::new("lib.md"));
        let blocks = combined(&[("a.md", &a), ("lib.md", &lib)]);

        let all = plan_all(&blocks, "a.md").unwrap();
        assert_eq!(all.iter().map(|c| c.last().unwrap().name).collect::<Vec<_>>(), vec!["go"]);

        let each = plan_each(&blocks, "a.md").unwrap();
        assert_eq!(each.iter().map(|c| c.last().unwrap().name).collect::<Vec<_>>(), vec!["go"]);
    }

    #[test]
    fn root_heading_walks_to_the_top_of_the_containment_tree() {
        let d = doc("# One\n\n## Two\n\n### Three\n\n```sh name=x\n:\n```\n");
        let headings = d.headings();
        let three_line = headings[2].2;
        assert_eq!(root_heading(&headings, Some(three_line)), Some(headings[0].2));
    }

    #[test]
    fn root_heading_of_the_file_level_scope_is_the_file_level_scope() {
        let d = doc("just text\n");
        assert_eq!(root_heading(&d.headings(), None), None);
    }

    #[test]
    fn root_heading_handles_a_document_with_no_level_one_wrapper() {
        // No "# " at all -- "Two" is its own top, not `None`.
        let d = doc("## Two\n\n### Three\n");
        let headings = d.headings();
        let three_line = headings[1].2;
        assert_eq!(root_heading(&headings, Some(three_line)), Some(headings[0].2));
    }
}
```
