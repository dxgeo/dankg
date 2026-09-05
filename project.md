---
title: DanKG Project
author: Daniel J. Okuniewicz
---
This document records *what* DanKG is and *why*. [Architecture](architecture.md)
records *how* it is built. Where the two disagree, this file wins.
architecture.md is wrong.

# Overview

DanKG (Dan's Knowledge Grapher) is a vanilla markdown knowledge graphing
tool. It depends on nothing else.

## Key Features

1. Lightweight
2. Plaintext-driven (no fancy front-end needed)
3. Agent-compatible
   - Example: any LLM can run it, because it only works with plaintext files
   - [Agent navigation pilot](agent_tests/pilot.md) tests a sharper version
     of this claim: does the literate form itself help an LLM navigate,
     not just run
4. Compatible with standard markdown
5. Can evaluate inline code
6. Can visualize the graph interactively
7. Manages literate databases

## Constraints

1. Pure Rust
2. Dependency-free, built from scratch

## General Functionality

You write a markdown file and give it a title, a date, and an author.
DanKG can generate a knowledge graph of all the content in that file and
in any linked files. Example: you link to another top-level heading in
the same document. Running DanKG on the file renders a graph from that
heading to the next.

A link connects both nodes automatically. DanKG renders it as
one-directional unless there's also a link back.

## Literate database management

Literate programming keeps the prose and the code that implements it in
one file. DanKG does the same for data. A markdown file can hold the
explanation of a table, the ETL that builds it, and a link from the
table back to both. Running DanKG on the file renders the tables and
views as nodes in the graph. Each node links back to the block that
produced it and forward to everything downstream. "Where did this number
come from" becomes a question you answer by following a link.

DuckDB is the first database supported. DanKG never links it in. DuckDB
runs as a configured command, like any other interpreter.

## Code evaluation

DanKG can evaluate code with the configured compiler or interpreter on
PATH, or in a virtual environment like `uv`. DanKG renders the output in
the knowledge graph.

DanKG evaluates only the code that is in view, meaning the current
context. If that code depends on code in another context, DanKG pulls
in only what it needs.

You define code blocks using standard markdown syntax. You can define
rules for code block evaluation in the config file or in each markdown
file.

DanKG never evaluates code automatically. You must evaluate it manually,
within a context.
