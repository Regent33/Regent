//! JSON args -> Action parsing for computer_use. Split from `mod.rs`
//! (file-size rule).

use super::*;

pub(super) fn parse_action(args: &Value) -> Result<Action, String> {
    let action = args
        .get("action")
        .and_then(Value::as_str)
        .ok_or("missing required parameter: action")?;
    match action {
        "screenshot" => Ok(Action::Screenshot),
        "list_windows" => Ok(Action::ListWindows),
        "focus_window" => Ok(Action::FocusWindow {
            window_id: window_id(args, action)?,
        }),
        "close_window" => Ok(Action::CloseWindow {
            window_id: window_id(args, action)?,
        }),
        "list_tabs" => Ok(Action::ListTabs {
            window_id: window_id(args, action)?,
        }),
        "close_tab" => Ok(Action::CloseTab {
            window_id: window_id(args, action)?,
            target: args
                .get("target")
                .and_then(Value::as_str)
                .filter(|target| !target.trim().is_empty())
                .ok_or("close_tab needs non-empty 'target'")?
                .to_owned(),
        }),
        "click" => {
            let x = args
                .get("x")
                .and_then(Value::as_i64)
                .ok_or("click needs integer x")?;
            let y = args
                .get("y")
                .and_then(Value::as_i64)
                .ok_or("click needs integer y")?;
            Ok(Action::Click {
                x: x as i32,
                y: y as i32,
            })
        }
        "type" => {
            let text = args
                .get("text")
                .and_then(Value::as_str)
                .ok_or("type needs 'text'")?;
            Ok(Action::Type {
                text: text.to_owned(),
            })
        }
        "key" => {
            let combo = args
                .get("keys")
                .and_then(Value::as_str)
                .filter(|k| !k.trim().is_empty())
                .ok_or("key needs 'keys' (e.g. 'ctrl+s')")?;
            Ok(Action::Key {
                combo: combo.to_owned(),
            })
        }
        other => Err(format!(
            "unknown action '{other}' (screenshot|list_windows|focus_window|close_window|list_tabs|close_tab|click|type|key)"
        )),
    }
}

fn window_id(args: &Value, action: &str) -> Result<i64, String> {
    args.get("window_id")
        .and_then(Value::as_i64)
        .filter(|id| *id != 0)
        .ok_or_else(|| format!("{action} needs non-zero integer window_id from list_windows"))
}
