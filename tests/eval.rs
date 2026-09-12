//! End-to-end coverage for `dankg eval --if-stale` (plans/data-engineering.md,
//! Option B) against the real binary. `eval/session.rs`'s own unit tests
//! already cover `run_single`'s precheck in isolation; this file covers the
//! one thing those cannot: that a real process, with real argument parsing
//! and a real spawn, actually skips a fresh block and actually re-runs a
//! changed one.
//!
//! The corpus is built fresh in a temp directory per test, never checked
//! in, the same reason `tests/db.rs` gives: `dankg eval` writes its result
//! back into the source file it runs against.

mod binary {
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;

    struct Corpus(PathBuf);

    impl Corpus {
        fn new(md: &str) -> Corpus {
            use std::sync::atomic::{AtomicU64, Ordering};
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("dankg-eval-if-stale-test-{}-{n}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(dir.join(".dankg")).expect("scratch root");
            fs::write(dir.join(".dankg/config"), "[lang.sh]\ncommand = sh {file}\n").expect("scratch config");
            fs::write(dir.join("a.md"), md).expect("scratch fixture");
            Corpus(dir)
        }

        fn path(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }

        fn read(&self, name: &str) -> String {
            fs::read_to_string(self.path(name)).expect("scratch fixture")
        }

        fn write(&self, name: &str, content: &str) {
            fs::write(self.path(name), content).expect("scratch fixture");
        }
    }

    impl Drop for Corpus {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn run(args: &[&str]) -> (String, String, bool) {
        let out = Command::new(env!("CARGO_BIN_EXE_dankg")).args(args).output().expect("failed to run dankg");
        (String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned(), out.status.success())
    }

    #[test]
    fn if_stale_skips_a_block_already_up_to_date() {
        let corpus = Corpus::new("```sh name=a\necho hi\n```\n");
        let a = corpus.path("a.md");
        let a = a.to_str().unwrap();

        let (_, stderr, ok) = run(&["eval", a, "--block", "a", "--yes"]);
        assert!(ok, "stderr: {stderr}");
        let after_first_run = corpus.read("a.md");

        let (_, stderr, ok) = run(&["eval", a, "--block", "a", "--yes", "--if-stale"]);
        assert!(ok, "stderr: {stderr}");
        assert!(stderr.contains("already up to date"), "{stderr:?}");
        assert_eq!(corpus.read("a.md"), after_first_run, "a fresh block must not run again, so nothing should change");
    }

    #[test]
    fn if_stale_still_runs_a_block_whose_source_changed() {
        let corpus = Corpus::new("```sh name=a\necho one\n```\n");
        let a = corpus.path("a.md");
        let a = a.to_str().unwrap();

        let (_, stderr, ok) = run(&["eval", a, "--block", "a", "--yes"]);
        assert!(ok, "stderr: {stderr}");
        assert!(corpus.read("a.md").contains("one\n"), "{:?}", corpus.read("a.md"));

        corpus.write("a.md", "```sh name=a\necho two\n```\n");
        let (_, stderr, ok) = run(&["eval", a, "--block", "a", "--yes", "--if-stale"]);
        assert!(ok, "stderr: {stderr}");
        assert!(!stderr.contains("already up to date"), "{stderr:?}");
        assert!(corpus.read("a.md").contains("two\n"), "{:?}", corpus.read("a.md"));
    }

    #[test]
    fn if_stale_runs_a_block_never_run_before() {
        let corpus = Corpus::new("```sh name=a\necho hi\n```\n");
        let a = corpus.path("a.md");
        let a = a.to_str().unwrap();

        let (_, stderr, ok) = run(&["eval", a, "--block", "a", "--yes", "--if-stale"]);
        assert!(ok, "stderr: {stderr}");
        assert!(corpus.read("a.md").contains("dankg:result name=a"), "{:?}", corpus.read("a.md"));
    }
}
