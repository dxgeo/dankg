//! Graph construction over a fixture corpus, checked against golden JSON.
//!
//! The golden file is the point of milestone 2: the graph is fully testable
//! before anything is drawn. Re-record it after intentional changes:
//!
//!     DANKG_BLESS=1 cargo test --test graph

use dankg::diag::Diags;
use dankg::graph::build::{self, ParsedFile};
use dankg::graph::{resolve, EdgeKind, Graph, NodeId};
use dankg::md::Document;
use dankg::render::json;
use std::fs;
use std::path::PathBuf;

const CORPUS: [&str; 3] = ["project.md", "notes/ideas.md", "orphan.md"];
const GOLDEN: &str = "tests/data/corpus.golden.json";

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn build_corpus() -> (Graph, Diags) {
    let base = root().join("tests/data/corpus");
    let mut diags = Diags::new("corpus");
    let files: Vec<ParsedFile> = CORPUS
        .iter()
        .map(|rel| {
            let source = fs::read_to_string(base.join(rel)).expect("fixture is missing");
            let mut d = Diags::new(*rel);
            let doc = Document::parse(&source, &mut d);
            diags.absorb(d);
            build::build(rel, &doc, source.lines().count() as u32)
        })
        .collect();
    let graph = resolve::resolve(&files, &mut diags);
    (graph, diags)
}

#[test]
fn matches_golden_json() {
    let (graph, _) = build_corpus();
    let rendered = json::render(&graph);
    let path = root().join(GOLDEN);

    if std::env::var("DANKG_BLESS").is_ok() {
        fs::write(&path, &rendered).expect("could not write golden file");
        return;
    }

    let expected = fs::read_to_string(&path)
        .expect("no golden file; run with DANKG_BLESS=1 to create one");
    assert_eq!(rendered, expected, "graph JSON changed; re-bless if intended");
}

#[test]
fn is_deterministic_regardless_of_file_order() {
    let (a, _) = build_corpus();
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
    let b = resolve::resolve(&files, &mut diags);

    assert_eq!(a, b);
}

#[test]
fn containment_follows_document_structure() {
    let (graph, _) = build_corpus();
    let contains: Vec<(String, String)> = graph
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Contains)
        .map(|e| (e.from.to_string(), e.to.to_string()))
        .collect();

    assert!(contains.contains(&("project#overview".into(), "project#constraints".into())));
    assert!(contains.contains(&("notes/ideas#ideas".into(), "notes/ideas#future".into())));
    assert!(
        !contains.iter().any(|(f, _)| f == "project#code-eval"),
        "Code Eval has no subheadings"
    );
}

#[test]
fn mutual_links_are_reciprocated_across_files() {
    let (graph, _) = build_corpus();
    let recip: Vec<(String, String)> = graph
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Link && e.reciprocated)
        .map(|e| (e.from.to_string(), e.to.to_string()))
        .collect();

    // Within a file.
    assert!(recip.contains(&("project#overview".into(), "project#constraints".into())));
    assert!(recip.contains(&("project#constraints".into(), "project#overview".into())));
    // Across files, via a wikilink one way and a path link the other.
    assert!(recip.contains(&("project#overview".into(), "notes/ideas#ideas".into())));
    assert!(recip.contains(&("notes/ideas#ideas".into(), "project#overview".into())));
}

#[test]
fn one_way_link_is_not_reciprocated() {
    let (graph, _) = build_corpus();
    let edge = graph
        .edges
        .iter()
        .find(|e| e.to.to_string() == "notes/ideas#future" && e.kind == EdgeKind::Link)
        .expect("the path link into Future exists");
    assert!(!edge.reciprocated);
}

#[test]
fn dangling_link_becomes_an_unresolved_node() {
    let (graph, diags) = build_corpus();
    let node = graph.node(&NodeId::new("nowhere", "lost")).expect("placeholder exists");
    assert!(!node.resolved);
    assert!(diags.items().iter().any(|d| d.message.contains("file not found: `nowhere.md`")));
}

#[test]
fn root_escape_is_refused_and_produces_no_node() {
    let (graph, diags) = build_corpus();
    assert!(
        !graph.nodes.iter().any(|n| n.file.contains("passwd")),
        "a refused link must not create a node"
    );
    assert!(diags.items().iter().any(|d| d.message.contains("escapes the root")));
}

#[test]
fn external_urls_are_recorded_on_the_node_not_graphed() {
    let (graph, _) = build_corpus();
    let overview = graph.node(&NodeId::new("project", "overview")).unwrap();
    assert_eq!(overview.external, vec!["https://example.com".to_string()]);
    assert!(!graph.nodes.iter().any(|n| n.id.file.contains("example.com")));
}

#[test]
fn headingless_file_still_gets_a_node() {
    let (graph, _) = build_corpus();
    let node = graph.node(&NodeId::new("orphan", "orphan")).expect("file-level node");
    assert_eq!(node.level, 0);
    assert!(node.resolved);
}

#[test]
fn frontmatter_tags_reach_every_node_in_the_file() {
    let (graph, _) = build_corpus();
    for node in graph.nodes.iter().filter(|n| n.id.file == "project") {
        assert_eq!(node.tags, vec!["rust".to_string(), "graphs".to_string()], "{}", node.id);
    }
}

/// The unit tests missed a root-escape leak because their fixtures sat at depth
/// zero, where climbing out is impossible. Driving the real binary against the
/// real corpus is what caught it, so that path is covered here directly.
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
    fn refuses_to_follow_a_link_out_of_the_root() {
        let (stdout, stderr, ok) = run(&[
            "graph",
            "tests/data/corpus/project.md",
            "tests/data/corpus/notes/ideas.md",
        ]);
        assert!(ok);
        assert!(stderr.contains("escapes the root, not followed"), "stderr: {stderr}");
        assert!(!stdout.contains("passwd"), "a refused link must not reach the graph");
    }

    #[test]
    fn the_root_is_discovered_and_reported() {
        let (_, stderr, _) = run(&[
            "graph",
            "tests/data/corpus/project.md",
            "tests/data/corpus/notes/ideas.md",
        ]);
        assert!(stderr.contains("root: tests/data/corpus"), "stderr: {stderr}");
    }

    #[test]
    fn node_ids_are_root_relative_so_output_is_location_independent() {
        let (stdout, _, _) = run(&["graph", "tests/data/corpus/project.md"]);
        assert!(stdout.contains("\"id\": \"project#overview\""), "stdout: {stdout}");
        assert!(!stdout.contains("tests/data/corpus/project#"));
    }

    #[test]
    fn diagnostics_go_to_stderr_and_stdout_stays_pipeable() {
        let (stdout, stderr, _) = run(&["graph", "tests/data/corpus/project.md"]);
        assert!(stdout.starts_with('{') && stdout.trim_end().ends_with('}'));
        assert!(!stdout.contains("warn:"));
        assert!(stderr.contains("warn:"));
    }

    /// Milestone 6 closed the last hole in this list, so the property worth
    /// keeping is no longer "html is refused" but "every format the parser
    /// accepts actually renders something".
    #[test]
    fn every_accepted_format_produces_output() {
        for format in ["json", "html", "dot", "mermaid"] {
            let (stdout, stderr, ok) =
                run(&["graph", "tests/data/corpus/project.md", "--format", format]);
            assert!(ok, "{format} failed: {stderr}");
            assert!(!stdout.is_empty(), "{format} rendered nothing");
        }
        let (_, stderr, ok) = run(&["graph", "tests/data/corpus/project.md", "--format", "yaml"]);
        assert!(!ok);
        assert!(stderr.contains("unknown format"), "stderr: {stderr}");
    }

    #[test]
    fn usage_error_exits_two_with_usage_text() {
        let (_, stderr, ok) = run(&["graph"]);
        assert!(!ok);
        assert!(stderr.contains("at least one path"));
        assert!(stderr.contains("usage:"));
    }
}
