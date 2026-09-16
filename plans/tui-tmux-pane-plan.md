# TUI editor handoff: a tmux pane, not a context switch

## Context

Today `enter`'s editor handoff (*TUI editor*, `tui/editor.md`) is a
full context switch. `event_loop` drops raw mode, blocks on
`editor::open`, then re-enters raw mode and reloads once the editor
exits. The reader's terminal briefly belongs to the editor, not to
`dankg tui`, and nothing in the tree pane is visible until the editor
closes.

<!-- dankg:depends target=../src/tui/editor.md#open quote="blocks until it exits" -->

The ask this plan answers: when `dankg tui` is itself running inside
a tmux session, hand the same resolved editor command to a new,
side-by-side pane instead, so the tree stays on screen the whole
time. Opening someone's editor stays the one place this crate shells
out to a program it does not render itself (decision 17); this plan
only changes where that program's window ends up, never whether
`dankg` still defers to `[editor] command`/`$EDITOR`/`$VISUAL`
exactly as `resolve` already decides.

<!-- dankg:depends target=../architecture.md#decision-17-editor-integration quote="No GUI/TUI toolkit; the user's editor is the buffer, always." -->

## What this reuses

**The argv `resolve` already builds.** `editor::resolve` already
turns `config.editor()` and the environment into the exact argv
`open` would spawn. The tmux route needs no second resolution path --
it wraps that same argv in `tmux split-window`, rather than handing
it straight to `Command::new`.

<!-- dankg:depends target=../src/tui/editor.md#resolve quote="Pure and separately testable from `open`" -->

**The live-reload sweep, already landed on this branch.** Because the
sweep now notices a changed file on its own cadence and calls
`reload_preserving_expansion`, the tmux route needs no reload call
site of its own. The reader saves in the other pane; the sweep picks
it up exactly as it would for a file edited from anywhere else. The
blocking route still calls `reload_preserving_expansion` itself,
since there the TUI has no tick running while the editor owns the
screen.

<!-- dankg:depends target=../src/tui/app.md#live-reload-sweep quote="That one reload calls `reload_preserving_expansion`, not plain `reload`" -->

**No new dependency.** `tmux` is invoked the same way
`[editor] command` already is: `std::process::Command`, argv built by
hand, no crate.

## What's new

### Detection: `$TMUX`, no config

The tmux route activates whenever `dankg tui` is itself running
inside a tmux session -- `$TMUX` set in the environment -- the same
way the existing `$EDITOR`/`$VISUAL` fallback activates itself with
no separate opt-in. No new `.dankg/config` key. A reader who does not
want it can unset `$TMUX` for the session or just not run `dankg tui`
inside tmux.

### The wrapped argv

A new pure function alongside `resolve`, `tmux_argv`, prepends
`["tmux", "split-window", "-h", "-f"]` to whatever `resolve` already
returned. `-h` splits side-by-side: the tree pane stays visible to
the reader's left, the editor opens to the right. `-f` spans the new
pane across the whole window height regardless of whatever other
splits the reader already had in that window -- without it, the new
pane only matches the height of `dankg tui`'s own pane, which can
already be less than full height if the reader's own layout stacks
something else below it. Testable exactly like `resolve` -- no real
tmux process, just an assertion on the built argv.

### `open`'s new shape

`open` gains a branch: when `$TMUX` is set and `resolve` found an
editor, it spawns the wrapped argv through `Command::new("tmux")`
instead of `Command::new(&argv[0])`. `tmux split-window` returns as
soon as the pane exists, not when the editor inside it exits, so
there is no `ExitStatus` for the editor itself to report. `open`'s
return type has to say which of three things happened, not two:
ran and exited with a status, opened in a pane with nothing to wait
for, or nothing configured at all. `editor_status` picks the status
line message from that same three-way split.

### `app.rs`'s `Enter` arm

The tmux branch skips `raw.take()`/re-enter entirely: there is no
reason to drop raw mode for a pane this process never draws into. It
also does not call `reload_preserving_expansion` itself -- the sweep
already covers it, per *What this reuses* above. The non-tmux branch
is untouched: suspend, block, resume, reload, exactly as today.

### Phase 4: one pane, not one per `enter`

Phases 1 through 3 spawn a fresh pane on every `enter`, whether or
not the reader already has one open. For an editor that blocks in
the foreground for its whole session -- vim/nvim, `emacs -nw`, `nano`
\-- that fills the tmux window with one pane per node visited, most of
them dead weight once the reader has moved on. An editor with its own
client/server reuse (`code -r`, `emacsclient`, `subl`) mostly dodges
this already: its CLI hands off to a persistent background window and
exits, so the pane closes itself right behind it. Phase 4 is only for
the editors that do not.

`open` starts capturing the pane it creates instead of only waiting
on it: `tmux split-window -h -f -P -F '#{pane_id}'` prints the new
pane's id on its own stdout, in addition to creating it. That id has
to survive somewhere a reader can quit and reopen `dankg tui` without
losing it -- an in-memory `App` field would not: a fresh process has
no memory of the one before it, even though the tmux pane from that
earlier session might still be sitting right there. tmux itself is
the persistence layer instead: a window-scoped user option
(`@dankg_editor_pane`), set once `open` creates a pane and read back
by the next `open` call in the same tmux window, however many
`dankg tui` processes have come and gone in between. It disappears on
its own once the window itself closes -- the same "nothing left to go
stale" property a durable file on disk would not have had.

Reuse only fires when a reader has explicitly configured a template
for it -- a new key, `[editor] reuse`, with the same `{file}`/`{line}`
placeholders `[editor] command` already has, but aimed differently:
instead of naming argv to spawn, it names the keystrokes
`tmux send-keys` sends into the pane the tmux option already names.
For vim/nvim that template reads
`:tab drop {file}<CR>:{line}<CR>` --
`:tab drop` is vim's own built-in "switch to this file's tab if it is
already open, else open a new one." Pressing `enter` on a node in the
same file the reader is already editing should jump within that
existing tab; pressing it on a different file should open a new tab
rather than replacing the current one. Both fall out of `:tab drop`
on its own, with no branching on dankg's side to tell the two cases
apart.

`tmux split-window` already makes a freshly spawned pane active on
its own, with no help from `open`. Reuse gets no such help: `send-keys`
only types into a pane, it does not switch the client's attention
there. Without an explicit `tmux select-pane -t <pane>` right after,
the reader would stay looking at the tree with no visible sign
`enter` did anything -- found by a reader's own report, not designed
in up front.

Unconfigured, the tmux option is never consulted and every `enter`
spawns fresh, exactly as phases 1-3 left it. This has to stay opt-in.
`[editor] command`'s own trust boundary already lets a reader name an
arbitrary program to spawn, the same way `[lang.*] command` does, but
a wrong *spawn* command only fails to launch. A wrong *reuse* template
gets typed as literal keystrokes into whatever is actually running in
that pane -- for the wrong editor, or the right editor in the wrong
mode, that can mean corrupting a live, unsaved buffer rather than
merely erroring out. Guessing a default here from `[editor] command`'s
own value would guess wrong for every reader not running vim, so
there is no default at all, the same reasoning `pane_available` (*TUI
editor*, `tui/editor.md`) already follows for keeping a single check
in one place rather than two that could disagree.

<!-- dankg:depends target=../src/tui/editor.md#pane_available quote="so the two call sites cannot disagree." -->

Several things make the tracked pane stop being trustworthy, and all
of them fall back to spawning fresh exactly as phases 1-3 already do:
the reader closed the pane by hand, the editor inside it exited on
its own (which closes the pane outright, since `open` spawns the
editor as the pane's own direct process rather than inside a shell --
there is no bare shell left behind to worry about), the reader
repurposed the pane for something else entirely (`tmux respawn-pane`,
say), the tmux window itself was recreated and lost the option along
with everything else in it, or `[editor] reuse` was never configured
in the first place. Liveness checks both the pane id and its current
foreground command together
(`tmux list-panes -a -F '#{pane_id} #{pane_current_command}'`), not
the id alone -- a reader who quit the editor by some means that does
leave the pane running something else should not have `send-keys`
type literal keystrokes into whatever that is. Trusting the tracked
pane blindly and letting `send-keys` into a pane tmux no longer
recognizes surface as a visible error is avoided the same way --
falling back is free, so there is no reason to make the reader see
that failure at all.

### Phase 5: which side the pane opens on

`-h -f` is hardcoded: side-by-side, editor to the right, full window
height. A reader whose own tmux layout puts other panes to the right
already, or who simply prefers the editor below the tree, has no way
to change that. The fix is a new key, `[editor] split`, one of
`right` (the default, today's behavior), `left`, `above`, or `below`
\-- named after where the pane actually ends up, not tmux's own
`-h`/`-v` flags, since `-h` produces a side-by-side split, the
opposite of what "horizontal" suggests to most readers. This is
exactly the confusion decision 17's "no surprises" spirit argues
against exposing directly.

A validated, bounded set of four values, not a raw flag string. `open`
already appends its own `-P -F '#{pane_id}'` for pane-id capture
(phase 4); a reader-supplied raw flag string risks colliding with
that, where a bad `[editor] command` merely fails to launch. `right`
maps to `-h`, `left` to `-h -b`, `below` to `-v`, `above` to `-v -b`
\-- `-b` places the new pane before `target-pane` instead of after.
`-f` stays unconditional across all four: it already means "full
height with `-h`, full width with `-v`," so it composes with every
side without its own branch.

`config::editor_split` follows `tui_breadcrumb`'s own shape exactly:
`None` (unset) defaults to `right`; an unrecognized value warns and
falls back to `right` too, rather than refusing to launch the editor
at all over a typo in one config key. Reading it needs a `&mut Diags`
to warn through, so `open` gains a `diags` parameter -- the same
"warn, don't refuse" reasoning `config.rs`'s other validated enums
(`tui_breadcrumb`, `keymap`) already follow, unlike `[editor] command`/
`[editor] reuse`, which are free-form templates with nothing to
validate.

<!-- dankg:depends target=../architecture.md#decision-17-editor-integration quote="No GUI/TUI toolkit; the user's editor is the buffer, always." -->

## What this explicitly does not do

- No support for other multiplexers or terminal splits (screen,
  kitty, wezterm) -- tmux only, for now.
- No new `.dankg/config` key through phase 3 -- detection is `$TMUX`
  alone. Phase 4 adds one, `[editor] reuse`, off by default; phase 5
  adds a second, `[editor] split`, defaulting to today's behavior.
- No raw tmux flag pass-through for `[editor] split` -- a validated,
  bounded `right`/`left`/`above`/`below`, not a string a reader could
  make collide with the `-P -F` flags `open` already relies on for
  pane-id capture.
- No change to `[editor] command`'s own template syntax, or to
  `resolve`'s existing precedence between config and environment.
- No attempt to wait for the editor's own exit inside the new pane
  (say, via `tmux wait-for`). Fire-and-forget, matching how little
  the live-reload sweep needs to be told.
- No editor-specific RPC (`nvim --server`/`--remote`, `emacsclient`'s
  own protocol). Phase 4 drives an already-running pane through
  `tmux send-keys` instead, so dankg never needs to know or detect
  which editor is actually inside it.
- No default or inferred `[editor] reuse` template. A reader who
  wants reuse writes the keystrokes for their own editor; dankg never
  guesses one from `[editor] command`'s own value.
- No attempt to track more than one reused pane. `@dankg_editor_pane`
  is a single tmux option per window; a reader who wants two editors
  open at once already gets that today by not configuring reuse at
  all.
- No persistence beyond the tmux window's own lifetime. Surviving
  `dankg tui` itself quitting and reopening is the point; surviving
  the window closing is not -- a window-scoped tmux option is gone
  the moment the window is, matching what the reused pane would look
  like anyway by then.

## Open questions

- **Split direction/side.** Resolved by phase 5: `[editor] split`
  (`right`/`left`/`above`/`below`). `-f` (full window height/width)
  stays unconditional across all four -- no reader has asked for a
  partial size yet.
- **No `$TMUX`, but some other multiplexer in play.** Out of scope --
  no evidence yet anyone wants it.
- **Escaping for `send-keys`.** A `{file}` substituted into a
  keystroke template, rather than an argv element, needs its own
  escaping rules -- a path with a space is one `Command` argument
  today, but naively textual once it is keystrokes typed at a shell
  or an editor's command line. Needs resolving in phase 4's own
  design, not guessed at here.
- **A kill-and-replace fallback for readers who skip
  `[editor] reuse`.** Capping at one pane by killing the previous
  one, rather than leaving reuse to `send-keys`, was considered and
  set aside:
  killing a pane with unsaved edits in it destroys them with no
  prompt. Worth revisiting only if a reader asks for it explicitly,
  eyes open to that risk.

## Phased build order

1. `tmux_argv`, pure and tested, not wired into `open` yet.
2. `open`'s three-way return type and the `$TMUX`-detection branch,
   `editor_status` updated to match. Unit tests only -- no real tmux
   process needed for any of this.
3. Wire into `app.rs`'s `Enter` arm. Verify by hand inside a real
   tmux session -- this cannot run in CI, which has no controlling
   tty and no tmux server either.
4. Capture the new pane's id (`split-window -P -F`), not consulted by
   anything yet -- every `enter` still spawns fresh, the same shape
   phase 1 had before it was wired into `open`.
5. `[editor] reuse`, the liveness check, and the `send-keys` branch in
   the `Enter` arm, wired end to end. The liveness check's own
   parsing (does a remembered id, paired with the right command,
   appear in `tmux list-panes`' output) is pure and unit-tested the
   same way `tmux_argv` is; invoking the real `tmux` commands stays
   thin and untested, the same split `open` already has. The captured
   pane id is persisted in the `@dankg_editor_pane` tmux window
   option, not an `App` field: an `App` field was tried first, then
   replaced once it became clear it could not survive the reader
   quitting and reopening `dankg tui` itself, only the pane surviving
   inside one running process.
6. Hand verification in a real tmux session, walking the whole
   reuse/fallback matrix: open node A, open node B (same pane
   retargets, no new one appears), open node A again (jumps within
   the existing tab, no duplicate), close the pane by hand and open
   node C (falls back to spawning fresh), quit and reopen `dankg tui`
   entirely and open node A again (still reuses the pane from the
   previous process, not a new one) -- and a second real-tmux check,
   independent of reuse: split the tmux window itself before `enter`
   is ever pressed, confirm the new editor pane still spans the whole
   window height (`-f`) rather than only `dankg tui`'s own, now
   partial, pane height.
7. `[editor] split` (`SplitSide`, `split_flags`, `editor_split`),
   wired into `tmux_argv`; `open` gains a `diags` parameter so
   `editor_split` has somewhere to warn. `split_flags` is pure and
   unit-tested, one assertion per value; hand verification covers all
   four in a real tmux session, confirming the pane actually lands on
   the named side.

## Critical files

- `src/tui/editor.md` -- `resolve`, `open`, `tests`. Phase 4 adds the
  pane-liveness check, the `send-keys` template resolution, the
  `@dankg_editor_pane` tmux window option that persists the tracked
  pane across `dankg tui` processes, and `select_pane` (switches tmux
  focus to a reused pane, mirroring what a freshly split one already
  gets) -- all self-contained here, with no new `App` state.
- `src/tui/app.md` -- the `Enter` arm in `event_loop`, `editor_status`.
  Also `ESC_TIMEOUT_MS`/`TMUX_ESC_TIMEOUT_MS`: a prerequisite fix, not
  part of the pane route itself, but found by testing it -- tmux can
  split an arrow key's `ESC` from its `[ <letter>` by more than the
  plain-terminal timeout tolerated. See *An escape sequence tmux
  delivered in two pieces*, `architecture.md`.
- `src/tui/input.md` -- `read_key` takes the Esc-disambiguation
  ceiling as a caller-supplied argument now, not a `const` it bakes in
  itself, so `event_loop` can pick a larger one inside tmux.
- `src/config.md` -- `[editor] reuse` (phase 4) and `[editor] split`
  (phase 5), alongside `[editor] command` in `KNOWN`; `SplitSide` and
  `editor_split`, the same validate-and-warn shape `tui_breadcrumb`
  already has.
- `architecture.md` -- a new decision documenting the pane route,
  added once phase 3 lands, not before.

## Verification

- Unit tests for `tmux_argv` and `open`'s three-way branch, same
  shape as today's `resolve` tests.
- Hand verification inside a real tmux session: launch `dankg tui`,
  press `enter` on a node, confirm a new pane opens to the right with
  the editor at the right file, edit and save, confirm the tree
  updates via the live-reload sweep with no keypress.
- Phase 4: unit tests for the pane-liveness parsing, same shape as
  `tmux_argv`'s own tests; hand verification of the full reuse/
  fallback matrix from phase 4's own build-order step, above,
  including the cross-process reuse (quit and reopen `dankg tui`) and
  full-window-height (`-f`, against a window already split before
  `dankg tui` even started) cases specifically. Also hand-verified:
  switching away from the editor pane and back to `dankg tui`, then
  triggering reuse again, confirms `select_pane` actually returns
  tmux's focus to the editor pane rather than leaving the reader on
  the tree.
- The `ESC_TIMEOUT_MS`/`TMUX_ESC_TIMEOUT_MS` fix has its own
  deterministic verification, independent of tmux's own timing: a pty
  harness (`pty.fork`, per this repo's own real-terminal testing
  pattern) that writes a `Down` arrow's `ESC` byte, sleeps past the
  plain-terminal ceiling, then writes `[B`, with `$TMUX` set so
  `pane_available` picks the tmux ceiling. Confirms the tree selection
  moves and the filter-picker overlay stays open (not dismissed) under
  that delay; a control run with `$TMUX` unset reproduces the original
  bug against the very same delayed bytes, proving the harness
  actually discriminates rather than passing regardless.
- Phase 5: unit tests for `split_flags`, one per `SplitSide` value,
  same shape as `tmux_argv`'s own tests; `editor_split`'s own tests
  mirror `tui_breadcrumb`'s (reads back, warns and falls back to
  `right` on an unrecognized value). Hand verification in a real tmux
  session for all four: `right`/`left` land side by side with the
  editor on the named side, `below`/`above` stack instead, all still
  spanning the full window height or width per `-f`.
- `cargo test`, `dankg fmt --check`, `dankg check .`.
