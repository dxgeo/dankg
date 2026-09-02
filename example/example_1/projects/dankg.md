---
title: DanKG
tags: [project, rust]
---
# DanKG

Described in full back at the [index](../index.md#about-this-graph). See
[[Garden]] for how this fits into the wider notebook.

## Architecture

Layout runs in four phases; see the reading list's
[graph drawing](../notes/reading-list.md#graph-drawing) entry for the
background reading.

### Evaluation model

Blocks form a dependency DAG via `name` and `deps` in the fence info
string. `dankg eval` (milestone 7) will run `setup` before `hello`,
concatenated into one process:

```python name=setup
count = 42
```

```python name=hello deps=setup
print(f"hello, corpus! count={count}")
```

## Open threads

Nothing here yet -- linked from the daily log once something needs tracking.
