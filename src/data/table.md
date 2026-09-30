# Data table

`dankg weave` renders two kinds of table (plan-weave.md, decisions 42
and 45): a GFM pipe table, already structured by `md/block.rs`, and a
fenced code block tagged `csv`, `tsv`, or `json` -- structured text
that has never been parsed as anything but a code block's own opaque
body. This module is the second path's reader. Both paths converge on
one shape, `TableData`, before either renderer (`render::typst`,
`render::weave_html`) ever sees a table -- neither renderer needs to
know or care which of the two produced it.

Two hand-rolled readers, no parsing crate for either -- the same
zero-dependency discipline `eval/sql.rs`'s own scanner already follows
for exactly the same reason.

```rust name=module_doc path=data/table.rs
//! Table readers for weave's fenced `csv`/`tsv`/`json` blocks. Hand-rolled,
//! no parsing crate for either -- decision 1.
//!
//! Both converge on `TableData`, the one shape `render::typst` and
//! `render::weave_html` consume regardless of which reader produced it.

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TableData {
    pub header: Vec<String>,
    pub rows: Vec<Vec<String>>,
}
```

## Delimited text: CSV and TSV

One reader for both -- only the delimiter differs. Quoted fields
(`"..."`, `""` an escaped quote inside one) may contain the delimiter,
a newline, or a bare `"` doubled. The first record is always the
header. Every byte sequence is *some* valid delimited text -- a stray
unescaped quote is just more field content -- so this never fails.
There is no malformed-CSV case to report.

```rust name=from_delimited path=data/table.rs
pub fn from_delimited(text: &str, delim: char) -> TableData {
    let mut records = parse_records(text, delim).into_iter();
    let header = records.next().unwrap_or_default();
    TableData { header, rows: records.collect() }
}

/// A record ends at an unquoted newline. `line_has_data` is what tells a
/// genuine trailing record (input with no final newline) apart from the
/// phantom empty one a trailing newline would otherwise produce.
fn parse_records(text: &str, delim: char) -> Vec<Vec<String>> {
    let chars: Vec<char> = text.chars().collect();
    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut line_has_data = false;
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        if in_quotes {
            match c {
                '"' if chars.get(i + 1) == Some(&'"') => {
                    field.push('"');
                    i += 2;
                }
                '"' => {
                    in_quotes = false;
                    i += 1;
                }
                _ => {
                    field.push(c);
                    i += 1;
                }
            }
            continue;
        }
        line_has_data = true;
        match c {
            '"' if field.is_empty() => {
                in_quotes = true;
                i += 1;
            }
            '\r' => i += 1,
            '\n' => {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
                line_has_data = false;
                i += 1;
            }
            d if d == delim => {
                record.push(std::mem::take(&mut field));
                i += 1;
            }
            _ => {
                field.push(c);
                i += 1;
            }
        }
    }
    if line_has_data {
        record.push(field);
        records.push(record);
    }
    records
}
```

## JSON

A small hand-rolled recursive-descent parser -- null/bool/number
(kept as its own original text rather than parsed to a float, so a
large or precise number is never silently rounded)/string/array/object --
then a second pass that recognizes exactly the two shapes a table can
come from. Anything else returns `None`; the caller (`weave`) falls
back to an ordinary code block, the same graceful degradation an
unconfigured `[weave.pdf] command` already gets.

```rust name=json_value path=data/table.rs
#[derive(Debug, Clone, PartialEq)]
enum Json {
    Null,
    Bool(bool),
    Number(String),
    Str(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

struct JsonParser {
    chars: Vec<char>,
    pos: usize,
}

impl JsonParser {
    fn new(text: &str) -> Self {
        JsonParser { chars: text.chars().collect(), pos: 0 }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t' | '\n' | '\r')) {
            self.pos += 1;
        }
    }

    /// The whole input must be exactly one value, no trailing garbage.
    fn parse(&mut self) -> Option<Json> {
        self.skip_ws();
        let v = self.value()?;
        self.skip_ws();
        if self.pos != self.chars.len() {
            return None;
        }
        Some(v)
    }

    fn value(&mut self) -> Option<Json> {
        self.skip_ws();
        match self.peek()? {
            '"' => self.string().map(Json::Str),
            '{' => self.object(),
            '[' => self.array(),
            't' => self.literal("true", Json::Bool(true)),
            'f' => self.literal("false", Json::Bool(false)),
            'n' => self.literal("null", Json::Null),
            c if c == '-' || c.is_ascii_digit() => self.number(),
            _ => None,
        }
    }

    fn literal(&mut self, word: &str, v: Json) -> Option<Json> {
        for expect in word.chars() {
            if self.bump()? != expect {
                return None;
            }
        }
        Some(v)
    }

    fn string(&mut self) -> Option<String> {
        if self.bump()? != '"' {
            return None;
        }
        let mut s = String::new();
        loop {
            match self.bump()? {
                '"' => return Some(s),
                '\\' => match self.bump()? {
                    '"' => s.push('"'),
                    '\\' => s.push('\\'),
                    '/' => s.push('/'),
                    'n' => s.push('\n'),
                    't' => s.push('\t'),
                    'r' => s.push('\r'),
                    'b' => s.push('\u{8}'),
                    'f' => s.push('\u{c}'),
                    'u' => {
                        let mut code = 0u32;
                        for _ in 0..4 {
                            code = code * 16 + self.bump()?.to_digit(16)?;
                        }
                        s.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
                    }
                    _ => return None,
                },
                c => s.push(c),
            }
        }
    }

    fn number(&mut self) -> Option<Json> {
        let start = self.pos;
        if self.peek() == Some('-') {
            self.pos += 1;
        }
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.pos += 1;
        }
        if self.peek() == Some('.') {
            self.pos += 1;
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.pos += 1;
            }
        }
        if matches!(self.peek(), Some('e' | 'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some('+' | '-')) {
                self.pos += 1;
            }
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.pos += 1;
            }
        }
        if self.pos == start {
            return None;
        }
        Some(Json::Number(self.chars[start..self.pos].iter().collect()))
    }

    fn array(&mut self) -> Option<Json> {
        self.bump();
        self.skip_ws();
        let mut items = Vec::new();
        if self.peek() == Some(']') {
            self.bump();
            return Some(Json::Array(items));
        }
        loop {
            items.push(self.value()?);
            self.skip_ws();
            match self.bump()? {
                ',' => self.skip_ws(),
                ']' => break,
                _ => return None,
            }
        }
        Some(Json::Array(items))
    }

    fn object(&mut self) -> Option<Json> {
        self.bump();
        self.skip_ws();
        let mut fields = Vec::new();
        if self.peek() == Some('}') {
            self.bump();
            return Some(Json::Object(fields));
        }
        loop {
            self.skip_ws();
            let key = self.string()?;
            self.skip_ws();
            if self.bump()? != ':' {
                return None;
            }
            fields.push((key, self.value()?));
            self.skip_ws();
            match self.bump()? {
                ',' => self.skip_ws(),
                '}' => break,
                _ => return None,
            }
        }
        Some(Json::Object(fields))
    }
}
```

`from_json` recognizes exactly two top-level shapes: an array of
objects (the first element's own keys, in declared order, become the
header; a later element's missing key renders empty, an extra key is
ignored) or an array of arrays (the first inner array is the header
row, matching `from_delimited`'s own convention -- the two readers
should feel like one convention wearing two syntaxes, not two
unrelated ones). A scalar cell renders as its own text. A cell that is
itself an array or object -- a shape neither reader flattens further --
is kept, not dropped, the same "kept, uninterpreted" precedent
`Block::Passthrough` already set: it renders as its own compact JSON
text rather than silently disappearing.

```rust name=from_json path=data/table.rs
pub fn from_json(text: &str) -> Option<TableData> {
    let Json::Array(items) = JsonParser::new(text).parse()? else { return None };
    if items.is_empty() {
        return Some(TableData::default());
    }

    if items.iter().all(|v| matches!(v, Json::Object(_))) {
        let Json::Object(first_fields) = &items[0] else { unreachable!() };
        let header: Vec<String> = first_fields.iter().map(|(k, _)| k.clone()).collect();
        let rows = items
            .iter()
            .map(|item| {
                let Json::Object(fields) = item else { unreachable!() };
                header
                    .iter()
                    .map(|h| fields.iter().find(|(k, _)| k == h).map_or(String::new(), |(_, v)| scalar(v)))
                    .collect()
            })
            .collect();
        return Some(TableData { header, rows });
    }

    if items.iter().all(|v| matches!(v, Json::Array(_))) {
        let mut rest = items.into_iter();
        let Some(Json::Array(first)) = rest.next() else { unreachable!() };
        let header = first.iter().map(scalar).collect();
        let rows = rest
            .map(|row| {
                let Json::Array(cells) = row else { unreachable!() };
                cells.iter().map(scalar).collect()
            })
            .collect();
        return Some(TableData { header, rows });
    }

    None
}

fn scalar(v: &Json) -> String {
    match v {
        Json::Null => String::new(),
        Json::Bool(b) => b.to_string(),
        Json::Number(n) => n.clone(),
        Json::Str(s) => s.clone(),
        Json::Array(_) | Json::Object(_) => compact(v),
    }
}

/// A minimal, valid JSON re-serialization -- just enough to keep a nested
/// cell's content visible rather than losing it.
fn compact(v: &Json) -> String {
    match v {
        Json::Null => "null".to_string(),
        Json::Bool(b) => b.to_string(),
        Json::Number(n) => n.clone(),
        Json::Str(s) => format!("\"{}\"", escape_json(s)),
        Json::Array(items) => format!("[{}]", items.iter().map(compact).collect::<Vec<_>>().join(",")),
        Json::Object(fields) => {
            let parts: Vec<String> =
                fields.iter().map(|(k, v)| format!("\"{}\":{}", escape_json(k), compact(v))).collect();
            format!("{{{}}}", parts.join(","))
        }
    }
}

fn escape_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            _ => out.push(c),
        }
    }
    out
}
```

## Tests

```rust name=tests path=data/table.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_csv() {
        let t = from_delimited("a,b,c\n1,2,3\n", ',');
        assert_eq!(t.header, vec!["a", "b", "c"]);
        assert_eq!(t.rows, vec![vec!["1", "2", "3"]]);
    }

    #[test]
    fn tsv_uses_tab_as_delimiter() {
        let t = from_delimited("a\tb\n1\t2\n", '\t');
        assert_eq!(t.header, vec!["a", "b"]);
        assert_eq!(t.rows, vec![vec!["1", "2"]]);
    }

    #[test]
    fn quoted_field_may_contain_the_delimiter_and_a_newline() {
        let t = from_delimited("a,b\n\"x,y\",\"line1\nline2\"\n", ',');
        assert_eq!(t.rows, vec![vec!["x,y", "line1\nline2"]]);
    }

    #[test]
    fn doubled_quote_is_an_escaped_quote() {
        let t = from_delimited("a\n\"she said \"\"hi\"\"\"\n", ',');
        assert_eq!(t.rows, vec![vec!["she said \"hi\""]]);
    }

    #[test]
    fn no_trailing_newline_still_captures_the_last_row() {
        let t = from_delimited("a,b\n1,2", ',');
        assert_eq!(t.rows, vec![vec!["1", "2"]]);
    }

    #[test]
    fn a_trailing_newline_does_not_add_a_phantom_row() {
        let t = from_delimited("a,b\n1,2\n", ',');
        assert_eq!(t.rows.len(), 1);
    }

    #[test]
    fn empty_input_is_an_empty_table() {
        assert_eq!(from_delimited("", ','), TableData::default());
    }

    #[test]
    fn json_array_of_objects() {
        let t = from_json(r#"[{"a":1,"b":"x"},{"a":2,"b":"y"}]"#).unwrap();
        assert_eq!(t.header, vec!["a", "b"]);
        assert_eq!(t.rows, vec![vec!["1", "x"], vec!["2", "y"]]);
    }

    #[test]
    fn json_object_missing_a_key_renders_empty_and_an_extra_key_is_ignored() {
        let t = from_json(r#"[{"a":1,"b":2},{"a":3,"c":4}]"#).unwrap();
        assert_eq!(t.header, vec!["a", "b"]);
        assert_eq!(t.rows, vec![vec!["1", "2"], vec!["3", ""]]);
    }

    #[test]
    fn json_array_of_arrays_first_row_is_the_header() {
        let t = from_json(r#"[["a","b"],[1,2],[3,4]]"#).unwrap();
        assert_eq!(t.header, vec!["a", "b"]);
        assert_eq!(t.rows, vec![vec!["1", "2"], vec!["3", "4"]]);
    }

    #[test]
    fn empty_json_array_is_an_empty_table() {
        assert_eq!(from_json("[]").unwrap(), TableData::default());
    }

    #[test]
    fn a_bare_object_is_not_a_table() {
        assert_eq!(from_json(r#"{"a":1}"#), None);
    }

    #[test]
    fn a_scalar_is_not_a_table() {
        assert_eq!(from_json("42"), None);
    }

    #[test]
    fn mixed_array_of_objects_and_arrays_is_not_a_table() {
        assert_eq!(from_json(r#"[{"a":1},["b"]]"#), None);
    }

    #[test]
    fn malformed_json_is_not_a_table() {
        assert_eq!(from_json("[1, 2,"), None);
        assert_eq!(from_json("not json"), None);
    }

    #[test]
    fn a_nested_cell_value_survives_as_compact_json() {
        let t = from_json(r#"[{"a":[1,2]},{"a":{"x":1}}]"#).unwrap();
        assert_eq!(t.rows, vec![vec!["[1,2]".to_string()], vec!["{\"x\":1}".to_string()]]);
    }

    #[test]
    fn string_escapes_and_unicode_are_decoded() {
        let t = from_json(r#"[["s"],["a\nb\té"]]"#).unwrap();
        assert_eq!(t.rows, vec![vec!["a\nb\té".to_string()]]);
    }
}
```
