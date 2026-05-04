use std::path::PathBuf;

use anyhow::{Context, Result};
use directories::{BaseDirs, ProjectDirs};

pub struct Paths {
    pub data_dir: PathBuf,
    pub db_path: PathBuf,
    pub transcripts_dir: PathBuf,
}

impl Paths {
    pub fn resolve() -> Result<Self> {
        let project = ProjectDirs::from("", "", "larder")
            .context("could not resolve XDG project directories")?;
        let data_dir = project.data_dir().to_path_buf();
        let db_path = data_dir.join("larder.sqlite");

        let base = BaseDirs::new().context("could not resolve home directory")?;
        let transcripts_dir = base.home_dir().join(".claude").join("projects");

        Ok(Self {
            data_dir,
            db_path,
            transcripts_dir,
        })
    }
}
