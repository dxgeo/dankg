# `dankg weave`: rendering a block's produced artifacts

## Context

Decision 46 pairs a recorded eval result with its source block, but
the "output" half is always the block's own captured stdout -- text,
nothing else. A block whose real point is a chart or a data table
usually does not print either one; it writes a file
(`plt.savefig("chart.png")`, `df.to_csv("summary.csv")`) and maybe
logs a line to stdout. Decision 45's own rationale already named this
gap and deferred it:

<!-- dankg:depends target=../architecture.md#decision-45-data-tables-from-csvjsontsv-fenced-blocks quote="Tagging an eval result fence with its own output format automatically is deferred" -->

This plan closes it, for two artifact shapes: an image
(`png`/`jpg`/`jpeg`/`gif`/`svg`/`webp`), rendered as a real image, and
a `csv`/`tsv`/`json` file, rendered as a real table -- both through
the attribute a block already has for exactly this: `produces=file:PATH`
(decision 33).

<!-- dankg:depends target=../architecture.md#decision-33-file-artifact-dependency quote="each paired with an ordinary" -->

**A correction to a premise worth stating up front.** `produces=file:PATH`
today is pure textual bookkeeping: `check_file_deps` compares a
reader's `reads=` path against its declared producer's `produces=`
path as strings. Nothing anywhere opens the file either one names.

<!-- dankg:depends target=../src/eval/plan.md#file-dependencies quote="this module compares two blocks' own declared attributes to each other" -->

Rendering an artifact is the first time anything in this codebase
reads bytes off a path `produces=` names. That surfaces a real,
already-latent bug: the process `dankg eval` spawns inherits
whatever directory `dankg` itself was invoked from, not the
directory `produces=file:PATH` already resolves relative to. That
mismatch has always existed; it has just never mattered until a
script's own relative file write needs to land where `produces=`
says it will. This plan fixes it, confirmed with the user, since the
whole feature is unreliable without that fix.

## What this reuses

**`eval::plan::parse_artifact`/`resolve_artifact`.** The `file:`
prefix parse and the root-relative path resolution `check_file_deps`
already uses are made `pub(crate)` (the same move decision 47 already
made for `corpus_graph_if_needed`) so weave resolves a `produces=file:`
value exactly the way `dankg check` already verifies one -- one
resolution, not two that could drift apart.

<!-- dankg:depends target=../src/eval/plan.md#file-dependencies quote="is a recognised artifact kind today" -->

**`data::table::from_delimited`/`from_json` and the shared `TableData`
emitter (decision 45).** A file's own content, read into a `String`,
is exactly what both functions already take. The table this produces
goes through the identical emitter a `csv`/`json`-tagged fence's own
inline content already renders through -- one code path regardless of
source.

**`eval::result::recognize_pair` (decision 46).** Artifact rendering
only ever applies inside a recognized pair. A plain named block with
no recorded result is not evidence that its `produces=file:` target
reflects what is on disk right now; only a block `dankg eval` has
actually run carries that evidence.

**The "misconfigured is reported, not fatal" pattern.** `weave.rs`'s
own `read_asset` already warns and skips a missing `[weave.html] css`
or `[weave.pdf] template` rather than failing the run. A missing or
unreadable artifact gets the identical treatment -- a stderr warning,
nothing rendered for that one artifact, the rest of the page
unaffected.

**Decision 1 (zero crates).** A hand-rolled base64 encoder for the
HTML backend's own `data:` URI is new code, not a new dependency --
the same choice `data::table`'s hand-rolled CSV/JSON readers already
made for the identical reason.

<!-- dankg:depends target=../architecture.md#decision-1-dependency-policy quote="Zero crates, std only, forever." -->

## What's new

### Decision 49: eval spawns in the declaring file's own directory

`eval::run::run`/`run_db`/`run_at` gain a `dir: &Path` parameter,
threaded straight to `Command::current_dir`. `eval::session::run_one` --
the one and only caller of either function -- computes it as
`root.join(dir_of(entry_file))` and passes it through. `run_one`'s own
public signature does not change. None of its existing callers
(`run_single`, `tui::eval`) or its own test suite need to change
either.

**Rationale:** Without a fixed working directory, a script's own
relative file write lands wherever the shell happened to be when
`dankg` was invoked, not where `produces=file:PATH` already resolves
it (decision 33's own `dir_of(from_file)`). This was always a latent
gap. It only becomes load-bearing once something actually reads the
file back, which this plan is the first thing to do. A chain that
concatenates blocks from more than one directory still has no single
correct answer -- accepted as a known limitation, not solved here.

### Decision 50: A `produces=file:` CSV/TSV/JSON artifact renders as a table

For a recognized pair, the source block's own `info.produces()` --
never the marker's `produces`/`reads`, which is decision 36's
inferred SQL relation names and an unrelated mechanism -- is checked
for a `file:` prefix. A `.csv`/`.tsv`/`.json` extension resolves,
reads, and renders through decision 45's own table pipeline, appended
inside the same paired unit, below the captured stdout: both show,
since stdout might be a log line while the real content lives in the
file. A `.json` file that does not parse as an array of objects or an
array of arrays falls back to an ordinary code block, the identical
fallback decision 45 already gives an inline `json` fence. A missing
or unreadable file warns on stderr and renders nothing extra. Any
other extension renders nothing extra either, silently -- the
extension is the only signal, never content-sniffed (decision 45's
own stance, unchanged).

### Decision 51: A `produces=file:` image renders as a real image

A `png`/`jpg`/`jpeg`/`gif`/`svg`/`webp` extension resolves and reads
the same way. In HTML, the raw bytes are base64-encoded and inlined
as `<img src="data:image/…;base64,…">`, keeping the woven page
self-contained (decision 43) -- no second file to ship alongside it.
In Typst, the resolved artifact is copied to
`.dankg/build/weave/assets/<root-relative-path>` before compiling,
mirroring tangle's own per-file nesting (decision 26), and referenced
as `#image("assets/<root-relative-path>")` -- a path always local to
the `.typ`'s own directory, sidestepping any question of how Typst's
own project root affects a relative image path elsewhere. Missing or
unreadable gets the identical stderr-warn-and-skip treatment as
decision 50.

## What this explicitly does not do

- More than one `produces=file:` artifact per block. `InfoString::produces()`
  returns its whole raw value unsplit, unlike `deps()`/`xdeps()`'s own
  comma-split. Comma-splitting it is a separate, later decision. One
  block, one artifact, for now -- a second artifact is a second block.
- Auto-inferring a produced file the way decision 36 infers a SQL
  block's own table writes. A file artifact is always hand-declared
  with `produces=file:PATH`; nothing here sniffs a script's own
  filesystem calls to guess one.
- Solving the working-directory question for a chain whose
  concatenated blocks live in more than one directory. Decision 49
  covers the single-file case, which is the overwhelming common one.
- Any change to how a `csv`/`json`-tagged *fence's own inline content*
  already renders (decision 45). This is a second, independent source
  feeding the identical `TableData`/emitter, not a replacement.
- Guessing an unrecognized extension is "probably" an image or a
  table. The extension is the only signal a reader gets to act on.

## Critical files

- `src/eval/run.md` / `eval/run.rs` -- `run`/`run_db`/`run_at` gain
  `dir: &Path`.
- `src/eval/session.md` / `eval/session.rs` -- `run_one` computes and
  passes it; its own signature and every existing caller stay
  unchanged.
- `src/eval/plan.md` / `eval/plan.rs` -- `parse_artifact`/`resolve_artifact`
  made `pub(crate)`.
- `src/data/table.md` -- unchanged, reused as-is by both new render
  paths.
- `src/render/weave_html.md` / `weave_html.rs` and `src/render/typst.md`
  / `typst.rs` -- `eval_pair` grows an artifact-rendering step; the new
  base64 encoder lives in `weave_html.rs`, the only backend that needs
  one.
- `src/weave.md` / `weave.rs` -- `render_pdf`'s own `build_dir` is
  where a Typst-bound image gets copied to.
- `architecture.md` -- add decisions 49, 50, 51.

## Verification

1. Edit loop per CLAUDE.md, then `dankg fmt --check` and `dankg check .`
   over the whole corpus.
2. Unit tests, scratch directories, real files on disk (existing
   style):
   - `run_one` spawns with the declaring file's own directory as its
     working directory -- a block whose script opens a relative path
     finds it regardless of the test process's own cwd.
   - `produces=file:data.csv`/`.json` resolves and renders as a table
     in both backends; a non-table-shaped JSON file falls back to a
     code block, warned.
   - `produces=file:chart.png` renders as a base64 `<img>` in HTML;
     the Typst backend copies it under `.dankg/build/weave/assets/`
     and emits a matching `#image(...)`, checked against the real
     `typst` binary the same way `tests/typst.rs` already does.
   - A missing or unreadable artifact warns on stderr and changes
     nothing else about the rendered pair.
3. A manual smoke test against `dankg_weave_example`: a Python block
   that actually writes a PNG (`matplotlib`) and a CSV
   (`pandas`/the standard library `csv` module) to disk, `dankg eval`,
   then `dankg weave --format html` and `--format pdf`, confirming
   both render for real in an opened browser and an opened PDF. Any
   package the example's own script imports is a dependency of the
   *example*, never of `dankg` itself -- decision 1 bounds what
   `dankg`'s own `Cargo.toml` may depend on, not what a reader's
   evaluated code imports.
