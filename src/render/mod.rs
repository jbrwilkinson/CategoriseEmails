pub mod console;
pub mod markdown;

use crate::report::Report;
use std::io::Write;

/// Anything that can turn a [`Report`] into output.
///
/// Implementations write to the supplied `out` so that callers can capture
/// output in tests by passing a `Vec<u8>` instead of `io::stdout()`.
pub trait Renderer {
    fn render(&self, report: &Report, out: &mut dyn Write);
}
