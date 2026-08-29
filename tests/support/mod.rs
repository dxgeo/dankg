//! Test-only helpers.
//!
//! Not every test binary uses every helper -- `fmt` needs the JSON reader but
//! not the HTML oracle -- so unused items here are expected, not a smell.
#![allow(dead_code)]

pub mod html;
pub mod json;
