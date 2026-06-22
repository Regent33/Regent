//! Curator use case — skill lifecycle maintenance over usage telemetry.
//! Invariants: only `created_by: agent` skills are
//! touched; pinned skills are exempt from every transition; the most
//! destructive action is archive — never delete.

use crate::application::library::SkillLibrary;
use crate::domain::entities::SkillState;
use crate::domain::errors::SkillError;

#[derive(Debug, Clone)]
pub struct CuratorConfig {
    pub stale_after_days: f64,
    pub archive_after_days: f64,
}

impl Default for CuratorConfig {
    fn default() -> Self {
        Self { stale_after_days: 30.0, archive_after_days: 90.0 }
    }
}

#[derive(Debug, Default)]
pub struct CuratorReport {
    pub marked_stale: Vec<String>,
    pub archived: Vec<String>,
}

pub fn curate(
    library: &SkillLibrary,
    now_epoch: f64,
    config: &CuratorConfig,
) -> Result<CuratorReport, SkillError> {
    let repository = library.repository();
    let mut usage = repository.load_usage()?;
    let mut report = CuratorReport::default();

    for name in repository.list_names()? {
        let record = match repository.load(&name) {
            Ok(record) => record,
            Err(error) => {
                tracing::warn!(skill = name, %error, "curator skipping unreadable skill");
                continue;
            }
        };
        // Hard scope: agent-created, unpinned, with telemetry only.
        if record.meta.created_by != "agent" || record.meta.pinned {
            continue;
        }
        let Some(telemetry) = usage.skills.get(&name).cloned() else {
            continue;
        };
        let idle_days = (now_epoch - telemetry.last_activity_at).max(0.0) / 86_400.0;

        if idle_days >= config.archive_after_days {
            repository.archive(&name)?;
            usage.touch(&name, telemetry.last_activity_at, |r| r.state = SkillState::Archived);
            report.archived.push(name);
        } else if idle_days >= config.stale_after_days && telemetry.state == SkillState::Active {
            usage.touch(&name, telemetry.last_activity_at, |r| r.state = SkillState::Stale);
            report.marked_stale.push(name);
        }
    }

    repository.save_usage(&usage)?;
    Ok(report)
}
