//! Named-agent persistence — dumb CRUD over the `agents` table. A definition is
//! a name + role description + system prompt, plus optional model and tool
//! allow-list. The board dispatcher resolves a task's assignee to one of these
//! and runs it; the CLI manages them via `regent agents`.

use crate::domain::entities::AgentRow;
use crate::domain::errors::StoreError;
use crate::infra::db::{Store, now_epoch};
use rusqlite::{OptionalExtension, params};

const COLUMNS: &str = "name, description, system_prompt, model, tools, created_at, updated_at";

fn row_to_agent(row: &rusqlite::Row<'_>) -> Result<AgentRow, rusqlite::Error> {
    Ok(AgentRow {
        name: row.get(0)?,
        description: row.get(1)?,
        system_prompt: row.get(2)?,
        model: row.get(3)?,
        tools: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

impl Store {
    /// All agents, alphabetical.
    pub fn list_agents(&self) -> Result<Vec<AgentRow>, StoreError> {
        self.with_read(|conn| {
            let mut stmt =
                conn.prepare(&format!("SELECT {COLUMNS} FROM agents ORDER BY name"))?;
            stmt.query_map([], row_to_agent)?.collect()
        })
    }

    pub fn find_agent(&self, name: &str) -> Result<Option<AgentRow>, StoreError> {
        self.with_read(|conn| {
            conn.query_row(
                &format!("SELECT {COLUMNS} FROM agents WHERE name = ?1"),
                params![name],
                row_to_agent,
            )
            .optional()
        })
    }

    /// Create or update an agent by name (preserves `created_at` on update).
    pub fn upsert_agent(
        &self,
        name: &str,
        description: &str,
        system_prompt: &str,
        model: Option<&str>,
        tools: Option<&str>,
    ) -> Result<(), StoreError> {
        self.with_write(|tx| {
            let now = now_epoch();
            tx.execute(
                "INSERT INTO agents
                   (name, description, system_prompt, model, tools, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
                 ON CONFLICT(name) DO UPDATE SET
                   description = ?2, system_prompt = ?3, model = ?4, tools = ?5, updated_at = ?6",
                params![name, description, system_prompt, model, tools, now],
            )?;
            Ok(())
        })
    }

    /// Remove an agent; returns whether a row was deleted.
    pub fn remove_agent(&self, name: &str) -> Result<bool, StoreError> {
        self.with_write(|tx| Ok(tx.execute("DELETE FROM agents WHERE name = ?1", params![name])? > 0))
    }
}

#[cfg(test)]
mod tests {
    use crate::Store;

    #[test]
    fn upsert_get_list_remove_roundtrip() {
        let store = Store::open_in_memory().unwrap();
        assert!(store.list_agents().unwrap().is_empty());

        store
            .upsert_agent("researcher", "web research", "You research + cite.", None, None)
            .unwrap();
        let got = store.find_agent("researcher").unwrap().unwrap();
        assert_eq!(got.description, "web research");
        assert_eq!(got.model, None);

        // Update preserves created_at, changes fields.
        store
            .upsert_agent("researcher", "deep research", "You research + cite.", Some("claude-opus-4-8"), Some("memory_search,web"))
            .unwrap();
        let got2 = store.find_agent("researcher").unwrap().unwrap();
        assert_eq!(got2.description, "deep research");
        assert_eq!(got2.model.as_deref(), Some("claude-opus-4-8"));
        assert_eq!(got2.created_at, got.created_at);

        assert_eq!(store.list_agents().unwrap().len(), 1);
        assert!(store.remove_agent("researcher").unwrap());
        assert!(!store.remove_agent("researcher").unwrap());
        assert!(store.find_agent("researcher").unwrap().is_none());
    }
}
