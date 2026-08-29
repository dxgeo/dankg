//! Minimal JSON reader, sufficient for the CommonMark spec file.
//!
//! Test-only. DanKG itself has no runtime need for JSON input, and taking a
//! crate for test data would defeat the point of the dependency constraint.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(BTreeMap<String, Json>),
}

impl Json {
    pub fn as_array(&self) -> &[Json] {
        match self {
            Json::Arr(v) => v,
            _ => panic!("expected array"),
        }
    }

    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(m) => m.get(key),
            _ => None,
        }
    }

    pub fn str(&self, key: &str) -> &str {
        match self.get(key) {
            Some(Json::Str(s)) => s,
            _ => panic!("expected string at {key}"),
        }
    }

    pub fn num(&self, key: &str) -> f64 {
        match self.get(key) {
            Some(Json::Num(n)) => *n,
            _ => panic!("expected number at {key}"),
        }
    }
}

pub fn parse(input: &str) -> Json {
    let chars: Vec<char> = input.chars().collect();
    let mut p = P { c: &chars, i: 0 };
    p.ws();
    let v = p.value();
    p.ws();
    assert_eq!(p.i, p.c.len(), "trailing JSON input");
    v
}

struct P<'a> {
    c: &'a [char],
    i: usize,
}

impl<'a> P<'a> {
    fn ws(&mut self) {
        while matches!(self.c.get(self.i), Some(' ' | '\t' | '\n' | '\r')) {
            self.i += 1;
        }
    }

    fn eat(&mut self, ch: char) {
        assert_eq!(self.c.get(self.i), Some(&ch), "expected {ch} at {}", self.i);
        self.i += 1;
    }

    fn value(&mut self) -> Json {
        match self.c.get(self.i) {
            Some('{') => self.object(),
            Some('[') => self.array(),
            Some('"') => Json::Str(self.string()),
            Some('t') => {
                self.i += 4;
                Json::Bool(true)
            }
            Some('f') => {
                self.i += 5;
                Json::Bool(false)
            }
            Some('n') => {
                self.i += 4;
                Json::Null
            }
            _ => self.number(),
        }
    }

    fn object(&mut self) -> Json {
        self.eat('{');
        let mut map = BTreeMap::new();
        self.ws();
        if self.c.get(self.i) == Some(&'}') {
            self.i += 1;
            return Json::Obj(map);
        }
        loop {
            self.ws();
            let k = self.string();
            self.ws();
            self.eat(':');
            self.ws();
            map.insert(k, self.value());
            self.ws();
            match self.c.get(self.i) {
                Some(',') => self.i += 1,
                Some('}') => {
                    self.i += 1;
                    return Json::Obj(map);
                }
                other => panic!("bad object at {}: {other:?}", self.i),
            }
        }
    }

    fn array(&mut self) -> Json {
        self.eat('[');
        let mut out = Vec::new();
        self.ws();
        if self.c.get(self.i) == Some(&']') {
            self.i += 1;
            return Json::Arr(out);
        }
        loop {
            self.ws();
            out.push(self.value());
            self.ws();
            match self.c.get(self.i) {
                Some(',') => self.i += 1,
                Some(']') => {
                    self.i += 1;
                    return Json::Arr(out);
                }
                other => panic!("bad array at {}: {other:?}", self.i),
            }
        }
    }

    fn string(&mut self) -> String {
        self.eat('"');
        let mut s = String::new();
        loop {
            let c = *self.c.get(self.i).expect("unterminated string");
            self.i += 1;
            match c {
                '"' => return s,
                '\\' => {
                    let e = *self.c.get(self.i).expect("dangling escape");
                    self.i += 1;
                    match e {
                        'n' => s.push('\n'),
                        't' => s.push('\t'),
                        'r' => s.push('\r'),
                        'b' => s.push('\u{8}'),
                        'f' => s.push('\u{c}'),
                        'u' => {
                            let hex: String = self.c[self.i..self.i + 4].iter().collect();
                            self.i += 4;
                            let n = u32::from_str_radix(&hex, 16).expect("bad \\u escape");
                            // Surrogate pair.
                            if (0xD800..0xDC00).contains(&n) {
                                assert_eq!(self.c.get(self.i), Some(&'\\'));
                                let hex2: String = self.c[self.i + 2..self.i + 6].iter().collect();
                                self.i += 6;
                                let lo = u32::from_str_radix(&hex2, 16).expect("bad low surrogate");
                                let cp = 0x10000 + ((n - 0xD800) << 10) + (lo - 0xDC00);
                                s.push(char::from_u32(cp).expect("bad surrogate pair"));
                            } else {
                                s.push(char::from_u32(n).unwrap_or('\u{FFFD}'));
                            }
                        }
                        other => s.push(other),
                    }
                }
                other => s.push(other),
            }
        }
    }

    fn number(&mut self) -> Json {
        let start = self.i;
        while matches!(
            self.c.get(self.i),
            Some('-' | '+' | '.' | 'e' | 'E' | '0'..='9')
        ) {
            self.i += 1;
        }
        let text: String = self.c[start..self.i].iter().collect();
        Json::Num(text.parse().expect("bad number"))
    }
}
