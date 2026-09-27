//! Navigating a page, and reading one back as text.
//!
//! These are the two halves of the cheapest useful loop an agent can run: go
//! somewhere, then look at what is there. Neither needs a snapshot or a ref, so
//! a host that only wants to fetch and summarise a page never pays for the
//! accessibility tree.

mod types;

pub use types::{
    EvaluateRequest, NavigateRequest, PageState, PageText, ReadFormat, ReadRequest, WaitUntil,
};

#[cfg(test)]
mod test;
