# Scaffold

`#![deny(rustdoc::broken_intra_doc_links)]` is what turns
`rust-docinject.py` writing a bad link into a build failure `cargo doc`
would otherwise only warn about by default -- the same reason this
pilot's own `command` runs `cargo doc`, not just `cargo test`.

```rust name=cargo_manifest path=Cargo.toml
[package]
name = "dankg-docinject-pilot"
version = "0.1.0"
edition = "2024"

[lib]
name = "dankg_docinject_pilot"
path = "lib.rs"

[dependencies]
```

```rust name=crate_root path=lib.rs
#![deny(rustdoc::broken_intra_doc_links)]

pub mod producer;
pub mod consumer;

#[cfg(test)]
mod tests {
    use super::consumer::caller::shout;

    #[test]
    fn shouts_the_greeting() {
        assert_eq!(shout("world"), "HELLO, WORLD!");
    }
}
```
