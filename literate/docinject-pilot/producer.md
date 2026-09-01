---
dankg.tangle.public: true
---

# Greeting

A friendly hello, capitalized on the name only -- nothing here depends on
anything else in this corpus.

```rust name=make_greeting
pub fn make_greeting(name: &str) -> String {
    format!("Hello, {name}!")
}
```
