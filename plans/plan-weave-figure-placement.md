# `dankg weave`: `--figures-inside`/`--figures-outside`, with a per-block override

## Context

Decision 54 wraps a captioned `produces=file:` artifact in a real
figure. Decision 55 moved that figure, in the Typst backend, from
inside the pair's own stroked `#block(...)` to a sibling right after
it, so a `[weave.pdf] template` could style it independently -- a
`#show`/`#set` rule restyles an element it matches, but it cannot
strip a stroke a block already applies to its own body, so the only
way to make placement configurable *at all* from a template was to
change dankg's own default and let a template opt back into the old
look.

<!-- dankg:depends target=../architecture.md#decision-55-a-captioned-figure-renders-outside-the-pairs-own-block-in-typst quote="nothing to unwrap a figure out of a box from the outside" -->

That worked, but only by asking the template to reconstruct "inside"
out of two independently-laid-out pieces: a stroke color matched by
hand to whichever of `gray`/`red` decision 46 happened to choose, and
a hand-tuned negative `above` margin (`-1.2em`, Typst's own default
block spacing) to cancel the gap decision 55's own split introduced.
Confirmed with the user: that is not a good foundation for something
this ordinary. A reader who liked the pre-55 look has no way back to
it that does not depend on knowing Typst's own default spacing by
heart.

This plan replaces the template-only lever with two dankg-native
ones: a `dankg weave` CLI flag setting the *document's* own default
placement, and a per-block `figure=` attribute that overrides it for
one block's own artifact. Confirmed with the user: the CLI flag's own
default reverts to decision 54's original behavior (figure inside
the block); `--figures-outside` asks for decision 55's own placement
instead, natively -- no template spacing/color hack required either
way, since dankg itself now renders each mode correctly as one clean
call, never as two pieces a template has to reassemble.

## What this reuses

**Decision 55's own figure-splitting machinery, made conditional.**
The Typst backend already builds a captioned artifact's `#figure(...)`
into its own buffer, separate from the pair's own `body`, precisely so
it *could* be appended after the block closes. Nothing about that
splitting changes. What changes is where the resolved placement sends
it: spliced into `body` before the block is formatted (inside), or
appended after it, exactly as decision 55 already does (outside).

**The `weave=`-style attribute pattern (decisions 48/52/53).** A new
`figure` key joins `KNOWN_ATTRS`. `InfoString::figure_outside(&self)`,
returning `Option<bool>`, reads it the same loose way `weave_hidden()`
already reads `weave=hidden`: `figure=inside` and `figure=outside` are
the two recognized values, `None` for anything else, including
absence -- never a hard error, matching every other attribute here.

**`--toc`/`--no-toc`'s own CLI shape (decision 44).** A boolean pair,
parsed the identical way in `cli::weave`, threaded down through
`weave::run` into both `render::html::render` and
`render::typst::render` as one new parameter. Unlike `--toc`, this
flag is not `--format`-specific, since decision 54 already gave both
backends a captioned artifact's own real figure, so both backends get
a real placement toggle here too, not just Typst.

<!-- dankg:depends target=../architecture.md#decision-44-weave-pdf-via-typst quote="a plain-text prepend rather than a Typst" -->

## What's new

### Decision 56: `--figures-inside`/`--figures-outside`, with a per-block `figure=` override

`dankg weave` gains a boolean pair, `--figures-inside`/`--figures-outside`,
parsed in `cli::weave` exactly the way `--toc`/`--no-toc` already are,
defaulting to `inside` -- decision 54's original placement -- when
neither is given. The resolved value threads down as a new parameter
on both `render::html::render` and `render::typst::render`, read at
the one place each backend already builds a captioned artifact's
figure. A new `figure` attribute, added to `KNOWN_ATTRS`, lets one
block override that document-wide default for its own artifact alone:
`figure=inside`/`figure=outside`, read through
`InfoString::figure_outside() -> Option<bool>`, resolved per block as
`info.figure_outside().unwrap_or(document_default)`. `figure=` on a
block with no captioned artifact at all is inert, the same
"meaningless here, silently ignored" stance `weave=output-hidden`
already takes on an unpaired block.

In the Typst backend, `inside` means splicing the already-built
`figure` buffer into `body` before `eval_pair` formats the
`#block(...)` call -- decision 54's original single-call shape,
restored exactly, not reconstructed by a template. `outside` keeps
decision 55's own placement: the figure appended as a sibling after
the block closes. In the HTML backend, the same split now applies
that previously did not exist: `eval_pair` builds the nested
`<figure class="table-figure">`/`<figure class="image-figure">` into
its own buffer, then either splices it before `</figure>` closes the
pair's own outer figure (inside, decision 54's original nesting) or
appends it after that closing tag (outside, a real DOM-level sibling
for the first time -- previously only a stylesheet's own `order`
trick could fake this visually, never move it out of the DOM).

**Rationale:** A `#show`/`#set` rule, or a CSS rule, can restyle an
element it matches. Neither can undo *structure* a renderer already
committed to -- a stroke already drawn around a body, a `<figure>`
already nested three levels deep. Placement is structure, not style,
so it belongs in the renderer's own hands, exposed as a real toggle,
not left for a template to reverse-engineer with a spacing constant
borrowed from the renderer's own defaults. A CLI flag for the
document's own default, with a per-block attribute free to override
it, mirrors exactly the layering `weave=hidden` and its own per-block
siblings already established: a global stance, overridable one block
at a time, never a config value fighting a hand-authored one.

<!-- dankg:depends target=../architecture.md#decision-54-a-produced-artifact-renders-as-a-real-captioned-figure quote="An artifact with no caption at all renders unwrapped" -->

## What this explicitly does not do

- Remove decision 55's own text from `architecture.md`. It stays,
  amended with a pointer to decision 56: the *mechanism* it
  introduced (a captioned figure that can render outside the pair's
  own block) is exactly what decision 56 keeps, gated behind an
  explicit choice instead of being the unconditional default.
- Add a third placement value, or a way to place a figure somewhere
  other than immediately before or after the pair's own block/figure.
  `inside`/`outside` covers every case decision 54/55 already do; a
  reader wanting a figure somewhere else in the document has
  `weave=output-hidden` plus a hand-placed reference already, a
  separate mechanism this does not touch.
- Change `[weave.pdf] template`'s own role. A template still fully
  controls a figure's own look -- caption position, stroke color,
  spacing beyond what "inside" already gives it for free. It no
  longer has to fake *placement* itself; styling stays exactly where
  it already was.
- Retrofit `dankg_weave_example`'s own `template.typ` here. Once this
  ships, its hand-tuned `above: -1.2em`/color-matching rule becomes
  dead weight -- `history_csv` goes back to `figure=inside` (or no
  attribute at all, since inside is the default) with no template
  rule required, and `history_png` gets an explicit `figure=outside`
  to keep demonstrating the other state. That edit happens in the
  example project itself, once the flag and attribute exist, not as
  part of this plan.

## Critical files

- `src/md/mod.md` / `md/mod.rs` -- `KNOWN_ATTRS` gains `figure`;
  `InfoString` gains `figure_outside() -> Option<bool>`.
- `src/cli.md` / `cli.rs` -- `Command::Weave` gains a
  `figures_outside: bool` field; `weave()`'s own arg loop parses
  `--figures-inside`/`--figures-outside` the way it already parses
  `--toc`/`--no-toc`, with no `--format`-specific rejection (both
  backends support this).
- `src/main.md` / `main.rs` -- `weave_cmd` passes the new field through
  to `weave::run`.
- `src/weave.md` / `weave.rs` -- `run` gains a `figures_outside: bool`
  parameter, threaded into `render_html`/`render_pdf`, then into
  `weave_html::render`/`typst::render`.
- `src/render/typst.md` / `typst.rs` -- `eval_pair` resolves
  `source_info.figure_outside().unwrap_or(figures_outside)` and
  splices the already-built `figure` buffer into `body` (inside) or
  appends it after the formatted block (outside, decision 55's own
  path, unchanged).
- `src/render/weave_html.md` / `weave_html.rs` -- `eval_pair` gains
  the identical split: a separate buffer for the nested
  `table-figure`/`image-figure`, spliced before or after the pair's
  own closing `</figure>` depending on the same resolved value.
- `architecture.md` -- add decision 56; amend decision 55 with a
  pointer to it.
- `README.md` -- document `--figures-inside`/`--figures-outside` and
  `figure=` alongside the existing `caption=`/`weave=` documentation.

## Verification

1. Edit loop per CLAUDE.md, then `dankg fmt --check` and `dankg check .`
   over the whole corpus.
2. Unit tests (existing style, both backends get a matching pair):
   - `cli::weave` parses `--figures-inside`/`--figures-outside` into
     the right field; passing neither defaults to inside; passing
     both is the same last-one-wins shape `--toc`/`--no-toc` already
     has if a caller passes both (whichever `match` arm runs last
     wins, since there is no explicit conflict check either way
     today).
   - A captioned table/image with no `figure=` and no CLI flag
     renders inside the pair's own block (Typst) / inside the pair's
     own outer `<figure>` (HTML) -- decision 54's original shape,
     confirmed by structural assertions on the output.
   - `--figures-outside` (no block attribute) renders every captioned
     artifact outside, matching decision 55's own existing tests
     unchanged in spirit.
   - `figure=outside` on one block, with the document default left at
     `inside`, renders only that block's own artifact outside; a
     second block with no attribute in the same document still
     renders inside.
   - `figure=inside` on one block overrides a document-wide
     `--figures-outside` back to inside, for that block alone.
   - `figure=` on an unpaired block, or a paired block with no
     captioned artifact, changes nothing.
3. A manual smoke test against `dankg_weave_example`, once this lands:
   drop `template.typ`'s own spacing/color-matching rule, weave with
   `--figures-outside` for `history_png`'s own default and
   `figure=inside` (or nothing) for `history_csv`, and confirm the PDF
   and HTML both show the same result the hand-tuned template used to
   fake -- one seamless box for the table, one detached figure for the
   image -- with no template rule doing either job.
