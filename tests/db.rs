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

        fn write(&self, name: &str, content: &str) {
            fs::write(self.path(name), content).expect("scratch fixture");
        }

        fn root(&self) -> &std::path::Path {
            &self.0
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
    fn check_reports_a_freshly_run_db_block_as_fresh() {
        // Regression test for a real bug: `check_cmd` used to compute a
        // db-targeted chain's staleness hash with `command_for` alone
        // (the `[lang.*]` path), never `db_command_for`, so it either
        // silently skipped every `db=` block or verified it against the
        // wrong template. This alone would have caught it -- before the
        // fix, this either panics resolving `[lang.sql]` or never
        // increments `checked` at all.
        let corpus = Corpus::new(
            "[db.warehouse]\ncommand = duckdb -csv {db} -f {file}\npath = :memory:\n",
            "```sql db=warehouse name=setup\nCREATE TABLE orders AS SELECT 1 AS n;\n```\n",
        );
        let (_, stderr, ok) = run(&["eval", corpus.path("warehouse.md").to_str().unwrap(), "--block", "setup", "--yes"]);
        assert!(ok, "stderr: {stderr}");
        let (_, stderr, ok) = run(&["check", corpus.root().to_str().unwrap()]);
        assert!(ok, "stderr: {stderr}");
        assert!(stderr.contains("0 stale of 1 eval result"), "{stderr:?}");
    }

    #[test]
    fn eval_resolves_xdeps_table_across_files_and_check_verifies_it() {
        // Needs a real file-backed database: the producer and reader run
        // as two separate `dankg eval` process spawns, and `:memory:`
        // starts fresh every time, so the reader would never see what
        // the producer wrote.
        let db_path = std::env::temp_dir().join(format!("dankg-db-eval-test-{}-xdeps.duckdb", std::process::id()));
        let _ = fs::remove_file(&db_path);
        let corpus = Corpus::new(
            &format!(
                "[db.warehouse]\ncommand = duckdb -csv {{db}} -f {{file}}\nlist = duckdb -csv {{db}} -c \"select table_name from duckdb_tables()\"\npath = {}\n",
                db_path.to_string_lossy()
            ),
            "```sql db=warehouse name=setup\nCREATE TABLE orders AS SELECT 1 AS id, 100 AS amount;\n```\n",
        );
        corpus.write(
            "report.md",
            "```sql db=warehouse name=report xdeps=table:orders\nSELECT * FROM orders;\n```\n",
        );

        let (_, stderr, ok) = run(&["eval", corpus.path("warehouse.md").to_str().unwrap(), "--block", "setup", "--yes"]);
        assert!(ok, "stderr: {stderr}");
        let (_, stderr, ok) = run(&["eval", corpus.path("report.md").to_str().unwrap(), "--block", "report", "--yes"]);
        assert!(ok, "stderr: {stderr}");

        let (_, stderr, ok) = run(&["check", corpus.root().to_str().unwrap()]);
        assert!(ok, "both fresh right after running: {stderr}");
        assert!(stderr.contains("0 stale of 2 eval result"), "{stderr:?}");

        // Edit the producer's own source without re-running it: `report`
        // never touched the database again, but its `xdeps=table:orders`
        // recomputes `setup`'s own hash from *current* source, finds it
        // no longer matches what `setup` last recorded, and refuses --
        // staleness crossing the relation boundary, decision 36's whole
        // point.
        let warehouse = corpus.read("warehouse.md");
        let edited = warehouse.replace("AS SELECT 1 AS id, 100 AS amount", "AS SELECT 1 AS id, 200 AS amount");
        assert_ne!(warehouse, edited, "the replacement should have matched something");
        corpus.write("warehouse.md", &edited);

        let (_, stderr, ok) = run(&["check", corpus.root().to_str().unwrap()]);
        let _ = fs::remove_file(&db_path);
        assert!(!ok, "stderr: {stderr}");
        assert!(stderr.contains("stale: warehouse.md `setup`"), "{stderr:?}");
        assert!(stderr.contains("stale: report.md `report`"), "{stderr:?}");
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
