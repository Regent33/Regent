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
            "unknown action '{other}' (screenshot|click|type|key)"
        )),
    }
}
