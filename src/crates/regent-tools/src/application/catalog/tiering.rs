//! SPL tool tiering: `defer` folds unpinned tools into the `load_tools`
//! index line; `load_tools` promotes them back. Split from `catalog.rs`
//! (file-size rule).

use super::*;

impl ToolCatalog {
    /// Replaces a tool's executor with a decorated one built from the
    /// original (definition untouched — the model sees no difference). The
    /// seam for per-surface decoration, e.g. the coding harness appending
    /// edit-time diagnostics to file-edit results. Returns false for unknown
    /// names (no-op).
    pub fn wrap_executor(
        &mut self,
        name: &str,
        wrap: impl FnOnce(Arc<dyn ToolExecutor>) -> Arc<dyn ToolExecutor>,
    ) -> bool {
        match self.tools.get_mut(name) {
            Some(entry) => {
                entry.executor = wrap(Arc::clone(&entry.executor));
                true
            }
            None => false,
        }
    }

    /// Removes tools by name (per-surface disable). Returns how many were
    /// removed; unknown names are ignored.
    pub fn disable(&mut self, names: &[String]) -> usize {
        let before = self.tools.len();
        self.tools
            .retain(|name, _| !names.iter().any(|n| n == name));
        before - self.tools.len()
    }

    /// Keeps only tools whose name is in `allowed` (a named agent's tool
    /// allow-list). Unknown names in `allowed` are ignored; an empty list keeps
    /// nothing. Returns how many were removed.
    ///
    /// Survivors are un-deferred: an allow-list is an explicit "these must
    /// work", and a restricted catalog rarely keeps `load_tools` — a tool
    /// left deferred here would be invisible AND unloadable, a silent
    /// capability hole (bit the CodePlan phase when a pinned-list override
    /// let auto-tiering defer a read-only tool).
    pub fn restrict_to(&mut self, allowed: &[String]) -> usize {
        let before = self.tools.len();
        self.tools
            .retain(|name, _| allowed.iter().any(|a| a == name));
        self.deferred.retain(|n| !self.tools.contains_key(n));
        before - self.tools.len()
    }

    /// Marks registered tools as deferred (schemas withheld until loaded) and
    /// registers the `load_tools` loader, whose description carries a one-line
    /// index of what's loadable. Unknown names are ignored. No-op for an
    /// empty effective list — no loader, zero prompt cost.
    pub fn defer(&mut self, names: &[String]) -> Result<usize, RegentError> {
        let known: Vec<String> = names
            .iter()
            .filter(|n| self.tools.contains_key(*n) && *n != "load_tools")
            .cloned()
            .collect();
        if known.is_empty() {
            return Ok(0);
        }
        self.deferred.extend(known.iter().cloned());
        let index: String = known
            .iter()
            .map(|n| {
                let desc = &self.tools[n].definition.description;
                // 60 chars: with adaptive tiering (SPL §3.5) MOST tools are
                // deferred, so the per-entry hook dominates the load_tools
                // schema — the P4 ≤2.5k-token catalog ceiling is sized to this.
                let hook: String = desc.chars().take(60).collect();
                format!("{n} ({hook}…)")
            })
            .collect::<Vec<_>>()
            .join(" · ");
        let deferred_defs: Vec<ToolDefinition> = known
            .iter()
            .map(|n| self.tools[n].definition.clone())
            .collect();
        self.register(
            ToolDefinition {
                name: "load_tools".into(),
                description: format!(
                    "Load the full schema of deferred tools, making them callable. More tools \
                     exist than are listed — load one when its purpose matches the task: {index}"
                ),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "names": {"type": "array", "items": {"type": "string"},
                                  "description": "Deferred tool names to load."}
                    },
                    "required": ["names"]
                }),
                toolset: "core".into(),
            },
            Arc::new(LoadToolsTool {
                deferred: deferred_defs,
                activated: Arc::clone(&self.activated),
            }),
        )?;
        Ok(self.deferred.len())
    }
}

/// Executor for `load_tools`: returns the requested deferred definitions
/// (the model reads the schemas from the result) and activates them so they
/// also appear in the next request's tool list.
struct LoadToolsTool {
    deferred: Vec<ToolDefinition>,
    activated: Arc<RwLock<BTreeSet<String>>>,
}

#[async_trait]
impl ToolExecutor for LoadToolsTool {
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        let requested: Vec<String> = args
            .get("names")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        if requested.is_empty() {
            return Ok(tool_error_json(
                "load_tools needs 'names' (a non-empty array)",
            ));
        }
        let mut loaded = Vec::new();
        let mut unknown = Vec::new();
        for name in &requested {
            match self.deferred.iter().find(|d| &d.name == name) {
                Some(def) => {
                    self.activated
                        .write()
                        .expect("catalog lock poisoned")
                        .insert(name.clone());
                    loaded.push(json!({
                        "name": def.name,
                        "description": def.description,
                        "parameters": def.parameters,
                    }));
                }
                // A near-miss self-corrects instead of dead-ending: suggest
                // deferred names that contain (or are contained by) the ask.
                None => {
                    let ask = name.trim().to_ascii_lowercase();
                    let close: Vec<&str> = if ask.is_empty() {
                        Vec::new() // "" substring-matches everything — no signal
                    } else {
                        self.deferred
                            .iter()
                            .map(|d| d.name.as_str())
                            .filter(|n| {
                                let cand = n.to_ascii_lowercase();
                                cand.contains(&ask) || ask.contains(&cand)
                            })
                            .collect()
                    };
                    unknown.push(json!({ "name": name, "did_you_mean": close }));
                }
            }
        }
        Ok(json!({
            "loaded": loaded,
            "unknown": unknown,
            "note": "loaded tools are callable now and listed from the next turn on",
        })
        .to_string())
    }
}
