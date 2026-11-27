# A CLI an agent can read

## Context

The main readers of dankg's output are now agents and scripts, not a
person at a terminal. That settles a question the GUI plan left open.
`plan-gui-frontend.md` built five views. A reader found two of them
useful: blast radius and the file graph. Both answer a question rather
than draw a picture. An agent asks the same questions. It asks
them from a shell.

So this plan stops work on the GUI and the TUI, and puts that effort
into the CLI instead. The GUI plan keeps its findings and is not
pursued. The TUI stays as it ships, frozen. Neither is deleted by this
plan.

The README already promises this reader:

<!-- dankg:depends target=../README.md#dankg quote="Every input and output is a file an editor" -->

Every input and output is a file an editor or an LLM can read and
write. That holds for the corpus. It does not yet hold for what the
commands report about it.

## What the review established

**Only `graph` speaks JSON.** Decision 13 made JSON first-class from
day one.

<!-- dankg:depends target=../architecture.md#decision-13-cli-shape quote="JSON is a first-class, testable surface from day one." -->

That decision only ever reached `graph`. `check` and `eval --list`
refuse a `--format` flag outright. `check` also writes its whole
report to stderr as prose, one line per finding.

<!-- dankg:depends target=../src/main.md#check_cmd quote="stale: {rel_path} `{}`" -->

A script reading `check` today gets one usable fact, the exit code.

**The exit code cannot tell a dirty corpus from a failed run.** `check`
exits 1 when it finds a stale result. It also exits 1 when the path it
was given does not exist.

<!-- dankg:depends target=../src/main.md#module_doc quote="Ok(false) => ExitCode::from(1)," -->

```sh name=cli-plan-exit-codes
# spawned in this file's own directory, per decision 49
d=../target/release/dankg
$d check ../tests/data/corpus >/dev/null 2>&1; echo "dirty corpus: $?"
$d check no-such-dir >/dev/null 2>&1; echo "missing path: $?"
$d check .. --format json >/dev/null 2>&1; echo "check --format json: $?"
$d eval .. --list --format json >/dev/null 2>&1; echo "eval --list --format json: $?"
```

<!-- dankg:result name=cli-plan-exit-codes hash=ea6609fe429aaf18 -->

```
dirty corpus: 1
missing path: 1
check --format json: 2
eval --list --format json: 2
```

A CI step can live with that. An agent cannot. An agent that gets 1
back has to read stderr to learn whether it should fix the corpus or
fix its own command line.

**Every question goes through the whole graph.** "What depends on this
section" has no command. The answer today is a `jq` filter over the
full `graph --format json` dump. `CLAUDE.md` carries two of those
filters for exactly this question.

```sh name=cli-plan-dump-size
# spawned in this file's own directory, per decision 49
../target/release/dankg graph .. --format json 2>/dev/null \
  | python3 -c '
import sys, json
text = sys.stdin.read()
g = json.loads(text)
print("%d nodes, %d edges, %d KB of JSON" % (
    len(g["nodes"]), len(g["edges"]), len(text) // 1024))
'
```

<!-- dankg:result name=cli-plan-dump-size hash=7df744f159add183 -->

```
1053 nodes, 1316 edges, 502 KB of JSON
```

An agent pays for that dump in context, once per question. It also
writes its own filter each time. A filter written fresh is a filter
that can be wrong.

**The obvious filter is wrong.** The `jq` recipe asks which `depends`
edges point at a node. That misses a case. A heading's section runs to
the next heading of the same or higher level. It therefore contains
its subsections.

<!-- dankg:depends target=../src/graph/model.md#node_and_edge quote="Last line belonging to this node: for a heading, before the next" -->

`dankg check` verifies a quote against that whole span. A marker that
targets a parent can therefore quote words that sit under one of its
subsections. Editing the subsection breaks the marker. The recipe,
asked about the subsection, never names it.

```sh name=cli-plan-ancestor-quotes
# spawned in this file's own directory, per decision 49
cd .. && ./target/release/dankg graph . --format json 2>/dev/null \
  | python3 -c '
import sys, json
g = json.load(sys.stdin)
nodes = {n["id"]: n for n in g["nodes"]}
kids = {}
for n in g["nodes"]:
    if n.get("parent") and n["kind"] == "heading":
        kids.setdefault(n["parent"], []).append(n["line"])
total = under = 0
for e in g["edges"]:
    t = nodes.get(e["to"])
    if e["kind"] != "depends" or not t or not t["resolved"]:
        continue
    if t["kind"] not in ("heading", "file"):
        continue
    total += 1
    lines = open(t["file"]).read().split("\n")
    end = min(kids.get(t["id"], [t["end_line"] + 1])) - 1
    own = " ".join(" ".join(lines[t["line"] - 1:end]).split())
    if " ".join(e["quote"].split()) not in own:
        under += 1
print("%d of %d markers on a heading quote text under a subsection" % (
    under, total))
'
```

<!-- dankg:result name=cli-plan-ancestor-quotes hash=59cefc767afe1aa2 -->

```
4 of 260 markers on a heading quote text under a subsection
```

The count is small. It is not zero. An agent editing a subsection
has no way to know it is in that set.

**A one-shot command is fast enough.** `backend-protocol-plan.md`
proposed `dankg serve` partly to avoid this cost:

<!-- dankg:depends target=backend-protocol-plan.md#context quote="re-parsing the whole corpus on every" -->

`graph::cache` already skips the re-parse for every unchanged file. A
warm load of this whole corpus is quick enough that a persistent
process buys an agent nothing.

```sh name=cli-plan-warm-load
# spawned in this file's own directory, per decision 49
d=../target/release/dankg
$d graph .. --format json >/dev/null 2>&1
python3 -c '
import subprocess, time
start = time.monotonic()
for _ in range(10):
    subprocess.run(["../target/release/dankg", "graph", "..",
                    "--format", "json"], capture_output=True)
ms = (time.monotonic() - start) * 100
print("warm graph load: %s" % ("under 100 ms" if ms < 100 else "%d ms" % ms))
'
```

<!-- dankg:result name=cli-plan-warm-load hash=ed2dbff933fd3eed -->

```
warm graph load: under 100 ms
```

**The query layer already exists.** `graph::query::links_for` returns
every edge touching one node, partitioned by kind and by side. It was
written for the reader this plan is about:

<!-- dankg:depends target=../src/graph/query.md#links_for quote="A reader about to rewrite a section wants to know who quoted it" -->

The TUI's cross-reference panel is its only caller. The new commands
below are its second.

## What's new

### Exit codes mean one thing each

Every command exits 0 on success. A gate exits 1 when it ran and found
something: `check` with a gating finding, `fmt --check` with a file out
of normal form. Every command exits 2 when it could not run. That
covers a bad argument, which already exits 2, and a missing path, a
read error, or a failed config parse, which exit 1 today.

This is the convention `diff` and `grep` already use. A script then
needs no stderr to branch. Exit 1 means "the corpus needs work". Exit
2 means "the command needs fixing".

### `--format json` on every command that reports

`check`, `eval --list` and `index` each gain `--format text|json`.
Text stays the default for these three. A CI log is still read by a
person.

`check`'s findings are collected into a list before any of them is
printed. The text renderer then prints today's lines, byte for byte. The
JSON renderer prints the same list as records. A finding carries:

- `kind`: `stale-result`, `stale-prose`, `stale-filedep`, `unresolved`,
  `title-collision`, `bad-tag`, or `wide-icon`
- `gating`: whether it fails the exit code
- `file` and `line`: where the finding sits
- `node`: the id of the heading or block it sits under
- `target`: the node id or path it refers to, when it has one
- `message`: the text line `check` prints today

A `stale-result` finding gains a line number, which the text form does
not carry today. A `stale-prose` finding also carries its `quote`. An
agent can fix a stale quote from that record alone, with no second
lookup.

The output ends with a summary object holding the counts `check`
already prints on its closing lines. The indexing diagnostics that
print before the report become findings too, of kind `diagnostic`.

`eval --list --format json` prints one record per block: name,
language, file, line, the node it sits under, and whether its language
is configured. `index --format json` prints the root and the file list.
That last one replaces the `jq -r '[.nodes[].file] | unique[]'` step
the `fmt` gate uses today.

### A reference names a node three ways

The new commands take a reference to a node. One resolver accepts all
three forms a caller is likely to hold:

- a node id, as `graph --format json` prints it
- a marker-style path, `file.md#slug`, resolved by
  `depends::resolve_target` from the root
- a `file.md:line`, resolved to the innermost node whose span holds
  that line

The third form is the one an agent holds after a `grep`. A reference
that matches nothing exits 2. A caller holding only a section's name
uses `find`, below, which may return more than one node.

### `dankg node`: one node and its edges

`dankg node <ref>...` prints each node with every edge touching it, in
both directions and of every kind. It wraps `links_for` and adds the
two relations that function leaves out: the containment parent and
children, and `eval-chain`. Each `depends` edge carries its quote.

### `dankg impact`: what can go stale if this node changes

`dankg impact <ref>` answers the question an agent asks before an
edit. The answer is every place that `dankg check` could report once
this node's text changes:

- each `depends` marker targeting this node, or any of its ancestors,
  whose quote sits inside this node's own span
- each `link` targeting this node, since renaming its heading breaks
  the link
- for a block, every block downstream of it through `eval-chain` and
  `reads`, followed transitively

The ancestor walk is the case the review found the `jq` recipe misses.
The quote filter keeps it honest. An ancestor's marker is listed only
when its quote actually lies under the node being asked about.

Prose dependencies are not followed transitively. A quote of a quote
breaks only when the middle section's own text changes. That is a
second edit, and a second question. Eval blocks are followed
transitively, because a re-run result is new input for everything
downstream of it.

### `dankg find`: a title to a node

`dankg find <text>` lists every node whose title or slug contains the
text, with id, file and line. It is the way into the two commands
above for a caller that knows a section's name but not its id.

### JSON is a contract, pinned by tests

`node`, `impact` and `find` print JSON by default, the same default
`graph` already has. `--format text` gives a person a readable form.

Each JSON output carries its own `version`, starting at 1. `graph`'s
stays at 3. One shared number would bump every output whenever any one
of them changed.

<!-- dankg:depends target=../src/render/json.md#module_doc quote="Bumped whenever the shape changes in a way a consumer would notice." -->

Each output gets a golden file under `tests/data`, as `graph` already
has. Each also gets a shape test. It walks the output's key paths and
compares them to a pinned list. A shape change fails that test, and
the failure message says to bump the version. The README gains one
section documenting every output's fields.

## Why not the alternatives

**Keep the `jq` recipes.** They work for a careful reader who knows the
three spellings of a target and the ancestor case. Each new caller has
to relearn both. A command learns them once.

**Build `dankg serve` first.** A persistent process suits an editor
plugin. An agent runs one shell command and reads its output. It has no
client to hold a connection open. The warm-load measurement above
removes the cost argument. The query functions land in `graph::query`,
so `serve` can wrap them later unchanged.

**One `dankg query` command with a small query language.** That is a
grammar to design, document and version. The three verbs answer the
questions the GUI prototype measured as useful. A fourth verb is
cheaper to add than a language is to get right.

**JSON as the default for `check`.** CI reads `check`. A person
reads the CI log. Text stays the default there. The new query verbs
have no such reader. They default to JSON.

## Build order

1. Exit codes. It is the smallest change. Every later item's tests
   assert on them.

2. `index --format json`, then the `fmt` gate moved onto it. The
   smallest new JSON output, with a caller on day one.

3. `check`'s findings collected into a list, with the text renderer
   pinned byte for byte against today's output. Then `--format json`
   over the same list.

4. `eval --list --format json`.

5. The reference resolver, with `dankg node` as its first caller.

6. `dankg impact`, with a test built from one of the real
   ancestor-quote cases above.

7. `dankg find`.

8. The README's output section, the version bump to 0.11.0, and the
   `CLAUDE.md` recipes replaced by the commands that answer them.

## What this explicitly does not do

- Build the GUI, or extend the TUI. `plan-gui-frontend.md` is not
  pursued. Both TUI plans already shipped, and the TUI stays frozen as
  they left it.
- Change `graph --format json`. Its shape and its version stay as they
  ship.
- Add `dankg serve`. `backend-protocol-plan.md` still owns it.
- Print errors as JSON. An error still goes to stderr as text, with
  exit 2. A caller that wants the reason reads stderr.
- Change what `check` gates on. A finding that is advisory today stays
  advisory, and says so in its `gating` field.
- Run anything. Every new command only reads, under decision 9.

## Open questions

1. Whether `impact` should list `link` edges from an ancestor, the
   way it lists ancestor quotes. A link to a parent does not break when
   a subsection changes. It is left out unless a real case argues
   otherwise.

2. Whether `eval --list --format json` should report staleness.
   `check` already computes it. `eval --if-stale` already acts on
   it. A third place risks a third answer.

3. Whether `find` should match on body text as well as titles. That
   would make it a search tool, which `grep` already is.

## Critical files

- `src/cli.md`: the new `--format` flags and the three new verbs.
- `src/main.md`: `check_cmd` split into collect and render, the exit
  code mapping, and `index_report`'s JSON form.
- `src/eval/session.md`: `list`'s JSON form.
- `src/graph/query.md`: the reference resolver, `impact`, and `find`,
  beside `links_for`.
- `src/depends.md`: `resolve_target` and `section_text`, reused for
  the ancestor walk.
- `src/render/json.md`: the encoder the new outputs reuse.
- `README.md` and `architecture.md`: the output section, and a decision
  each for the exit codes and the JSON contract.

## Verification

- `cargo test`, with a binary test per exit code and a golden file and
  shape test per JSON output.
- `check`'s text output diffed against today's binary on this corpus
  and on every fixture corpus under `tests/data`. It must be identical.
- `impact` run on each ancestor-quote case above, listing
  the marker the `jq` recipe misses.
- One of the `agent_tests/` pilots re-run with the new commands in
  place of the recipes, compared on command count and output size.
