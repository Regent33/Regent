//! Skills library + curator behavior contract against the real filesystem
//! repository. Tests are grouped by concern in the submodules.

use regent_skills::{FsSkillRepository, SkillLibrary};
use std::sync::Arc;

mod archive;
mod bundled;
mod crud;
mod curator;
mod curator_exposure;
mod curator_starvation;

pub fn library(dir: &std::path::Path) -> SkillLibrary {
    SkillLibrary::new(Arc::new(FsSkillRepository::new(dir).unwrap()))
}

/// Wall clock, for the tests that mix fixture timestamps with a real `view`
/// (which stamps `SystemTime::now`). A frozen `now` would make those two
/// disagree by 25 years and pass for the wrong reason.
pub fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}
