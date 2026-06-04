pub mod text;

use crate::report::Report;

/// Anything that can turn a [`Report`] into output.
pub trait Renderer {
    fn render(&self, report: &Report);
}
