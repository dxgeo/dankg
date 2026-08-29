//! Layout and the drawn formats.
//!
//! The golden files are the point: a layout that is not byte-identical from
//! one run to the next cannot be committed, and a graph you cannot commit you
//! cannot diff. Re-record after an intentional change:
//!
//!     DANKG_BLESS=1 cargo test --test layout

use dankg::diag::Diags;
use dankg::graph::build::{self, ParsedFile};
use dankg::graph::{resolve, view, Graph, NodeId};
use dankg::layout::{self, NODE_SEP};
use dankg::md::Document;
use dankg::render::{assets, dot, html, mermaid};
use std::fs;
use std::path::PathBuf;

const CORPUS: [&str; 3] = ["project.md", "notes/ideas.md", "orphan.md"];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The committed fixture corpus, indexed exactly as `tests/graph.rs` does it.
fn fixture() -> Graph {
    let base = root().join("tests/data/corpus");
    let mut diags = Diags::new("corpus");
    let files: Vec<ParsedFile> = CORPUS
        .iter()
        .map(|rel| {
            let source = fs::read_to_string(base.join(rel)).expect("fixture is missing");
            let mut d = Diags::new(*rel);
            let doc = Document::parse(&source, &mut d);
            build::build(rel, &doc, source.lines().count() as u32)
        })
        .collect();
    resolve::resolve(&files, &mut diags)
}

fn graph_of(files: &[(&str, &str)]) -> Graph {
    let mut diags = Diags::new("t");
    let parsed: Vec<ParsedFile> = files
        .iter()
        .map(|(path, src)| {
            let mut d = Diags::new(*path);
            let doc = Document::parse(src, &mut d);
            build::build(path, &doc, src.lines().count() as u32)
        })
        .collect();
    resolve::resolve(&parsed, &mut diags)
}

fn check_golden(name: &str, rendered: &str) {
    let path = root().join("tests/data").join(name);
    if std::env::var("DANKG_BLESS").is_ok() {
        fs::write(&path, rendered).expect("could not write golden file");
        return;
    }
    let expected = fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("no {name}; run with DANKG_BLESS=1 to create one"));
    assert_eq!(rendered, expected, "{name} changed; re-bless if intended");
}

#[test]
fn dot_matches_golden() {
    let graph = fixture();
    check_golden("corpus.golden.dot", &dot::render(&graph, &layout::layout(&graph)));
}

#[test]
fn mermaid_matches_golden() {
    let graph = fixture();
    check_golden("corpus.golden.mmd", &mermaid::render(&graph, &layout::layout(&graph)));
}

/// The page is rendered from the whole fixture as both the drawing and the
/// index, so the golden pins the stylesheet, the script and the markup in one
/// file -- which is the only honest way to review a format whose three halves
/// are maintained by hand.
fn fixture_page() -> String {
    let graph = fixture();
    let entries = vec!["project.md".to_string()];
    let page = html::Page { root: "tests/data/corpus", entries: &entries };
    html::render(&graph, &layout::layout(&graph), &graph, &page)
}

#[test]
fn html_matches_golden() {
    check_golden("corpus.golden.html", &fixture_page());
}

mod html_structure {
    use super::*;

    /// Every id the script reaches for has to be one the renderer emits. The
    /// two halves are hand-written in different languages and cannot be
    /// type-checked against each other, so this is the check that they agree.
    #[test]
    fn the_script_only_asks_for_elements_the_renderer_emits() {
        let page = fixture_page();
        let mut asked = 0;
        for (needle, close) in [("getElementById(\"", "\")")] {
            let mut rest = assets::JS;
            while let Some(at) = rest.find(needle) {
                rest = &rest[at + needle.len()..];
                let id = &rest[..rest.find(close).expect("a closed call")];
                assert!(page.contains(&format!("id=\"{id}\"")), "the script wants #{id}, which the page has not got");
                asked += 1;
            }
        }
        assert!(asked >= 7, "only {asked} ids checked; the scan stopped finding them");
    }

    /// Same drift, the other direction: a marker referenced but never defined
    /// silently renders an edge with no arrowhead.
    #[test]
    fn every_marker_an_edge_points_at_is_defined() {
        let page = fixture_page();
        let defs = &page[..page.find("<g id=\"scene\">").expect("a scene")];
        let mut rest = page.as_str();
        let mut seen = 0;
        while let Some(at) = rest.find("marker-end=\"url(#") {
            rest = &rest[at + "marker-end=\"url(#".len()..];
            let id = &rest[..rest.find(')').expect("a closed url()")];
            assert!(defs.contains(&format!("<marker id=\"{id}\"")), "no <marker id=\"{id}\">");
            seen += 1;
        }
        assert!(seen > 0, "the fixture draws no directed edges at all");
    }

    #[test]
    fn every_edge_endpoint_was_declared_as_a_node() {
        let page = fixture_page();
        let ids: Vec<&str> = collect(&page, "data-id=\"");
        assert!(!ids.is_empty());
        for attr in ["data-from=\"", "data-to=\""] {
            for endpoint in collect(&page, attr) {
                assert!(ids.contains(&endpoint), "edge names {endpoint}, which is not a box");
            }
        }
    }

    #[test]
    fn the_tags_the_renderer_opens_are_the_tags_it_closes() {
        let page = fixture_page();
        for tag in ["g", "a", "text", "title", "script", "svg", "header", "footer", "html", "body"] {
            let opened = page.matches(&format!("<{tag} ")).count() + page.matches(&format!("<{tag}>")).count();
            let closed = page.matches(&format!("</{tag}>")).count();
            assert_eq!(opened, closed, "<{tag}> opens {opened} times and closes {closed}");
        }
    }

    /// The blob is what expansion runs on, so a view that drew three boxes
    /// still has to carry the other nodes or clicking one would do nothing.
    #[test]
    fn the_page_ships_the_whole_index_however_little_it_draws() {
        let index = fixture();
        let view = view::select(&index, &[NodeId::new("notes/ideas", "ideas")], 0);
        let entries = vec!["notes/ideas.md".to_string()];
        let page = html::render(
            &view,
            &layout::layout(&view),
            &index,
            &html::Page { root: "tests/data/corpus", entries: &entries },
        );

        assert_eq!(page.matches("class=\"node").count(), 1, "depth 0 draws one box");
        for node in &index.nodes {
            assert!(
                page.contains(&format!("\"id\": \"{}\"", node.id)),
                "{} is not in the blob, so it could never be expanded into",
                node.id
            );
        }
    }

    fn collect<'a>(page: &'a str, attr: &str) -> Vec<&'a str> {
        let mut out = Vec::new();
        let mut rest = page;
        while let Some(at) = rest.find(attr) {
            rest = &rest[at + attr.len()..];
            out.push(&rest[..rest.find('"').expect("a closed attribute")]);
        }
        out
    }
}

#[test]
fn the_same_input_lays_out_byte_for_byte_the_same() {
    let graph = fixture();
    let first = dot::render(&graph, &layout::layout(&graph));
    for _ in 0..16 {
        assert_eq!(dot::render(&graph, &layout::layout(&graph)), first);
    }
}

#[test]
fn layout_does_not_depend_on_the_order_files_were_read_in() {
    let forwards = fixture();

    let base = root().join("tests/data/corpus");
    let mut order: Vec<&str> = CORPUS.to_vec();
    order.reverse();
    let mut diags = Diags::new("corpus");
    let files: Vec<ParsedFile> = order
        .iter()
        .map(|rel| {
            let source = fs::read_to_string(base.join(rel)).unwrap();
            let mut d = Diags::new(*rel);
            let doc = Document::parse(&source, &mut d);
            build::build(rel, &doc, source.lines().count() as u32)
        })
        .collect();
    let backwards = resolve::resolve(&files, &mut diags);

    assert_eq!(layout::layout(&forwards), layout::layout(&backwards));
}

/// The one invariant a reader notices immediately when it breaks.
#[test]
fn no_two_boxes_overlap() {
    let graph = graph_of(&[
        ("a.md", "# A Rather Long Heading\n\n[b](b.md#b) [c](c.md#c)\n\n## A1\n\n## A2\n"),
        ("b.md", "# B\n\n[c](c.md#c)\n\n## B1\n\n### B2\n"),
        ("c.md", "# C\n\n[a](a.md#a-rather-long-heading)\n"),
    ]);
    let laid = layout::layout(&graph);

    for layer in laid.layers() {
        for pair in layer.windows(2) {
            let gap = (pair[1].x - pair[1].width / 2) - (pair[0].x + pair[0].width / 2);
            assert!(gap >= NODE_SEP, "{} and {} overlap", pair[0].id, pair[1].id);
        }
    }
}

#[test]
fn every_polyline_runs_from_its_source_to_its_target() {
    let graph = graph_of(&[
        ("a.md", "# A\n\n[deep](b.md#b2)\n\n## A1\n"),
        ("b.md", "# B\n\n## B1\n\n### B2\n"),
    ]);
    let laid = layout::layout(&graph);
    let at = |id: &NodeId| laid.nodes.iter().find(|n| &n.id == id).expect("laid out");

    for edge in &laid.edges {
        let (from, to) = (at(&edge.from), at(&edge.to));
        assert_eq!(edge.points.first().unwrap().x, from.x, "{} starts adrift", edge.from);
        assert_eq!(edge.points.last().unwrap().x, to.x, "{} ends adrift", edge.to);
        // Every intermediate point sits strictly between the two ranks.
        for point in &edge.points[1..edge.points.len() - 1] {
            let (lo, hi) = (from.y.min(to.y), from.y.max(to.y));
            assert!(point.y >= lo && point.y <= hi, "a bend escaped its span");
        }
    }
}

#[test]
fn a_view_lays_out_as_a_graph_in_its_own_right() {
    let graph = graph_of(&[
        ("a.md", "# A\n\n[b](b.md#b)\n"),
        ("b.md", "# B\n\n[c](c.md#c)\n"),
        ("c.md", "# C\n\n[d](d.md#d)\n"),
        ("d.md", "# D\n"),
    ]);
    let entries = view::entry_nodes(&graph, &["a.md".to_string()]);
    let near = view::select(&graph, &entries, 1);

    let laid = layout::layout(&near);
    assert_eq!(laid.nodes.len(), 2, "the entry and one hop");
    assert_eq!(laid.ranks, 2);
    assert!(dot::render(&near, &laid).contains("\"a#a\" -> \"b#b\""));
}

/// Graphviz is not installed in every environment this runs in, so the output
/// is checked structurally rather than by shelling out to `dot`.
mod dot_shape {
    use super::*;

    fn declared_and_referenced(out: &str) -> (Vec<String>, Vec<String>) {
        let mut declared = Vec::new();
        let mut referenced = Vec::new();
        for line in out.lines().map(str::trim) {
            if let Some((left, right)) = line.split_once(" -> ") {
                referenced.push(left.to_string());
                referenced.push(
                    right.split(&[' ', ';', '['][..]).next().unwrap_or(right).to_string(),
                );
            } else if line.starts_with('"')
                && let Some(id) = line.split(" [").next()
            {
                declared.push(id.to_string());
            }
        }
        (declared, referenced)
    }

    #[test]
    fn braces_balance_and_the_graph_is_closed() {
        let graph = fixture();
        let out = dot::render(&graph, &layout::layout(&graph));
        let opens = out.matches('{').count();
        assert_eq!(opens, out.matches('}').count(), "unbalanced braces");
        assert!(out.trim_end().ends_with('}'));
    }

    #[test]
    fn every_edge_endpoint_was_declared_as_a_node() {
        let graph = fixture();
        let out = dot::render(&graph, &layout::layout(&graph));
        let (declared, referenced) = declared_and_referenced(&out);

        assert_eq!(declared.len(), graph.nodes.len());
        for id in &referenced {
            assert!(declared.contains(id), "{id} is used but never declared");
        }
    }

    #[test]
    fn positions_are_whole_numbers_so_the_file_diffs_cleanly() {
        let graph = fixture();
        let out = dot::render(&graph, &layout::layout(&graph));
        for line in out.lines().filter(|l| l.contains("pos=\"")) {
            let pos = line.split("pos=\"").nth(1).unwrap().split('!').next().unwrap();
            let (x, y) = pos.split_once(',').expect("pos is a pair");
            assert!(x.parse::<i32>().is_ok() && y.parse::<i32>().is_ok(), "{pos}");
        }
    }
}

mod mermaid_shape {
    use super::*;

    #[test]
    fn every_referenced_identifier_is_declared_first() {
        let graph = fixture();
        let out = mermaid::render(&graph, &layout::layout(&graph));

        let declared: Vec<&str> = out
            .lines()
            .filter_map(|l| l.trim().split_once('[').map(|(id, _)| id))
            .filter(|id| id.starts_with('n'))
            .collect();
        assert_eq!(declared.len(), graph.nodes.len());

        for line in out.lines().map(str::trim) {
            let Some(arrow) = ["==>", "-->", "---"].iter().find(|a| line.contains(**a)) else {
                continue;
            };
            let (left, right) = line.split_once(arrow).unwrap();
            for id in [left.trim(), right.trim()] {
                assert!(declared.contains(&id), "{id} is used but never declared");
            }
        }
    }

    #[test]
    fn the_class_line_only_names_declared_nodes() {
        let graph = fixture();
        let out = mermaid::render(&graph, &layout::layout(&graph));
        let class = out.lines().find(|l| l.trim().starts_with("class ")).expect("a dangling node");
        let named: Vec<&str> = class.trim().trim_start_matches("class ").trim_end_matches(" dangling").split(',').collect();
        assert_eq!(named.len(), graph.nodes.iter().filter(|n| !n.resolved).count());
    }
}

mod binary {
    use std::process::Command;

    fn run(args: &[&str]) -> (String, String, bool) {
        let out = Command::new(env!("CARGO_BIN_EXE_dankg"))
            .args(args)
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output()
            .expect("failed to run dankg");
        (
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
            out.status.success(),
        )
    }

    #[test]
    fn dot_and_mermaid_reach_stdout_and_diagnostics_do_not() {
        for format in ["dot", "mermaid"] {
            let (stdout, stderr, ok) =
                run(&["graph", "tests/data/corpus/project.md", "--format", format]);
            assert!(ok, "{format} failed: {stderr}");
            assert!(!stdout.contains("warn:"), "{format} put diagnostics on stdout");
            assert!(stderr.contains("warn:"), "{format} lost its diagnostics");
        }
    }

    #[test]
    fn depth_shrinks_the_drawing_and_all_does_not() {
        let (near, _, _) =
            run(&["graph", "tests/data/corpus/notes/ideas.md", "--format", "mermaid", "--depth", "0"]);
        let (whole, _, _) =
            run(&["graph", "tests/data/corpus/notes/ideas.md", "--format", "mermaid", "--all"]);

        // Declarations only: `n0["Ideas"]`, not the edge lines that also start
        // with an identifier.
        let count = |out: &str| out.lines().filter(|l| l.contains("[\"")).count();
        assert_eq!(count(&near), 2, "ideas.md has two headings and no hops: {near}");
        assert!(count(&whole) > count(&near), "--all draws more than depth 0");
    }

    #[test]
    fn the_drawn_view_is_reported_on_stderr() {
        let (_, stderr, _) =
            run(&["graph", "tests/data/corpus/notes/ideas.md", "--format", "dot", "--depth", "0"]);
        assert!(stderr.contains("indexed: 3 file(s)"), "{stderr}");
        assert!(stderr.contains("drawn: 2 nodes"), "{stderr}");
    }

    #[test]
    fn json_is_always_the_whole_index_and_says_so_when_asked_otherwise() {
        let (plain, _, _) = run(&["graph", "tests/data/corpus/notes/ideas.md"]);
        let (deep, stderr, _) =
            run(&["graph", "tests/data/corpus/notes/ideas.md", "--depth", "0"]);
        assert_eq!(plain, deep, "--depth must not quietly shrink the json");
        assert!(stderr.contains("does not apply to `--format json`"), "{stderr}");
    }

    #[test]
    fn naming_a_directory_draws_the_whole_corpus() {
        let (whole, _, _) = run(&["graph", "tests/data/corpus", "--format", "mermaid"]);
        let (all, _, _) = run(&["graph", "tests/data/corpus", "--format", "mermaid", "--all"]);
        assert_eq!(whole, all, "a directory names the corpus, so there is no view to take");
    }

    #[test]
    fn repeated_runs_are_byte_identical() {
        let (first, _, _) = run(&["graph", "tests/data/corpus/project.md", "--format", "dot"]);
        for _ in 0..4 {
            let (again, _, _) = run(&["graph", "tests/data/corpus/project.md", "--format", "dot"]);
            assert_eq!(first, again);
        }
    }

    #[test]
    fn html_is_one_self_contained_file_on_stdout() {
        let (stdout, stderr, ok) =
            run(&["graph", "tests/data/corpus/project.md", "--format", "html"]);
        assert!(ok, "{stderr}");
        assert!(stdout.starts_with("<!doctype html>"), "{}", &stdout[..80.min(stdout.len())]);
        assert!(stdout.trim_end().ends_with("</html>"));
        assert!(!stdout.contains("warn:"), "html put diagnostics on stdout");
        assert!(stderr.contains("warn:"), "html lost its diagnostics");
    }

    #[test]
    fn html_repeats_byte_for_byte_and_writes_the_same_thing_to_a_file() {
        let args = ["graph", "tests/data/corpus/project.md", "--format", "html"];
        let (first, _, _) = run(&args);
        for _ in 0..4 {
            assert_eq!(run(&args).0, first, "html is not deterministic");
        }

        let out = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("page.html");
        let path = out.to_str().unwrap();
        let (stdout, stderr, ok) = run(&["graph", "tests/data/corpus/project.md", "--format", "html", "-o", path]);
        assert!(ok, "{stderr}");
        assert!(stdout.is_empty(), "-o should write the file, not stdout");
        assert_eq!(std::fs::read_to_string(&out).unwrap(), first);
    }

    /// `--depth` shapes the drawing. It must not shape the blob: the index is
    /// what makes expansion possible without a second run.
    #[test]
    fn depth_shrinks_the_drawing_but_never_the_embedded_index() {
        let (near, _, _) =
            run(&["graph", "tests/data/corpus/notes/ideas.md", "--format", "html", "--depth", "0"]);
        let (whole, _, _) =
            run(&["graph", "tests/data/corpus/notes/ideas.md", "--format", "html", "--all"]);

        let boxes = |page: &str| page.matches("class=\"node").count();
        assert!(boxes(&near) < boxes(&whole), "depth 0 should draw less than --all");

        let blob = |page: &str| page.matches("\"id\": \"").count();
        assert_eq!(blob(&near), blob(&whole), "the index is the index at any depth");
    }
}
