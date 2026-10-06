# Graph resolve

Resolution runs only once the *whole* corpus is parsed, for two reasons
that both trace back to the same fact. A link's target may live in a file
that had not been read yet when the link itself was found. Backlinks
are only honest once every file has been seen ([decision 6](../../architecture.md#decision-6-index-scope)).
The root is a hard boundary here too. A link that would escape it is
refused rather than followed, never even attempted, because "an LLM can
run it for you" means the markdown being graphed is frequently untrusted
input, not a file the reader necessarily wrote themselves.

```rust name=module_doc path=graph/resolve.rs
//! Link resolution and root containment.
//!
//! Resolution runs once the whole corpus is parsed, because a link's target may
//! live in a file that had not been read when the link was found, and because
//! backlinks are only honest when every file has been seen.
//!
//! The root is a hard boundary. A link that escapes it is refused rather than
//! followed. "an LLM can run it for you" means the markdown being graphed is
//! frequently untrusted input.

use super::build::{file_stem, strip_extension, ParsedFile, RawLink, Target};
use super::model::{Edge, EdgeKind, Graph, Node, NodeId, NodeKind};
use super::slug::slugify;
use crate::diag::Diags;

pub fn resolve(files: &[ParsedFile], diags: &mut Diags) -> Graph {
    let mut graph = Graph::default();

    for file in files {
        graph.nodes.extend(file.nodes.iter().cloned());
        graph.edges.extend(file.containment.iter().cloned());
    }

    for file in files {
        for link in &file.links {
            match resolve_one(file, link, files, diags) {
                Resolution::Found(to) => graph.edges.push(Edge {
                    from: link.from.clone(),
                    to,
                    kind: EdgeKind::Link,
                    line: link.line,
                    reciprocated: false,
                    quote: String::new(),
                }),
                Resolution::Dangling(node) => {
                    let id = node.id.clone();
                    if !graph.contains(&id) {
                        graph.nodes.push(node);
                    }
                    graph.edges.push(Edge {
                        from: link.from.clone(),
                        to: id,
                        kind: EdgeKind::Link,
                        line: link.line,
                        reciprocated: false,
                        quote: String::new(),
                    });
                }
                Resolution::Refused => {}
            }
        }
    }

    resolve_file_reads(files, &mut graph);
    resolve_depends(files, &mut graph, diags);
    resolve_eval_chain(files, &mut graph);

    graph.reciprocate();
    graph.sort();
    graph
}

/// Decision 76. Joins every `reads=file:PATH` to the artifact node of
/// whichever block writes that same file, anywhere in the corpus.
///
/// Both sides are compared as root-relative normalized paths, through the
/// same `join_normalize`/`dir_of` a written link already resolves through
/// and `eval::plan::check_file_deps` already compares with. Two blocks in
/// different directories naming one file by different relative spellings
/// therefore still match, and the graph cannot disagree with `check`
/// about which file an edge is about.
///
/// The producer side needs nothing stored. An artifact node already
/// carries the path as written, as its title (decision 74), and the file
/// that declared it, so its resolved path is recomputed here from the
/// node itself.
///
/// A read with no producer anywhere gets no edge, and no placeholder. A
/// block reading a file the corpus does not generate is ordinary -- a
/// checked-in CSV is the common case -- so a decision 8 placeholder would
/// report a dangling reference that is not one, and `dankg check` would
/// fail a corpus that is correct. `check_file_deps` is what reports a
/// `reads=` whose declared `deps=` names no matching producer, and it
/// stays the only thing that does.
fn resolve_file_reads(files: &[ParsedFile], graph: &mut Graph) {
    // Every artifact in the corpus, by its own resolved path.
    let mut producers: Vec<(String, NodeId)> = Vec::new();
    for file in files {
        for node in &file.nodes {
            if node.kind != NodeKind::Artifact {
                continue;
            }
            if let Some(resolved) = join_normalize(dir_of(&file.path), &node.title) {
                producers.push((resolved, node.id.clone()));
            }
        }
    }

    for file in files {
        for read in &file.reads {
            let Some(wanted) = join_normalize(dir_of(&file.path), &read.path) else { continue };
            for (resolved, artifact) in &producers {
                if resolved != &wanted {
                    continue;
                }
                // Artifact to reader, the direction a relation's own
                // `Reads` edge already runs (decision 35).
                graph.edges.push(Edge {
                    from: artifact.clone(),
                    to: read.from.clone(),
                    kind: EdgeKind::Reads,
                    line: read.line,
                    reciprocated: false,
                    quote: String::new(),
                });
            }
        }
    }
}
```

## A prose dependency, and an eval chain

Decision 32's marker becomes an edge here rather than in
[`graph::build`](build.md), for the reason every other authored relation
waits: the section a marker names routinely lives in another file. Both
passes below run after `resolve_file_reads` and before `reciprocate`, so
a `Depends` edge is reciprocated exactly the way a `Link` already is when
two sections depend on each other's claims.

A marker resolves through `depends::resolve_target`, which is
`resolve_path`'s own `join_normalize`/`dir_of` narrowed to the one shape a
`target=` can take. Reusing it is the point. `dankg check` reports a
marker against the graph it already built. A marker that `check` called
resolvable and the graph called dangling would be two answers to one
question.

The three outcomes are a written link's own. A target escaping the root
is refused and never graphed. A target naming nothing gets decision 8's
placeholder, which is what makes an unwritten section a reader already
depends on visible rather than merely warned about. Everything else is an
edge carrying the marker's `quote=`.

One difference from a link is deliberate. A dangling link warns through
`Diags`. `dankg check` then fails on it. A dangling marker warns the
same way. `check` stays advisory about it, because decision 32 says a
quoted claim is a weak signal. The warning is the graph's, not a new
gate.

```rust name=resolve_depends path=graph/resolve.rs
/// Decision 32's `dankg:depends` marker, as an edge from the section that
/// declared it to the section it names.
///
/// Resolution is `depends::resolve_target`, the same function `check_cmd`
/// resolves a marker through, so the graph and `dankg check` cannot
/// disagree about where a marker points. The `quote=` rides onto the edge
/// here and is never verified: comparing it against the target's own text
/// is `check`'s job and stays advisory (decision 32).
fn resolve_depends(files: &[ParsedFile], graph: &mut Graph, diags: &mut Diags) {
    for file in files {
        for marker in &file.depends {
            let Some(to) = crate::depends::resolve_target(&file.path, &marker.target) else {
                diags.warn_in(
                    &file.path,
                    marker.line,
                    format!("`{}` escapes the root, not followed", marker.target),
                );
                continue;
            };

            // An existing-but-unresolved node is a placeholder some
            // dangling link already pushed. Reusing it is what keeps one
            // unwritten section one node, however many references reach
            // it.
            if !graph.node(&to).is_some_and(|n| n.resolved) {
                if !graph.contains(&to) {
                    diags.warn_in(
                        &file.path,
                        marker.line,
                        format!("depends on `{}`, which names no section", marker.target),
                    );
                    let node = placeholder(&to.file, &to.slug, &to.file);
                    graph.nodes.push(node);
                }
            }

            graph.edges.push(Edge {
                from: marker.from.clone(),
                to,
                kind: EdgeKind::Depends,
                line: marker.line,
                reciprocated: false,
                quote: marker.quote.clone(),
            });
        }
    }
}
```

An eval chain resolves differently. The difference is worth stating. A
`deps=` entry names a block *by its declared name*, never by its slug.
Those two come apart. A block whose name collides with a heading's takes
a `-1` suffix from the file's own `Slugger`. `slugify(name)` would then
address the wrong node, or no node at all. Matching on the block node's
own `title` -- which is exactly the declared name -- is therefore exact
where a slug lookup is a guess.

This is the one place a marker and a dep part company. Open question 3
settled that a marker addresses its target by slug and accepts the
order-dependence, because a `target=` is written as a fragment and a
reader expects fragment rules. A `deps=` entry was never a fragment.

A dep naming no such block gets no edge and no placeholder.
`eval::plan::resolve_dep` already refuses to plan that corpus, naming the
block and the entry. `dankg eval` fails on it. A placeholder here would
draw a second report of one error. A `dankg check` already failing would
gain a dangling node it cannot act on.

```rust name=resolve_eval_chain path=graph/resolve.rs
/// Every `deps=`/`xdeps=` entry, as a block-to-block edge.
///
/// Path resolution is `plan::split_dep` plus the same
/// `join_normalize`/`dir_of` `plan::resolve_dep` uses, so the edge and the
/// eval plan always agree on which file an entry means. The *name* half
/// is matched against a block node's own `title` rather than slugified: a
/// block's declared name and its slug come apart under collision
/// (`tests` beside a `## Tests` heading becomes `tests-1`), and the
/// declared name is what `deps=` was written against.
fn resolve_eval_chain(files: &[ParsedFile], graph: &mut Graph) {
    for file in files {
        for dep in &file.deps {
            let (path_part, name) = crate::eval::plan::split_dep(&dep.target);
            let key = match path_part {
                None => file.key.clone(),
                Some(rel) => {
                    let Some(joined) = join_normalize(dir_of(&file.path), rel) else { continue };
                    strip_extension(&joined)
                }
            };
            let Some(to) = graph
                .nodes
                .iter()
                .find(|n| n.kind == NodeKind::Block && n.id.file == key && n.title == name)
                .map(|n| n.id.clone())
            else {
                continue;
            };
            graph.edges.push(Edge {
                from: dep.from.clone(),
                to,
                kind: EdgeKind::EvalChain,
                line: dep.line,
                reciprocated: false,
                quote: String::new(),
            });
        }
    }
}
```

Every link resolves to one of exactly three outcomes. A dangling target
still *becomes* a node rather than just a warning. That is the entire
mechanism behind seeing what a corpus references but has not yet written.
The placeholder is the visible trace of the gap.

```rust name=resolution_and_path path=graph/resolve.rs
enum Resolution {
    Found(NodeId),
    /// The target does not exist. It still becomes a node. A dangling link is
    /// how you see what you have referenced but not yet written.
    Dangling(Node),
    /// Outside the root. Not followed, and not graphed.
    Refused,
}

fn resolve_one(
    file: &ParsedFile,
    link: &RawLink,
    files: &[ParsedFile],
    diags: &mut Diags,
) -> Resolution {
    match &link.target {
        Target::Path(dest) => resolve_path(file, link, dest, files, diags),
        Target::Wiki(target) => resolve_wiki(file, link, target, files, diags),
    }
}

fn resolve_path(
    file: &ParsedFile,
    link: &RawLink,
    dest: &str,
    files: &[ParsedFile],
    diags: &mut Diags,
) -> Resolution {
    let (path_part, fragment) = split_fragment(dest);

    // A bare `#fragment` addresses the current file.
    if path_part.is_empty() {
        return match find_slug(file, fragment) {
            Some(id) => Resolution::Found(id),
            None => {
                diags.warn_in(
                    &file.path,
                    link.line,
                    format!("no heading `{fragment}` in this file"),
                );
                Resolution::Dangling(placeholder(&file.key, fragment, &file.path))
            }
        };
    }

    let Some(joined) = join_normalize(dir_of(&file.path), path_part) else {
        diags.warn_in(
            &file.path,
            link.line,
            format!("`{path_part}` escapes the root, not followed"),
        );
        return Resolution::Refused;
    };

    let key = strip_extension(&joined);
    let Some(target_file) = files.iter().find(|f| f.key == key) else {
        diags.warn_in(&file.path, link.line, format!("file not found: `{path_part}`"));
        let slug = if fragment.is_empty() { file_stem(&key).to_string() } else { fragment.to_string() };
        return Resolution::Dangling(placeholder(&key, &slug, &joined));
    };

    if fragment.is_empty() {
        return match target_file.entry_node() {
            Some(node) => Resolution::Found(node.id.clone()),
            None => Resolution::Dangling(placeholder(&key, file_stem(&key), &target_file.path)),
        };
    }

    match find_slug(target_file, fragment) {
        Some(id) => Resolution::Found(id),
        None => {
            diags.warn_in(
                &file.path,
                link.line,
                format!("no heading `{fragment}` in `{}`", target_file.path),
            );
            Resolution::Dangling(placeholder(&key, fragment, &target_file.path))
        }
    }
}
```

Wikilinks resolve differently from a written path on purpose.
`[[Heading]]` searches heading slugs across the *whole* corpus first,
since the wikilink form exists specifically to address headings by name
rather than by file. Only when nothing slugifies to a match does it fall
back to a file of that name, so `[[ideas]]` still finds `ideas.md` even
when no heading in it happens to slugify to "ideas".

```rust name=resolve_wiki path=graph/resolve.rs
fn resolve_wiki(
    file: &ParsedFile,
    link: &RawLink,
    target: &str,
    files: &[ParsedFile],
    diags: &mut Diags,
) -> Resolution {
    let (name, fragment) = split_fragment(target);

    // `[[#Heading]]` addresses the current file.
    if name.is_empty() {
        return match find_slug(file, fragment) {
            Some(id) => Resolution::Found(id),
            None => {
                diags.warn_in(&file.path, link.line, format!("no heading `{fragment}` in this file"));
                Resolution::Dangling(placeholder(&file.key, fragment, &file.path))
            }
        };
    }

    // `[[other#Heading]]`: find the file by stem or alias, then the heading.
    if !fragment.is_empty() {
        let matches = files_named(files, name);
        let Some(target_file) = pick(&matches, file, link, name, diags) else {
            diags.warn_in(&file.path, link.line, format!("no file named `{name}`"));
            return Resolution::Dangling(placeholder(name, fragment, name));
        };
        return match find_slug(target_file, fragment) {
            Some(id) => Resolution::Found(id),
            None => {
                diags.warn_in(
                    &file.path,
                    link.line,
                    format!("no heading `{fragment}` in `{}`", target_file.path),
                );
                Resolution::Dangling(placeholder(&target_file.key, fragment, &target_file.path))
            }
        };
    }

    // `[[Heading]]`: a slug search across the corpus comes first, since the
    // wikilink form is written to address headings.
    let wanted = slugify(name);
    let mut hits: Vec<&Node> = files
        .iter()
        .flat_map(|f| f.nodes.iter())
        .filter(|n| n.id.slug == wanted)
        .collect();
    hits.sort_by(|a, b| a.id.cmp(&b.id));

    if let Some(first) = hits.first() {
        if hits.len() > 1 {
            diags.warn_in(
                &file.path,
                link.line,
                format!(
                    "`[[{name}]]` is ambiguous ({} matches), using `{}`",
                    hits.len(),
                    first.id
                ),
            );
        }
        return Resolution::Found(first.id.clone());
    }

    // Fall back to a file of that name, so `[[ideas]]` finds ideas.md even when
    // no heading slugifies to "ideas".
    let matches = files_named(files, name);
    if let Some(target_file) = pick(&matches, file, link, name, diags) {
        if let Some(node) = target_file.entry_node() {
            return Resolution::Found(node.id.clone());
        }
    }

    diags.warn_in(&file.path, link.line, format!("unresolved wikilink `[[{name}]]`"));
    Resolution::Dangling(placeholder(name, &wanted, name))
}
```

Ambiguity -- two files with the same stem, two headings with the same
slug -- is always reported, then settled deterministically by taking the
lexicographically first match, so the graph never depends on filesystem
walk order for something a reader would notice as flaky.

```rust name=lookup_helpers path=graph/resolve.rs
fn files_named<'a>(files: &'a [ParsedFile], name: &str) -> Vec<&'a ParsedFile> {
    let mut out: Vec<&ParsedFile> = files
        .iter()
        .filter(|f| {
            file_stem(&f.key).eq_ignore_ascii_case(name)
                || f.aliases.iter().any(|a| a.eq_ignore_ascii_case(name))
        })
        .collect();
    out.sort_by(|a, b| a.key.cmp(&b.key));
    out
}

/// Ambiguity is reported, then settled by taking the lexicographically first
/// path, so the graph stays deterministic.
fn pick<'a>(
    matches: &[&'a ParsedFile],
    from: &ParsedFile,
    link: &RawLink,
    name: &str,
    diags: &mut Diags,
) -> Option<&'a ParsedFile> {
    let first = matches.first()?;
    if matches.len() > 1 {
        diags.warn_in(
            &from.path,
            link.line,
            format!("`{name}` is ambiguous ({} files), using `{}`", matches.len(), first.path),
        );
    }
    Some(first)
}

fn find_slug(file: &ParsedFile, fragment: &str) -> Option<NodeId> {
    let wanted = slugify(fragment);
    file.nodes
        .iter()
        .find(|n| n.id.slug == wanted || n.id.slug == fragment)
        .map(|n| n.id.clone())
}
```

A dangling placeholder is always created as a heading, never a block.
`[text](file#name)` can only ever address a heading, so whatever it
failed to find would have had to be one too.

```rust name=placeholder_and_split path=graph/resolve.rs
fn placeholder(key: &str, slug: &str, path: &str) -> Node {
    let slug = if slug.is_empty() { "section".to_string() } else { slugify(slug) };
    Node {
        id: NodeId::new(key, &slug),
        title: slug.replace('-', " "),
        file: path.to_string(),
        line: 0,
        end_line: 0,
        level: 0,
        parent: None,
        tags: Vec::new(),
        external: Vec::new(),
        resolved: false,
        // A dangling link always points at a heading-shaped target. A
        // block is never something `[text](file#name)` can name. So a
        // placeholder invented to receive one is a placeholder heading.
        kind: NodeKind::Heading,
    }
}

fn split_fragment(dest: &str) -> (&str, &str) {
    match dest.split_once('#') {
        Some((path, fragment)) => (path.trim(), fragment.trim()),
        None => (dest.trim(), ""),
    }
}
```

`join_normalize` is shared with `eval::plan`'s cross-file `deps=`
resolution rather than reimplemented there. A dependency and a written
link agree about what a relative path means and about the root boundary,
because they are, quite literally, the same function.

```rust name=dir_of_and_join_normalize path=graph/resolve.rs
/// The root-relative directory a root-relative file path sits in. Shared
/// with `eval::plan`'s cross-file `deps=` resolution, so a dependency and a
/// written link agree about what a relative path means without a second
/// implementation of "relative to this file" to keep in step.
pub(crate) fn dir_of(path: &str) -> &str {
    match path.rfind('/') {
        Some(i) => &path[..i],
        None => "",
    }
}

/// Join a relative link onto the linking file's directory and normalize it.
/// Returns `None` when the result climbs above the root. Also `eval::plan`'s
/// only way to turn a cross-file `deps=other.md#name` entry into the file it
/// names -- the same boundary refusal a written link already gets, reused
/// rather than reimplemented.
pub(crate) fn join_normalize(base_dir: &str, rel: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();

    let segments = if rel.starts_with('/') {
        // A root-relative link is taken as relative to the root itself.
        rel.trim_start_matches('/').split('/').collect::<Vec<_>>()
    } else {
        base_dir
            .split('/')
            .chain(rel.split('/'))
            .collect::<Vec<_>>()
    };

    for segment in segments {
        match segment {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return None;
                }
            }
            other => parts.push(other),
        }
    }

    Some(parts.join("/"))
}
```

## Tests

```rust name=tests path=graph/resolve.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::build;
    use crate::md::Document;

    fn corpus(files: &[(&str, &str)]) -> (Vec<ParsedFile>, Diags) {
        let mut diags = Diags::new("corpus");
        let parsed: Vec<ParsedFile> = files
            .iter()
            .map(|(path, src)| {
                let mut d = Diags::new(*path);
                let doc = Document::parse(src, &mut d);
                let built = build::build(path, &doc, src.lines().count() as u32, &mut d);
                diags.absorb(d);
                built
            })
            .collect();
        (parsed, diags)
    }

    fn graph_of(files: &[(&str, &str)]) -> (Graph, Diags) {
        let (parsed, mut diags) = corpus(files);
        let g = resolve(&parsed, &mut diags);
        (g, diags)
    }

    fn links(g: &Graph) -> Vec<(String, String, bool)> {
        g.edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Link)
            .map(|e| (e.from.to_string(), e.to.to_string(), e.reciprocated))
            .collect()
    }

    #[test]
    fn same_file_fragment() {
        let (g, d) = graph_of(&[("a.md", "# One\n\n[go](#two)\n\n# Two\n")]);
        assert_eq!(links(&g), vec![("a#one".into(), "a#two".into(), false)]);
        assert!(d.is_empty());
    }

    #[test]
    fn cross_file_with_fragment() {
        let (g, d) = graph_of(&[
            ("notes/a.md", "# One\n\n[go](b.md#two)\n"),
            ("notes/b.md", "# Two\n"),
        ]);
        assert_eq!(links(&g), vec![("notes/a#one".into(), "notes/b#two".into(), false)]);
        assert!(d.is_empty());
    }

    #[test]
    fn relative_paths_normalize() {
        let (g, d) = graph_of(&[
            ("notes/deep/a.md", "# One\n\n[go](../b.md)\n"),
            ("notes/b.md", "# Two\n"),
        ]);
        assert_eq!(links(&g), vec![("notes/deep/a#one".into(), "notes/b#two".into(), false)]);
        assert!(d.is_empty());
    }

    #[test]
    fn bare_file_link_lands_on_first_heading() {
        let (g, _) = graph_of(&[("a.md", "# One\n\n[go](b.md)\n"), ("b.md", "# First\n\n# Second\n")]);
        assert_eq!(links(&g)[0].1, "b#first");
    }

    #[test]
    fn escaping_the_root_is_refused_not_followed() {
        let (g, d) = graph_of(&[("a.md", "# One\n\n[bad](../../etc/passwd.md)\n")]);
        assert!(links(&g).is_empty(), "no edge is created");
        assert!(d.items()[0].message.contains("escapes the root"));
    }

    #[test]
    fn missing_file_becomes_a_placeholder_with_a_warning() {
        let (g, d) = graph_of(&[("a.md", "# One\n\n[go](ideas.md#future)\n")]);
        let target = g.node(&NodeId::new("ideas", "future")).expect("placeholder exists");
        assert!(!target.resolved);
        assert!(d.items()[0].message.contains("file not found"));
        assert_eq!(d.items()[0].line, 3);
    }

    #[test]
    fn missing_heading_in_an_existing_file_warns() {
        let (g, d) = graph_of(&[("a.md", "# One\n\n[go](b.md#nope)\n"), ("b.md", "# Two\n")]);
        assert!(!g.node(&NodeId::new("b", "nope")).unwrap().resolved);
        assert!(d.items()[0].message.contains("no heading `nope`"));
    }

    #[test]
    fn wikilink_resolves_by_heading_slug() {
        let (g, d) = graph_of(&[("a.md", "# One\n\n[[Code Eval]]\n"), ("b.md", "# Code Eval\n")]);
        assert_eq!(links(&g)[0].1, "b#code-eval");
        assert!(d.is_empty());
    }

    #[test]
    fn wikilink_falls_back_to_file_name() {
        let (g, d) = graph_of(&[("a.md", "# One\n\n[[ideas]]\n"), ("ideas.md", "# Unrelated Title\n")]);
        assert_eq!(links(&g)[0].1, "ideas#unrelated-title");
        assert!(d.is_empty());
    }

    #[test]
    fn wikilink_with_fragment_finds_file_then_heading() {
        let (g, _) = graph_of(&[("a.md", "# One\n\n[[b#Two]]\n"), ("x/b.md", "# Two\n")]);
        assert_eq!(links(&g)[0].1, "x/b#two");
    }

    #[test]
    fn ambiguous_wikilink_warns_and_picks_first_path() {
        let (g, d) = graph_of(&[
            ("a.md", "# Start\n\n[[Shared]]\n"),
            ("z.md", "# Shared\n"),
            ("m.md", "# Shared\n"),
        ]);
        assert_eq!(links(&g)[0].1, "m#shared");
        assert!(d.items()[0].message.contains("ambiguous"));
    }

    #[test]
    fn alias_resolves_a_wikilink() {
        let (g, d) = graph_of(&[
            ("a.md", "# One\n\n[[arch#Two]]\n"),
            ("b.md", "---\nalias: arch\n---\n# Two\n"),
        ]);
        assert_eq!(links(&g)[0].1, "b#two");
        assert!(d.is_empty());
    }

    #[test]
    /// Decision 76, same file. The artifact sits between the two blocks:
    /// producer writes it, consumer reads it.
    #[test]
    fn a_reads_file_joins_the_consumer_to_the_artifact() {
        let (g, d) = graph_of(&[(
            "a.md",
            "```sh name=fetch produces=file:raw.csv\n:\n```\n\n```sh name=clean reads=file:raw.csv\n:\n```\n",
        )]);
        let artifact = NodeId::new("a", "raw");
        assert!(
            g.edges.iter().any(|e| e.kind == EdgeKind::Produces
                && e.from == NodeId::new("a", "fetch")
                && e.to == artifact),
            "{:?}",
            g.edges
        );
        assert!(
            g.edges.iter().any(|e| e.kind == EdgeKind::Reads
                && e.from == artifact
                && e.to == NodeId::new("a", "clean")),
            "{:?}",
            g.edges
        );
        assert!(d.is_empty(), "{d:?}");
    }

    /// The match is on the resolved path, never the written one. Two
    /// blocks in different directories spell one file differently and
    /// still join -- the same comparison `eval::plan::check_file_deps`
    /// already makes.
    #[test]
    fn a_reads_file_joins_across_directories_and_spellings() {
        let (g, _) = graph_of(&[
            ("a.md", "```sh name=fetch produces=file:data/raw.csv\n:\n```\n"),
            ("sub/b.md", "```sh name=clean reads=file:../data/raw.csv\n:\n```\n"),
        ]);
        assert!(
            g.edges.iter().any(|e| e.kind == EdgeKind::Reads
                && e.from == NodeId::new("a", "raw")
                && e.to == NodeId::new("sub/b", "clean")),
            "{:?}",
            g.edges
        );
    }

    /// A read with no producer anywhere gets no edge and no placeholder.
    /// Reading a file the corpus does not generate is ordinary, so
    /// inventing a node would fail `dankg check` over a correct corpus.
    #[test]
    fn a_reads_file_with_no_producer_is_silent() {
        let (g, d) = graph_of(&[("a.md", "```sh name=clean reads=file:external.csv\n:\n```\n")]);
        assert!(g.nodes.iter().all(|n| n.kind != NodeKind::Artifact), "{:?}", g.nodes);
        assert!(g.edges.iter().all(|e| e.kind != EdgeKind::Reads), "{:?}", g.edges);
        assert!(g.nodes.iter().all(|n| n.resolved), "no placeholder may be invented: {:?}", g.nodes);
        assert!(d.is_empty(), "{d:?}");
    }

    /// A `reads=` naming a relation rather than a file is the db path,
    /// and decision 76 leaves it alone.
    #[test]
    fn a_reads_without_a_file_prefix_joins_no_artifact() {
        let (g, _) = graph_of(&[(
            "a.md",
            "```sh name=fetch produces=file:raw.csv\n:\n```\n\n```sh name=clean reads=orders\n:\n```\n",
        )]);
        assert!(
            g.edges.iter().all(|e| e.kind != EdgeKind::Reads),
            "a bare relation name is not a file: {:?}",
            g.edges
        );
    }

    /// The whole point of decision 69. A block and the file it writes
    /// are two targets, and a fragment reaches each by its own name.
    /// `resolve` needs no change for this: the artifact's slug is a real
    /// slug in its own file, which is what makes it a node rather than
    /// an alias on the block.
    #[test]
    fn a_fragment_reaches_an_artifact_and_its_block_separately() {
        let (g, d) = graph_of(&[(
            "a.md",
            "# One\n\nSee [[#quarterly]] and [[#chart]].\n\n```sh name=chart produces=file:data/quarterly.csv\n:\n```\n",
        )]);
        // `links` comes back in `Graph::sort`'s own order, not the
        // document's, so this asserts on membership rather than sequence.
        let mut targets: Vec<String> = links(&g).iter().map(|(_, to, _)| to.clone()).collect();
        targets.sort();
        assert_eq!(targets, vec!["a#chart", "a#quarterly"]);
        assert!(d.is_empty(), "{d:?}");
    }

    /// Decision 69 is what makes this reachable. Before it, a fragment
    /// naming a produced file resolved to nothing and `dankg check`
    /// reported a dead link over a reference that was correct.
    #[test]
    fn a_fragment_naming_a_produced_file_is_no_longer_a_dead_link() {
        let (g, _) = graph_of(&[(
            "a.md",
            "# One\n\n[[#quarterly]]\n\n```sh name=chart produces=file:data/quarterly.csv\n:\n```\n",
        )]);
        let target = g.node(&NodeId::new("a", "quarterly")).expect("artifact node exists");
        assert!(target.resolved, "a placeholder would mean the reference still dangles");
        assert_eq!(target.title, "data/quarterly.csv");
    }

    #[test]
    fn mutual_links_are_reciprocated() {
        let (g, _) = graph_of(&[
            ("a.md", "# One\n\n[go](b.md#two)\n"),
            ("b.md", "# Two\n\n[back](a.md#one)\n"),
        ]);
        assert!(links(&g).iter().all(|(_, _, recip)| *recip));
    }

    #[test]
    fn containment_edges_survive_resolution() {
        let (g, _) = graph_of(&[("a.md", "# One\n\n## Two\n")]);
        let contains: Vec<_> = g.edges.iter().filter(|e| e.kind == EdgeKind::Contains).collect();
        assert_eq!(contains.len(), 1);
        assert_eq!(contains[0].from.to_string(), "a#one");
    }

    #[test]
    fn output_is_deterministic() {
        let files = [("b.md", "# Two\n\n[x](a.md#one)\n"), ("a.md", "# One\n")];
        let (g1, _) = graph_of(&files);
        let mut reversed = files;
        reversed.reverse();
        let (g2, _) = graph_of(&reversed);
        assert_eq!(g1, g2);
    }

    fn of_kind(g: &Graph, kind: EdgeKind) -> Vec<(String, String, String)> {
        g.edges
            .iter()
            .filter(|e| e.kind == kind)
            .map(|e| (e.from.to_string(), e.to.to_string(), e.quote.clone()))
            .collect()
    }

    #[test]
    fn a_marker_becomes_an_edge_from_the_section_that_declares_it() {
        let (g, d) = graph_of(&[
            (
                "a.md",
                "# One\n\n## Inner\n\n<!-- dankg:depends target=b.md#two quote=\"a claim\" -->\n",
            ),
            ("b.md", "# Two\n\nIt makes a claim here.\n"),
        ]);
        // `a#inner`, not `a#one`: the innermost heading open at the
        // marker's own line is the section that declares it.
        assert_eq!(
            of_kind(&g, EdgeKind::Depends),
            vec![("a#inner".to_string(), "b#two".to_string(), "a claim".to_string())]
        );
        assert!(d.is_empty(), "a resolving marker warns about nothing: {:?}", d.items());
    }

    #[test]
    fn a_marker_naming_no_section_becomes_a_placeholder_and_warns() {
        let (g, d) = graph_of(&[(
            "a.md",
            "# One\n\n<!-- dankg:depends target=#gone quote=\"x\" -->\n",
        )]);
        let target = g.node(&NodeId::new("a", "gone")).expect("placeholder exists");
        assert!(!target.resolved);
        assert_eq!(of_kind(&g, EdgeKind::Depends).len(), 1, "the edge is drawn anyway");
        assert!(d.items()[0].message.contains("names no section"));
        assert_eq!(d.items()[0].line, 3);
    }

    #[test]
    fn a_marker_escaping_the_root_is_refused_not_followed() {
        let (g, d) = graph_of(&[(
            "a.md",
            "# One\n\n<!-- dankg:depends target=../../etc/passwd.md#x quote=\"x\" -->\n",
        )]);
        assert!(of_kind(&g, EdgeKind::Depends).is_empty(), "no edge is created");
        assert!(d.items()[0].message.contains("escapes the root"));
    }

    #[test]
    fn one_placeholder_serves_a_dangling_link_and_a_marker_naming_the_same_section() {
        let (g, _) = graph_of(&[(
            "a.md",
            "# One\n\n[go](#gone)\n\n<!-- dankg:depends target=#gone quote=\"x\" -->\n",
        )]);
        let placeholders: Vec<_> =
            g.nodes.iter().filter(|n| n.id == NodeId::new("a", "gone")).collect();
        assert_eq!(placeholders.len(), 1, "one unwritten section is one node: {placeholders:?}");
    }

    #[test]
    fn a_marker_and_a_written_link_between_the_same_sections_stay_two_edges() {
        let (g, _) = graph_of(&[
            (
                "a.md",
                "# One\n\n[go](b.md#two)\n\n<!-- dankg:depends target=b.md#two quote=\"q\" -->\n",
            ),
            ("b.md", "# Two\n"),
        ]);
        assert_eq!(links(&g), vec![("a#one".into(), "b#two".into(), false)]);
        assert_eq!(
            of_kind(&g, EdgeKind::Depends),
            vec![("a#one".to_string(), "b#two".to_string(), "q".to_string())]
        );
    }

    #[test]
    fn a_deps_and_an_xdeps_each_become_an_eval_chain_edge() {
        let (g, d) = graph_of(&[
            (
                "a.md",
                "# One\n\n```python name=setup\nx = 1\n```\n\n```python name=report deps=setup\nprint(x)\n```\n",
            ),
            (
                "notes/b.md",
                "# Two\n\n```python name=summary xdeps=../a.md#report\npass\n```\n",
            ),
        ]);
        let chain: Vec<(String, String)> = of_kind(&g, EdgeKind::EvalChain)
            .into_iter()
            .map(|(f, t, _)| (f, t))
            .collect();
        assert!(chain.contains(&("a#report".to_string(), "a#setup".to_string())), "{chain:?}");
        assert!(
            chain.contains(&("notes/b#summary".to_string(), "a#report".to_string())),
            "cross-file, through the same `path#name` form: {chain:?}"
        );
        assert_eq!(chain.len(), 2);
        assert!(d.is_empty(), "{:?}", d.items());
    }

    /// A `deps=` entry names a block by its *declared name*, which comes
    /// apart from its slug under collision: a `name=tests` block beside a
    /// `## Tests` heading is slugged `tests-1`. Matching on the slug
    /// would draw this edge to the heading, or to nothing.
    #[test]
    fn an_eval_chain_follows_the_declared_name_not_the_slug() {
        let (g, _) = graph_of(&[(
            "a.md",
            "# Tests\n\n```python name=tests\nx = 1\n```\n\n```python name=run deps=tests\nprint(x)\n```\n",
        )]);
        let chain = of_kind(&g, EdgeKind::EvalChain);
        assert_eq!(chain.len(), 1);
        assert_eq!(chain[0].0, "a#run");
        let to = g.node(&NodeId::new("a", chain[0].1.split('#').nth(1).unwrap())).unwrap();
        assert_eq!(to.kind, NodeKind::Block, "the block, never the heading it collided with");
        assert_eq!(to.title, "tests");
    }

    /// `eval::plan::resolve_dep` already refuses to plan a corpus whose
    /// `deps=` names nothing, saying which block and which entry. A
    /// placeholder here would be a second report of one error.
    #[test]
    fn a_deps_naming_no_block_gets_no_edge_and_no_placeholder() {
        let (g, d) = graph_of(&[(
            "a.md",
            "# One\n\n```python name=report deps=missing\npass\n```\n",
        )]);
        assert!(of_kind(&g, EdgeKind::EvalChain).is_empty());
        assert!(g.nodes.iter().all(|n| n.resolved), "{:?}", g.nodes);
        assert!(d.is_empty(), "{:?}", d.items());
    }

    #[test]
    fn a_marker_inside_a_list_item_is_no_edge_matching_check() {
        let (g, _) = graph_of(&[
            ("a.md", "# One\n\n- item\n\n  <!-- dankg:depends target=b.md#two quote=\"q\" -->\n"),
            ("b.md", "# Two\n"),
        ]);
        assert!(
            of_kind(&g, EdgeKind::Depends).is_empty(),
            "`depends::markers_in` does not find one here either, so the graph must not"
        );
    }
}
```
