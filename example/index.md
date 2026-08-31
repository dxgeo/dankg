---
title: Knowledge Base Index
tags: [index, meta]
---
# Knowledge Base Index

This is the entry point of the example corpus. It links out to [[DanKG]],
to the [reading list](notes/reading-list.md) by file (landing on its first
heading), and to a specific day in the [daily log](notes/daily-log.md#2026-08-28).

An external reference is recorded on the node but never becomes a graph
node itself: [DanKG on GitHub](https://github.com/example/dankg).

This one points at a file `.dankgignore` deliberately excludes from the
corpus, so it renders as an unresolved placeholder with a warning on
stderr: [draft notes](scratch/draft.md).

## About this graph

Nodes are headings; links between headings become edges. [[Garden]] links
here in return, so that pair of edges renders as one reciprocated line
instead of two arrows. This paragraph also links to its own heading,
[About this graph](#about-this-graph), which the layout draws as a loop
rather than dropping.
