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
