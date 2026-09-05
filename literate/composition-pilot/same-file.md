# Utils

Two blocks under one heading, `quadruple` depending on `double`. Tangle
folds both into the very same output file, in document order, regardless
of `deps=` (decision 24: containment decides placement, `deps=` is eval's
own concern and tangle never reads it) -- and document order already put
`double` ahead of `quadruple` here, the same order `deps=` would also
demand of eval's own concatenation (decision 11). The two mechanisms agree
by construction whenever a dependency and its target share one heading:
neither needs `use`, `mod`, or any cross-file reference at all, because
there is no cross-file boundary between them to begin with.

```rust name=double
fn double(n: i32) -> i32 {
    n * 2
}
```

```rust name=quadruple deps=double
fn quadruple(n: i32) -> i32 {
    double(double(n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quadruple_is_double_twice() {
        assert_eq!(quadruple(3), 12);
    }
}
```

See `consumer.md`'s *Pilot notes* for what this file proved once compared
against the `producer.md`/`consumer.md` pair below.
