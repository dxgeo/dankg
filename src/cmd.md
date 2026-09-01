# Cmd

Every configured `{name}` template in this codebase -- `[editor] command`
today, a `[lang.*]`/`[db.*]` command once `dankg eval` reads one -- goes
through the same two steps this file owns: substitute known placeholders,
then split the result into an argv `std::process::Command` can run
directly, with no shell in between. Splitting quoted arguments by hand
rather than reaching for a shell means `code --at="a b":10` never risks
whatever a shell would do with the rest of the string; the tradeoff, spelled
out in each function's own doc comment below, is no `|`/`&&`/`$VAR`
support at all.

```rust name=module_doc path=cmd.rs
//! Command templates: `{name}` placeholder substitution and a hand-rolled
//! argv split, shared by anything that spawns a configured external command
//! -- the TUI's editor handoff today, `eval`'s `[lang.*]`/`[db.*]` commands
//! once milestone 8 exists. Deliberately does not spawn anything itself: a
//! caller waiting on an interactive editor and a caller capturing eval
//! output configure `Command`'s stdio completely differently, and this
//! module has no opinion about which.
```

`substitute` never treats an unknown `{name}` as an error -- see why in its
own doc comment, which is the reason worth keeping in the code itself
rather than only here: a reader of the generated `cmd.rs` alone, with no
markdown in hand, still needs to know a typo silently survives rather than
failing loudly.

```rust name=substitute path=cmd.rs
/// Replaces every `{name}` in `template` with its value from `vars`. An
/// unknown `{name}` is left in place rather than silently dropped -- a
/// mistyped config key should show up as a literal `{line}` in the spawned
/// command, not vanish. Assumes a value never itself contains `{name}`-
/// shaped text; nothing here guards against that.
pub fn substitute(template: &str, vars: &[(&str, &str)]) -> String {
    let mut out = template.to_string();
    for (name, value) in vars {
        out = out.replace(&format!("{{{name}}}"), value);
    }
    out
}
```

```rust name=split path=cmd.rs
/// Splits a command string into a program and its arguments, honouring
/// single and double quotes so a path or argument containing a space can be
/// written down. No escape sequences beyond that and no shell features
/// (`|`, `&&`, `$VAR`) -- this is run directly via [`std::process::Command`],
/// never handed to a shell, so none of that would do anything anyway.
pub fn split(command: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut in_word = false;
    let mut quote: Option<char> = None;

    for ch in command.chars() {
        match quote {
            Some(q) if ch == q => quote = None,
            Some(_) => current.push(ch),
            None if ch == '\'' || ch == '"' => {
                quote = Some(ch);
                in_word = true;
            }
            None if ch.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut current));
                    in_word = false;
                }
            }
            None => {
                current.push(ch);
                in_word = true;
            }
        }
    }
    if in_word {
        words.push(current);
    }
    words
}
```

```rust name=build path=cmd.rs
/// [`substitute`] then [`split`], the combination every caller actually
/// wants. `None` when there is no program to run -- an empty or
/// whitespace-only template, including one that started non-empty but
/// substituted down to nothing.
pub fn build(template: &str, vars: &[(&str, &str)]) -> Option<Vec<String>> {
    let argv = split(&substitute(template, vars));
    if argv.is_empty() {
        None
    } else {
        Some(argv)
    }
}
```

## Tests

```rust name=tests path=cmd.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitutes_known_placeholders() {
        assert_eq!(
            substitute("code -g {file}:{line}", &[("file", "a.md"), ("line", "10")]),
            "code -g a.md:10"
        );
    }

    #[test]
    fn an_unknown_placeholder_is_left_in_place() {
        assert_eq!(substitute("echo {nope}", &[("file", "a.md")]), "echo {nope}");
    }

    #[test]
    fn split_handles_bare_words_and_both_quote_styles() {
        assert_eq!(split("code -g"), vec!["code", "-g"]);
        assert_eq!(split("sh -c 'echo hi'"), vec!["sh", "-c", "echo hi"]);
        assert_eq!(split(r#"code "a b""#), vec!["code", "a b"]);
    }

    #[test]
    fn a_quote_can_sit_inside_an_otherwise_unquoted_word() {
        assert_eq!(split(r#"code --at="a b":10"#), vec!["code", "--at=a b:10"]);
    }

    #[test]
    fn extra_whitespace_does_not_produce_empty_words() {
        assert_eq!(split("  code   -g  "), vec!["code", "-g"]);
    }

    #[test]
    fn build_combines_substitution_and_split() {
        assert_eq!(
            build("code -g {file}:{line}", &[("file", "notes.md"), ("line", "12")]),
            Some(vec!["code".to_string(), "-g".to_string(), "notes.md:12".to_string()])
        );
    }

    #[test]
    fn an_empty_or_blank_template_builds_to_nothing() {
        assert_eq!(build("", &[]), None);
        assert_eq!(build("   ", &[]), None);
    }
}
```
