//! Structural (parse) and semantic validation. Always runs before render;
//! a document with any error is never rendered.

// `!(v >= 0.0)` is deliberate throughout: it also rejects NaN, which
// non-JSON inputs (MessagePack) can carry.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

mod parse;
mod rules;

pub(crate) use parse::parse;
pub(crate) use rules::check;

#[cfg(test)]
mod tests;
