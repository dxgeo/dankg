# TUI commands

Try `dankg tui example/example_1` from the repo root, then press `g`:
it runs the block below and shows what it printed on the status
line -- its first line of output, since the status line is one row.
Reopen this file afterward to see the block's full output, recorded
by the same `<!-- dankg:result -->` marker any other eval block gets.
Press `?` first to see it listed alongside the built-in keys.

Any top-level block carrying a `key=` attribute becomes a keybinding this
way, once `[tui] commands` (in `.dankg/config`) names the file it lives
in -- nothing is scanned unless a file is named explicitly.

```sh name=greet key=g
echo "hello from a custom dankg command"
```

<!-- dankg:result name=greet hash=6a2d08cce40405d3 -->

```
hello from a custom dankg command
```

Add `protocol=lines` to a block and its stdout stops being plain
text: three recognized prefixes become a small remote-control
protocol for the TUI itself instead. Press `c` to try it -- it jumps
the tree to [Garden](projects/garden.md)'s own `Todo` heading and sets
the status line to something other than its own raw output. It also
writes: `tag:` doesn't just remember `kind=task` for this session, it
inserts a real `<!-- dankg:tag -->` comment right above `## Todo` in
`garden.md` itself, the same "write it into the file" policy
`dankg:result` already follows for a captured result. Open
[Garden](projects/garden.md) afterward to see it -- it is still there
after quitting and reopening `dankg tui`, since it is no longer
something only this session remembers.

The marker names `kind=task` and `target=#todo` -- never an icon.
`☐` comes from `.dankg/config`'s own `[kind.task]` section instead --
set once there, not repeated on every `tag:` line that ever sets
`kind=task` again. Open the filter menu (`f`) too: `tag:task` now
shows up as its own entry, alongside the built-in four, and the tree
row itself carries that `☐` badge, read back from `[kind.task]` on
every load. Had this said `kind=tsak` instead, `dankg check` would
fail on it -- a `kind=` naming no `[kind.*]` section is caught the
same way a broken link is. `target=#todo` is its own check too: it is
what the marker is *for*, not just wherever it happens to sit, so
inserting a new heading between the marker and `## Todo` -- an
otherwise ordinary edit -- would not silently reattach it;
`dankg check` would report the mismatch instead. A block without
`protocol=lines` -- `greet`, above -- never gets scanned this way, so
nothing it prints by coincidence (some other tool's own `status: ...`
line, say) is ever misread as one of these.

```sh name=classify key=c protocol=lines
echo "select: projects/garden.md#todo"
echo "status: tagged the garden's own Todo as a task"
echo "tag: projects/garden.md#todo kind=task"
```

<!-- dankg:result name=classify hash=b4f7048a430caa43 -->

```
select: projects/garden.md#todo
status: tagged the garden's own Todo as a task
tag: projects/garden.md#todo kind=task
```

None of this needs a command at all: press `t` on any node to pick a
declared `[kind.*]` directly, or `n` to declare a new one on the spot
(a name, then an optional icon) -- the exact same marker gets
written, just without writing `classify` first.
