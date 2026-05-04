use anyhow::Result;

use crate::cli::OutputFormat;
use crate::search::Hit;

pub fn render_hits(_hits: &[Hit], _format: OutputFormat, _color: bool) -> Result<String> {
    todo!("render hits in text/json/md")
}

pub fn render_command_only(_hits: &[Hit]) -> Option<String> {
    todo!("first command only, for $(...) substitution")
}
