//! The dependency DAG: which named top-level code blocks a target needs,
//! in the order `run.rs` must concatenate them.
//!
//! Deliberately scoped to *top-level* blocks only -- `doc.blocks` directly,
//! never recursing into a list the way `Document::named_blocks` does for
//! the graph. `result.rs`'s write-back needs a block's position in that
//! same flat list to find (or place) its result marker right after it;
//! keeping eval's whole view of "what blocks exist" to that flat list
//! means there is only one notion of "the code block named `index`" to
//! keep in step, rather than two. A named block nested in a list is
//! invisible to eval entirely -- not planned, not a valid `deps=` target --
//! a documented gap rather than a silently different rule for write-back
//! versus everything else.

use crate::md::{Block, Document, InfoString};
use std::collections::HashSet;
use std::fmt;

/// One block eval can run: enough of `Block::Code` and its `InfoString` to
/// plan, execute and locate for write-back, borrowed straight from the
/// parsed `Document` rather than copied.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockRef<'a> {
    /// Position in `doc.blocks` -- what `result::locate_existing` uses to
    /// look immediately after this block for an existing result marker.
    pub index: usize,
    pub name: &'a str,
    pub lang: Option<&'a str>,
    pub source: &'a str,
    pub line: u32,
    pub end_line: u32,
    pub deps: Vec<&'a str>,
    pub timeout: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlanError {
    /// Two top-level blocks share a name -- `deps=` and write-back would
    /// not know which one they meant.
    DuplicateName(String),
    /// The requested `--block` target does not exist among this file's
    /// top-level named blocks.
    UnknownTarget(String),
    /// `block`'s `deps=` names something that is not a top-level named
    /// block in this file.
    UnknownDep { block: String, dep: String },
    /// `deps=` forms a cycle; `path` names it in the order discovered,
    /// ending back where it started.
    Cycle(Vec<String>),
    /// `block` declares a different `lang` than `target`, the block whose
    /// chain it was pulled into -- refused because the whole chain is
    /// concatenated into one file and run through `target`'s one
    /// interpreter, and code in the wrong language would just fail there.
    MixedLang { target: String, target_lang: String, block: String, block_lang: String },
}

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlanError::DuplicateName(name) => {
                write!(f, "two top-level blocks are both named `{name}`")
            }
            PlanError::UnknownTarget(name) => {
                write!(f, "no top-level block is named `{name}`")
            }
            PlanError::UnknownDep { block, dep } => {
                write!(f, "`{block}` depends on `{dep}`, which is not a top-level named block")
            }
            PlanError::Cycle(path) => {
                write!(f, "dependency cycle: {}", path.join(" -> "))
            }
            PlanError::MixedLang { target, target_lang, block, block_lang } => {
                write!(
                    f,
                    "`{block}` is `{block_lang}` but `{target}`, whose chain it is part of, is `{target_lang}`; \
                     one interpreter runs the whole concatenated file"
                )
            }
        }
    }
}

/// Every top-level named code block in `doc`, in document order. What
/// `plan_for`/`plan_all` search and `main` reports "no named blocks" against
/// when it is empty.
pub fn top_level_blocks(doc: &Document) -> Vec<BlockRef<'_>> {
    doc.blocks
        .iter()
        .enumerate()
        .filter_map(|(index, b)| {
            let Block::Code { info, text, line, end_line, .. } = b else { return None };
            block_ref(index, info, text, *line, *end_line)
        })
        .collect()
}

fn block_ref<'a>(
    index: usize,
    info: &'a InfoString,
    text: &'a str,
    line: u32,
    end_line: u32,
) -> Option<BlockRef<'a>> {
    let name = info.name()?;
    Some(BlockRef {
        index,
        name,
        lang: info.lang.as_deref(),
        source: text,
        line,
        end_line,
        deps: info.deps(),
        timeout: info.timeout(),
    })
}

/// `target`'s transitive `deps=`, topologically ordered, `target` itself
/// last -- exactly the list `run.rs` concatenates into one file and spawns
/// once. Every block in the result shares the language of `blocks`'
/// occurrence of `target`, if it has one at all: mixing an unstated
/// dependency in a different language into the one interpreter that will
/// run the whole concatenated file is far more likely a mistake than an
/// intentional multi-language pipeline, so it is refused rather than run.
pub fn plan_for<'a>(blocks: &'a [BlockRef<'a>], target: &str) -> Result<Vec<BlockRef<'a>>, PlanError> {
    check_unique_names(blocks)?;
    if !blocks.iter().any(|b| b.name == target) {
        return Err(PlanError::UnknownTarget(target.to_string()));
    }
    let mut order = Vec::new();
    let mut done = HashSet::new();
    let mut stack = Vec::new();
    visit(blocks, target, &mut stack, &mut done, &mut order)?;
    check_consistent_lang(&order, target)?;
    Ok(order)
}

/// Every top-level named block, each preceded by its own transitive
/// `deps=`, in one document-consistent order -- `--all`'s per-block runs
/// happen in this order so that, by the time a block that depends on
/// another is evaluated, the one it depends on has already been (see
/// architecture.org's Open Questions: this is `--all`'s answer to
/// cross-block, same-file ordering; it says nothing about cross-file
/// ordering, which `eval` does not attempt since it only ever opens one
/// file). Unlike `plan_for`, this does not enforce a single language
/// across the whole file -- each block still only mixes languages with its
/// own dependency chain, checked independently as `--all` reaches it.
pub fn plan_all<'a>(blocks: &'a [BlockRef<'a>]) -> Result<Vec<Vec<BlockRef<'a>>>, PlanError> {
    check_unique_names(blocks)?;
    let mut order = Vec::new();
    let mut done = HashSet::new();
    let mut stack = Vec::new();
    for b in blocks {
        visit(blocks, b.name, &mut stack, &mut done, &mut order)?;
    }
    order
        .iter()
        .map(|b| {
            let mut stack = Vec::new();
            let mut done = HashSet::new();
            let mut chain = Vec::new();
            visit(blocks, b.name, &mut stack, &mut done, &mut chain)?;
            check_consistent_lang(&chain, b.name)?;
            Ok(chain)
        })
        .collect()
}

fn check_unique_names(blocks: &[BlockRef]) -> Result<(), PlanError> {
    let mut seen: HashSet<&str> = HashSet::new();
    for b in blocks {
        if !seen.insert(b.name) {
            return Err(PlanError::DuplicateName(b.name.to_string()));
        }
    }
    Ok(())
}

fn check_consistent_lang(chain: &[BlockRef], target: &str) -> Result<(), PlanError> {
    let Some(target_lang) = chain.iter().find(|b| b.name == target).and_then(|b| b.lang) else {
        return Ok(());
    };
    for b in chain {
        if let Some(lang) = b.lang
            && lang != target_lang
            && b.name != target
        {
            return Err(PlanError::MixedLang {
                target: target.to_string(),
                target_lang: target_lang.to_string(),
                block: b.name.to_string(),
                block_lang: lang.to_string(),
            });
        }
    }
    Ok(())
}

/// Depth-first, dependencies before dependents, recording `name` only once
/// even when several blocks share it as a dependency. `stack` is the path
/// from the outermost call to here, in `deps=` declaration order -- both
/// what proves a cycle and what names it in the error.
fn visit<'a>(
    blocks: &'a [BlockRef<'a>],
    name: &str,
    stack: &mut Vec<String>,
    done: &mut HashSet<String>,
    order: &mut Vec<BlockRef<'a>>,
) -> Result<(), PlanError> {
    if done.contains(name) {
        return Ok(());
    }
    if let Some(pos) = stack.iter().position(|s| s == name) {
        let mut cycle = stack[pos..].to_vec();
        cycle.push(name.to_string());
        return Err(PlanError::Cycle(cycle));
    }
    let Some(block) = blocks.iter().find(|b| b.name == name) else {
        // The caller distinguishes "target itself is unknown" (checked
        // before `visit` is ever called) from "someone's dep is unknown".
        return Err(PlanError::UnknownDep { block: String::new(), dep: name.to_string() });
    };
    stack.push(name.to_string());
    for dep in &block.deps {
        visit(blocks, dep, stack, done, order).map_err(|e| match e {
            PlanError::UnknownDep { block: b, dep } if b.is_empty() => {
                PlanError::UnknownDep { block: name.to_string(), dep }
            }
            other => other,
        })?;
    }
    stack.pop();
    done.insert(name.to_string());
    order.push(block.clone());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::Diags;

    fn doc(src: &str) -> Document {
        Document::parse(src, &mut Diags::new("t.md"))
    }

    #[test]
    fn a_single_block_with_no_deps_plans_to_itself() {
        let d = doc("```python name=only\nprint(1)\n```\n");
        let blocks = top_level_blocks(&d);
        let plan = plan_for(&blocks, "only").unwrap();
        assert_eq!(plan.iter().map(|b| b.name).collect::<Vec<_>>(), vec!["only"]);
    }

    #[test]
    fn transitive_deps_come_before_the_target_in_declaration_order() {
        let d = doc(
            "```python name=setup\na=1\n```\n\n```python name=fetch deps=setup\nb=2\n```\n\n```python name=index deps=fetch\nc=3\n```\n",
        );
        let blocks = top_level_blocks(&d);
        let plan = plan_for(&blocks, "index").unwrap();
        assert_eq!(plan.iter().map(|b| b.name).collect::<Vec<_>>(), vec!["setup", "fetch", "index"]);
    }

    #[test]
    fn a_shared_dependency_appears_only_once() {
        let d = doc(
            "```sh name=setup\n:\n```\n\n```sh name=a deps=setup\n:\n```\n\n```sh name=b deps=setup\n:\n```\n\n```sh name=top deps=a,b\n:\n```\n",
        );
        let blocks = top_level_blocks(&d);
        let plan = plan_for(&blocks, "top").unwrap();
        assert_eq!(plan.iter().map(|b| b.name).collect::<Vec<_>>(), vec!["setup", "a", "b", "top"]);
    }

    #[test]
    fn an_unknown_target_is_reported() {
        let d = doc("```sh name=only\n:\n```\n");
        let blocks = top_level_blocks(&d);
        assert_eq!(plan_for(&blocks, "nope").unwrap_err(), PlanError::UnknownTarget("nope".into()));
    }

    #[test]
    fn an_unknown_dependency_names_both_ends() {
        let d = doc("```sh name=top deps=ghost\n:\n```\n");
        let blocks = top_level_blocks(&d);
        assert_eq!(
            plan_for(&blocks, "top").unwrap_err(),
            PlanError::UnknownDep { block: "top".into(), dep: "ghost".into() }
        );
    }

    #[test]
    fn a_two_block_cycle_is_reported_not_silently_broken() {
        let d = doc("```sh name=a deps=b\n:\n```\n\n```sh name=b deps=a\n:\n```\n");
        let blocks = top_level_blocks(&d);
        let err = plan_for(&blocks, "a").unwrap_err();
        assert!(matches!(err, PlanError::Cycle(_)), "{err:?}");
    }

    #[test]
    fn a_self_dependency_is_a_one_element_cycle() {
        let d = doc("```sh name=a deps=a\n:\n```\n");
        let blocks = top_level_blocks(&d);
        assert!(matches!(plan_for(&blocks, "a").unwrap_err(), PlanError::Cycle(_)));
    }

    #[test]
    fn duplicate_names_are_refused_even_when_unrelated_to_the_target() {
        let d = doc("```sh name=x\n:\n```\n\n```sh name=x\n:\n```\n");
        let blocks = top_level_blocks(&d);
        assert_eq!(plan_for(&blocks, "x").unwrap_err(), PlanError::DuplicateName("x".into()));
    }

    #[test]
    fn a_named_block_nested_in_a_list_is_invisible_to_eval() {
        let d = doc("- ```sh name=hidden\n  :\n  ```\n");
        assert!(top_level_blocks(&d).is_empty());
    }

    #[test]
    fn a_dependency_in_a_different_language_is_refused() {
        let d = doc("```sh name=setup\n:\n```\n\n```python name=top deps=setup\n:\n```\n");
        let blocks = top_level_blocks(&d);
        let err = plan_for(&blocks, "top").unwrap_err();
        assert_eq!(
            err,
            PlanError::MixedLang {
                target: "top".into(),
                target_lang: "python".into(),
                block: "setup".into(),
                block_lang: "sh".into(),
            }
        );
    }

    #[test]
    fn plan_all_runs_a_dependency_before_whatever_declares_it_first() {
        // `b` is declared before `a` in the file, but `b` depends on `a`, so
        // `--all` must still run `a` first -- topological order wins over
        // declaration order.
        let d = doc("```sh name=b deps=a\n:\n```\n\n```sh name=a\n:\n```\n");
        let blocks = top_level_blocks(&d);
        let chains = plan_all(&blocks).unwrap();
        let targets: Vec<&str> = chains.iter().map(|c| c.last().unwrap().name).collect();
        assert_eq!(targets, vec!["a", "b"]);
        assert_eq!(chains[1].iter().map(|b| b.name).collect::<Vec<_>>(), vec!["a", "b"], "b's own chain still includes its dependency");
    }

    #[test]
    fn plan_all_reports_the_same_errors_as_plan_for() {
        let d = doc("```sh name=a deps=ghost\n:\n```\n");
        let blocks = top_level_blocks(&d);
        assert!(matches!(plan_all(&blocks).unwrap_err(), PlanError::UnknownDep { .. }));
    }
}
