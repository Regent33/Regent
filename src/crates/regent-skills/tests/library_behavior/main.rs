//! Skills library + curator behavior contract against the real filesystem
//! repository. Tests are grouped by concern in the submodules.

use regent_skills::{FsSkillRepository, SkillLibrary};
use std::sync::Arc;

mod archive;
mod bundled;
mod crud;
mod curator;

pub fn library(dir: &std::path::Path) -> SkillLibrary {
    SkillLibrary::new(Arc::new(FsSkillRepository::new(dir).unwrap()))
}
