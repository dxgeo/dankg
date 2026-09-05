# Greeting

`make_greeting` has no dependency of its own -- it is the DAG root
`consumer.md#use_greeting` reaches with a cross-file `deps=` (decision
29), and, separately, the tangle node `producer/greeting.rs` becomes once
this file is tangled alongside `consumer.md` (decision 26: naming more
than one file triggers the per-file nesting neither `hash.md` nor a
single named file ever needed).

```rust name=make_greeting
fn make_greeting(name: &str) -> String {
    format!("Hello, {name}!")
}
```
