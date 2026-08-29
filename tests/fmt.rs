//! `dankg fmt` round-tripping.
//!
//! Two properties, and between them they exercise the parser harder than the
//! conformance suite does -- every construct has to survive a full
//! parse-print-parse cycle rather than merely producing the right HTML.
//!
//! - *Idempotence*: formatting twice equals formatting once.
//! - *Fixed point*: already-formatted input comes back unchanged.
//!
//! The corpus fixtures are committed in normal form, so they must satisfy both.

mod support;

use dankg::diag::Diags;
use dankg::md::{fmt, Document};
use std::fs;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Format, re-parse, and format again. Reports the first property that breaks.
fn round_trip(name: &str, source: &str) -> Result<String, String> {
    let mut diags = Diags::new(name);
    let doc = Document::parse(source, &mut diags);
    let once = fmt::format(&doc);

    let mut again = Diags::new(name);
    let reparsed = Document::parse(&once, &mut again);
    let twice = fmt::format(&reparsed);

    if twice != once {
        return Err(format!(
            "{name}: not idempotent\n-- input --\n{source}\n-- once --\n{once}\n-- twice --\n{twice}"
        ));
    }
    if fmt::without_lines(&reparsed) != fmt::without_lines(&doc) {
        return Err(format!(
            "{name}: meaning changed\n-- input --\n{source}\n-- once --\n{once}\n\
             -- before --\n{:#?}\n-- after --\n{:#?}",
            fmt::without_lines(&doc),
            fmt::without_lines(&reparsed)
        ));
    }
    Ok(once)
}

fn corpus_files() -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect(&root().join("tests/data/corpus"), &mut out);
    out.sort();
    out
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("corpus directory is missing").flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|e| e == "md") {
            out.push(path);
        }
    }
}

#[test]
fn corpus_round_trips() {
    for path in corpus_files() {
        let source = fs::read_to_string(&path).unwrap();
        round_trip(&path.display().to_string(), &source).unwrap();
    }
}

#[test]
fn corpus_fixtures_are_already_in_normal_form() {
    for path in corpus_files() {
        let source = fs::read_to_string(&path).unwrap();
        let formatted = round_trip(&path.display().to_string(), &source).unwrap();
        assert_eq!(
            formatted,
            source,
            "{} is not in normal form; run `cargo run -- fmt` on the corpus",
            path.display()
        );
    }
}

/// Spec inputs that cannot round-trip, and will not until indented code blocks
/// are inside the subset.
///
/// All four are the same shape: an indented block that the parser keeps as
/// `Passthrough` sits next to a list whose items were indented more deeply than
/// normal form allows. Normalizing the list narrows its content column, and the
/// indented block -- which the formatter is obliged to re-emit byte for byte --
/// is then deep enough to be swallowed as a continuation line.
///
/// `dankg fmt` does not corrupt these files: `verify` catches the change and
/// refuses to write. They are listed rather than counted so that fixing one
/// fails this test and prompts its removal.
const KNOWN_UNFORMATTABLE: &[u32] = &[257, 291, 312, 313];

#[test]
fn spec_inputs_round_trip() {
    let text = fs::read_to_string(root().join("tests/data/commonmark/spec.json"))
        .expect("vendored spec.json is missing");
    let spec = support::json::parse(&text);

    let mut failed: Vec<u32> = Vec::new();
    let mut reports: Vec<String> = Vec::new();
    for case in spec.as_array() {
        let example = case.num("example") as u32;
        if let Err(why) = round_trip(&format!("example {example}"), case.str("markdown")) {
            failed.push(example);
            if !KNOWN_UNFORMATTABLE.contains(&example) {
                reports.push(why);
            }
        }
    }

    assert!(
        reports.is_empty(),
        "{} spec input(s) newly fail to round trip:\n\n{}",
        reports.len(),
        reports.join("\n\n")
    );
    assert_eq!(
        failed, KNOWN_UNFORMATTABLE,
        "the known-unformattable list is stale; update KNOWN_UNFORMATTABLE"
    );
}

/// `verify` is the safety net that keeps a formatter bug off the disk, so the
/// one input family known to trip it must actually trip it.
#[test]
fn verify_rejects_a_document_it_would_change() {
    let source = "  1.  A paragraph\n    with two lines.\n";
    let mut diags = Diags::new("t.md");
    let doc = Document::parse(source, &mut diags);
    let formatted = fmt::format(&doc);
    assert!(fmt::verify(&doc, &formatted).is_err());
}

#[test]
fn verify_accepts_ordinary_documents() {
    for path in corpus_files() {
        let source = fs::read_to_string(&path).unwrap();
        let mut diags = Diags::new("t.md");
        let doc = Document::parse(&source, &mut diags);
        let formatted = fmt::format(&doc);
        fmt::verify(&doc, &formatted).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    }
}

/// `fmt` writes to files, so the parts that only exist at the process boundary
/// -- in-place rewriting, the `--check` exit code, the refusal to write -- are
/// covered against the real binary rather than the library.
mod binary {
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;

    /// A scratch file that cleans up after itself.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str, contents: &str) -> Scratch {
            let path = std::env::temp_dir()
                .join(format!("dankg-fmt-{}-{name}", std::process::id()));
            fs::write(&path, contents).expect("scratch file");
            Scratch(path)
        }

        fn arg(&self) -> String {
            self.0.display().to_string()
        }

        fn read(&self) -> String {
            fs::read_to_string(&self.0).expect("scratch file")
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

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
    fn rewrites_a_file_in_place() {
        let f = Scratch::new("messy.md", "##   Title   ##
*  one
*  two
");
        let (_, _, ok) = run(&["fmt", &f.arg()]);
        assert!(ok);
        assert_eq!(f.read(), "## Title

* one
* two
");
    }

    #[test]
    fn check_reports_on_stdout_and_writes_nothing() {
        let source = "#  Title
";
        let f = Scratch::new("check.md", source);
        let (stdout, _, ok) = run(&["fmt", "--check", &f.arg()]);
        assert!(!ok, "--check must exit non-zero when a file would change");
        assert_eq!(stdout.trim_end(), f.arg());
        assert_eq!(f.read(), source, "--check must not write");
    }

    #[test]
    fn check_is_quiet_and_succeeds_on_normal_form() {
        let (stdout, _, ok) = run(&["fmt", "--check", "tests/data/corpus/project.md"]);
        assert!(ok);
        assert!(stdout.is_empty(), "stdout: {stdout}");
    }

    #[test]
    fn refuses_to_write_what_it_cannot_verify() {
        let source = "  1.  A paragraph
    with two lines.
";
        let f = Scratch::new("unverifiable.md", source);
        let (_, stderr, ok) = run(&["fmt", &f.arg()]);
        assert!(!ok);
        assert!(stderr.contains("left unchanged"), "stderr: {stderr}");
        assert_eq!(f.read(), source);
    }
}
