---
title: Weaving a Document
author: DanKG
date: 2026-09-26
cover: true
bibliography: refs.yml
---
# Weaving a Document

Every `dankg weave` feature is exercised somewhere below. The same
file renders both ways:

```sh
dankg weave report.md --format html -o report.html
dankg weave report.md --format pdf
```

Weave reads one file. It never walks a corpus. A link naming another
file therefore stays plain text rather than resolving to something
that is not there.

# What the frontmatter does

The `title`, `author` and `date` above become a real cover page in the
PDF. `date` is parsed into a Typst date rather than echoed as a
string. The template is what controls how it reads. `cover: true` is what
absorbs the repeated `# Weaving a Document` heading: the title is
already on the cover. Printing it twice helps nobody. Setting
`cover: false` drops the cover page instead and keeps the heading.

`bibliography: refs.yml` names the file every citation below resolves
against. A `hayagriva` fence in the document body is the other way to
declare one. The two merge when both are present.

# Prose, tables and data

Ordinary markdown renders as you would expect: *emphasis*, **strong**,
`a code span`, and a [link to the Typst site](https://typst.app).
Literate programming is Knuth's idea [@knuth1984]. The case for
showing data rather than describing it is Tufte's [@tufte2001].
@codd1970 argues the relational case separately.

A bare `@` in prose parses as a citation key. A handle meant
literally needs escaping: write \@dankg to get \@dankg.

A GFM pipe table is real structure, with per-column alignment:

| Region | Quarter | Revenue |
| :-- | :-: | --: |
| North | Q1 | 1200 |
| South | Q1 | 980 |
| North | Q2 | 1440 |

A fenced block tagged `csv`, `tsv` or `json` is a second table source.
The tag alone decides; content is never sniffed:

```csv
region,quarter,revenue
North,Q3,1610
South,Q3,1050
```

***

# Running code, and recording what it printed

A named block followed by its own recorded result renders as one
paired unit. `dankg eval report.md --all` is what writes those
markers; weave never runs anything itself.

```sh name=greeting
echo "hello from a recorded run"
```

<!-- dankg:result name=greeting hash=27cd2e5a4b243d69 -->

```
hello from a recorded run
```

A block that fails keeps its output and says so, rather than hiding
the failure:

```sh name=failing
echo "this went wrong" >&2
exit 3
```

<!-- dankg:result name=failing hash=ccd8d9d10af9bf47 failed -->

```
```

# Figures a reader can point at

A block that declares `produces=file:` and carries a caption renders
as a real figure. Its label is the block's own `name=`, so
[[#quarterly]] needs no extra attribute to point at the table below.
A labelled reference says something else instead:
[[#quarterly|the quarterly breakdown]].

```sh name=quarterly produces=file:data/quarterly.csv caption="Revenue by region and quarter"
mkdir -p data
printf 'region,quarter,revenue\n' > data/quarterly.csv
printf 'North,Q1,1200\nSouth,Q1,980\n' >> data/quarterly.csv
printf 'North,Q2,1440\nSouth,Q2,1010\n' >> data/quarterly.csv
echo "wrote data/quarterly.csv"
```

<!-- dankg:result name=quarterly hash=8770a213ef93c5d2 -->

```
wrote data/quarterly.csv
```

Tables and images are numbered on separate counters, because Typst
counts them separately and the two backends have to agree. The chart
below is therefore Figure 1 while the table above is Table 1. See
[[#trend]] for the chart, or [[#trend|the revenue trend]] to say it in
your own words.

`artifact=` renames the target, for when a path is a filesystem detail
the prose should not have to quote. The chart's block is named
`chart`. It writes `figures/trend.svg`, so its slug would be `trend`
either way. The attribute is declared here to show it.

```sh name=chart produces=file:figures/trend.svg caption="Revenue trend across three quarters" figure=outside artifact=trend
mkdir -p figures
cat > figures/trend.svg <<'SVG'
<svg xmlns="http://www.w3.org/2000/svg" width="320" height="140" viewBox="0 0 320 140">
  <polyline fill="none" stroke="#3c6df0" stroke-width="3" points="20,110 100,84 180,56 260,24"/>
  <polyline fill="none" stroke="#b0b0b0" stroke-width="2" stroke-dasharray="4 3" points="20,118 100,112 180,104 260,98"/>
  <line x1="20" y1="125" x2="300" y2="125" stroke="#3c3c3c" stroke-width="1"/>
  <text x="20" y="138" font-family="Helvetica" font-size="9">Q1</text>
  <text x="176" y="138" font-family="Helvetica" font-size="9">Q3</text>
</svg>
SVG
echo "wrote figures/trend.svg"
```

<!-- dankg:result name=chart hash=1dbf7712e3e9ba05 -->

```
wrote figures/trend.svg
```

That block also carries `figure=outside`. Its figure stands apart from
the pair's own box instead of nesting inside it.
`--figures-inside` and `--figures-outside` set the document-wide
default. A block's own `figure=` overrides it.

The markdown link form resolves the same way a wikilink does:
[the table again](#quarterly) and [the chart again](#trend) both land
on their figures.

# Showing one half of a pair

## Keeping only the artifact

`weave=source-hidden` keeps the artifact and drops everything else --
no source, no captured output, no provenance line -- for a walkthrough
that shows a result without the code behind it.

```sh name=headline produces=file:data/headline.csv weave=source-hidden caption="Headline numbers"
mkdir -p data
printf 'metric,value\nrevenue,4630\ngrowth,18%%\n' > data/headline.csv
echo "wrote data/headline.csv"
```

<!-- dankg:result name=headline hash=0aab248b841fb68b -->

```
wrote data/headline.csv
```

## Keeping only the source

`weave=output-hidden` is the mirror image. It keeps the source and
drops that whole second half, artifact included, for a snippet worth
showing without spoiling what it produces.

```sh name=spoiler weave=output-hidden
echo "the answer is 42"
```

<!-- dankg:result name=spoiler hash=56e191227ed16928 -->

```
the answer is 42
```

`weave=hidden` drops both halves together. The block below renders
nothing at all, which is why nothing appears between this paragraph
and the next heading.

```sh name=setup weave=hidden
echo "scaffolding nobody needs to read"
```

<!-- dankg:result name=setup hash=eaa4331689ff42a2 -->

```
scaffolding nobody needs to read
```

# Pointing at a heading

A heading resolves by its own slug, the anchor its table-of-contents
link already uses. [[#what-the-frontmatter-does]] points at the
frontmatter section above.

The two backends deliberately differ on one point here. A bare heading
reference reads as a section number in the PDF and as the heading's
own title in HTML, because HTML numbers no heading. The PDF needs
`#set heading(numbering: "1.")` in a `[weave.pdf] template` before a
bare one will compile at all, which `template.typ` here supplies.
Writing your own label makes the two identical:
[[#what-the-frontmatter-does|the frontmatter section]].

A wikilink naming another file is left alone, because there is no
corpus to resolve it against: [[SomeOtherFile]] stays plain text.

# What fails the weave

Three things stop the render rather than producing a page with a hole
in it. A reference that resolves to nothing fails, on the line it was
written. A citation key the bibliography does not carry fails the same
way. An artifact that cannot be read or copied fails too.

All three report every bad line in one run, and write no output file
of any kind.

# A gap worth knowing about

`dankg check` does not yet know what a figure slug is. The graph
builds a block's node from its `name=` alone. A produced artifact gets
no node of its own. The reference [[#trend|to the chart]]
therefore resolves in weave and is reported as a dead link by
`check`. Running `dankg check .` in this directory shows it, alongside
the `[[SomeOtherFile]]` above, which is unresolved on purpose.

Both halves of a figure slug are invisible to the graph: a declared
`artifact=`, and the path stem a figure takes when none is declared.
`plans/plan-label-resolution.md` is the plan that closes this.
