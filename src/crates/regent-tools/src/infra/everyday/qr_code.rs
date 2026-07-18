//! `qr_code` — everyday QR generation, fully offline (qrcode crate). Three
//! outputs: `terminal` returns a unicode-block rendering inline (no file),
//! `svg`/`png` write a file through the same `ToolContext::resolve` jail every
//! file tool uses. Errors are descriptive — an unwritable path or oversized
//! payload names itself, never a silent empty result.

use crate::domain::contracts::ToolExecutor;
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use qrcode::QrCode;
use qrcode::render::{svg, unicode};
use regent_kernel::{RegentError, ToolDefinition, tool_result_json};
use serde_json::{Value, json};

#[must_use]
pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "qr_code".into(),
        description: "Generate a QR code from text (a URL, wifi string, contact, any payload). \
                      Offline. `output`: \"terminal\" (default — returns a unicode rendering \
                      inline), \"svg\" or \"png\" (write a file; `path` required)."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "text": {"type": "string", "description": "The payload to encode."},
                "output": {"type": "string", "enum": ["terminal", "svg", "png"]},
                "path": {"type": "string", "description": "Output file path (svg/png only)."}
            },
            "required": ["text"]
        }),
        toolset: "everyday".into(),
    }
}

pub struct QrCodeTool;

#[async_trait]
impl ToolExecutor for QrCodeTool {
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<String, RegentError> {
        let text = args
            .get("text")
            .and_then(Value::as_str)
            .filter(|t| !t.is_empty())
            .ok_or_else(|| bad("`text` (a non-empty string) is required"))?;
        let output = args
            .get("output")
            .and_then(Value::as_str)
            .unwrap_or("terminal");

        let code = QrCode::new(text.as_bytes())
            .map_err(|e| bad(format!("cannot encode as QR ({e}) — payload too large?")))?;

        match output {
            "terminal" => {
                let rendered = code.render::<unicode::Dense1x2>().quiet_zone(true).build();
                Ok(tool_result_json(json!({
                    "qr": rendered,
                    "note": "print inside a code block — needs a monospace font",
                })))
            }
            "svg" | "png" => {
                let path = args
                    .get("path")
                    .and_then(Value::as_str)
                    .ok_or_else(|| bad(format!("`path` is required for {output} output")))?;
                let resolved = ctx.resolve(path).map_err(|e| bad(e.to_string()))?;
                if let Some(parent) = resolved.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| bad(format!("cannot create {}: {e}", parent.display())))?;
                }
                if output == "svg" {
                    let rendered = code.render::<svg::Color>().min_dimensions(256, 256).build();
                    std::fs::write(&resolved, rendered.as_bytes())
                        .map_err(|e| bad(format!("cannot write {}: {e}", resolved.display())))?;
                } else {
                    let img = code
                        .render::<image::Luma<u8>>()
                        .min_dimensions(256, 256)
                        .build();
                    img.save(&resolved)
                        .map_err(|e| bad(format!("cannot write {}: {e}", resolved.display())))?;
                }
                Ok(tool_result_json(json!({
                    "created": resolved.display().to_string(),
                    "format": output,
                })))
            }
            other => Err(bad(format!(
                "unknown output '{other}' — use \"terminal\", \"svg\", or \"png\""
            ))),
        }
    }
}

fn bad(message: impl Into<String>) -> RegentError {
    RegentError::Tool {
        tool: "qr_code".into(),
        message: message.into(),
    }
}

#[cfg(test)]
#[path = "qr_code_tests.rs"]
mod tests;
