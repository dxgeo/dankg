//! End-to-end coverage for `db=` blocks (decision 16, milestone 9's first
//! slice) against the real binary. `eval/run.rs` and `eval/session.rs`'s
//! own unit tests already cover `db_command_for`/`run_db`/`run_one` in
//! isolation, real `duckdb` process and all. This file covers the one
//! thing those cannot: that `dankg eval`, invoked as a real process with
//! real argument parsing and real root discovery, wires `[db.*]`
//! resolution and write-back together correctly end to end.
//!
//! The corpus is built fresh in a temp directory per test, never checked
//! in: `dankg eval` writes its result back into the source file it runs
//! against, and a fixture under `tests/data/` would end up mutated on
//! disk after every test run, unlike every other `tests/data/*-corpus`
//! fixture here, which only `check` (read-only) ever touches.

mod binary {
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;

    /// A scratch corpus: `.dankg/config` plus one `.md` file, entirely in
    /// the OS temp directory, cleaned up on drop.
    struct Corpus(PathBuf);

    impl Corpus {
        fn new(config: &str, md: &str) -> Corpus {
            // A counter, not just the pid: tests in this binary run in
            // parallel and share one pid, so a lone pid-named directory
            // lets one test's `Drop` delete another's mid-run.
            use std::sync::atomic::{AtomicU64, Ordering};
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir()
                .join(format!("dankg-db-eval-test-{}-{n}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(dir.join(".dankg")).expect("scratch root");
            fs::write(dir.join(".dankg/config"), config).expect("scratch config");
            fs::write(dir.join("warehouse.md"), md).expect("scratch fixture");
            Corpus(dir)
        }

        fn path(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }

        fn read(&self, name: &str) -> String {
            fs::read_to_string(self.path(name)).expect("scratch fixture")
        }
    }

    impl Drop for Corpus {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn run(args: &[&str]) -> (String, String, bool) {
        let out = Command::new(env!("CARGO_BIN_EXE_dankg"))
            .args(args)
            .output()
            .expect("failed to run dankg");
        (
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
            out.status.success(),
        )
    }

    #[test]
    fn eval_runs_a_db_targeted_block_against_real_duckdb_and_writes_back() {
        let corpus = Corpus::new(
            "[db.warehouse]\ncommand = duckdb -csv {db} -f {file}\npath = :memory:\n",
            "# Warehouse\n\n```sql db=warehouse name=orders\nCREATE TABLE orders AS SELECT 1 AS id, 100 AS amount;\nSELECT * FROM orders;\n```\n",
        );
        let (_, stderr, ok) = run(&[
            "eval",
            corpus.path("warehouse.md").to_str().unwrap(),
            "--block",
            "orders",
            "--yes",
        ]);
        assert!(ok, "stderr: {stderr}");
        let written = corpus.read("warehouse.md");
        assert!(written.contains("dankg:result name=orders"), "{written:?}");
        assert!(written.contains("id,amount"), "{written:?}");
        assert!(written.contains("1,100"), "{written:?}");
    }

    #[test]
    fn eval_reports_an_unconfigured_database() {
        let corpus = Corpus::new(
            "# no [db.*] section at all\n",
            "```sql db=missing name=orders\nselect 1;\n```\n",
        );
        let (_, stderr, ok) = run(&[
            "eval",
            corpus.path("warehouse.md").to_str().unwrap(),
            "--block",
            "orders",
            "--yes",
        ]);
        assert!(!ok);
        assert!(stderr.contains("db.missing"), "{stderr:?}");
    }
}
