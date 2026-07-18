//! `random_gen` — offline randomness: dice rolls, coin flips, picking/
//! shuffling a list, and passwords. Dice/coin/pick/shuffle use the fast
//! thread-local generator; passwords use `SysRng` (rand 0.10's rename of
//! `OsRng`, the OS's crypto entropy source) via rejection sampling.

use crate::domain::contracts::ToolExecutor;
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use rand::rngs::SysRng;
use rand::seq::{IndexedRandom, SliceRandom};
use rand::{RngExt, TryRng};
use regent_kernel::{RegentError, ToolDefinition, tool_error_json, tool_result_json};
use serde_json::{Value, json};

const MAX_DICE: u32 = 1000;
const MAX_COIN_FLIPS: usize = 10_000;
const MAX_PASSWORD_LENGTH: usize = 512;
const DEFAULT_PASSWORD_LENGTH: u64 = 16;
const UPPER: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const LOWER: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
const DIGITS: &[u8] = b"0123456789";
const SYMBOLS: &[u8] = b"!@#$%^&*()-_=+[]{}<>?/~";

#[must_use]
pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "random_gen".into(),
        description: "Offline randomness. `mode`: \"dice\" (notation, e.g. \"2d6\", N and M \
                      capped at 1000), \"coin\" (count, default 1), \"pick\" (items, count — \
                      distinct picks from a list), \"shuffle\" (items), or \"password\" \
                      (length default 16, uppercase/lowercase/digits/symbols flags — \
                      cryptographically generated)."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "mode": {"type": "string", "enum": ["dice", "coin", "pick", "shuffle", "password"]},
                "notation": {"type": "string", "description": "Dice notation (mode=dice)."},
                "count": {"type": "integer", "description": "Coin flips or pick count."},
                "items": {"type": "array", "description": "List to pick from or shuffle."},
                "length": {"type": "integer", "description": "Password length (mode=password)."},
                "uppercase": {"type": "boolean"},
                "lowercase": {"type": "boolean"},
                "digits": {"type": "boolean"},
                "symbols": {"type": "boolean"}
            },
            "required": ["mode"]
        }),
        toolset: "everyday".into(),
    }
}

pub struct RandomGenTool;

#[async_trait]
impl ToolExecutor for RandomGenTool {
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        match args.get("mode").and_then(Value::as_str) {
            Some("dice") => dice(&args),
            Some("coin") => coin(&args),
            Some("pick") => pick(&args),
            Some("shuffle") => shuffle(&args),
            Some("password") => password(&args),
            Some(other) => Ok(tool_error_json(format!(
                "unknown mode '{other}' — use dice, coin, pick, shuffle, or password"
            ))),
            None => Ok(tool_error_json("missing required parameter: mode")),
        }
    }
}

fn dice(args: &Value) -> Result<String, RegentError> {
    let Some(notation) = args.get("notation").and_then(Value::as_str) else {
        return Ok(tool_error_json("missing required parameter: notation"));
    };
    let (n, m) = match parse_dice(notation) {
        Ok(v) => v,
        Err(e) => return Ok(tool_error_json(e)),
    };
    let mut rng = rand::rng();
    let rolls: Vec<u32> = (0..n).map(|_| rng.random_range(1..=m)).collect();
    let total: u32 = rolls.iter().sum();
    Ok(tool_result_json(
        json!({"notation": notation, "rolls": rolls, "total": total}),
    ))
}

fn parse_dice(notation: &str) -> Result<(u32, u32), String> {
    let bad = || format!("cannot parse dice notation '{notation}' — expected NdM, e.g. '2d6'");
    let lowered = notation.to_lowercase();
    let (n_str, m_str) = lowered.split_once('d').ok_or_else(bad)?;
    let n: u32 = n_str.trim().parse().map_err(|_| bad())?;
    let m: u32 = m_str.trim().parse().map_err(|_| bad())?;
    if n == 0 || m == 0 {
        return Err(format!(
            "'{notation}' needs at least 1 die and at least 1 side"
        ));
    }
    if n > MAX_DICE || m > MAX_DICE {
        return Err(format!(
            "'{notation}' exceeds the limit of {MAX_DICE} for N and M"
        ));
    }
    Ok((n, m))
}

fn coin(args: &Value) -> Result<String, RegentError> {
    let count = args.get("count").and_then(Value::as_u64).unwrap_or(1) as usize;
    if count == 0 || count > MAX_COIN_FLIPS {
        return Ok(tool_error_json(format!(
            "count must be between 1 and {MAX_COIN_FLIPS}"
        )));
    }
    let mut rng = rand::rng();
    let flips: Vec<&str> = (0..count)
        .map(|_| {
            if rng.random_bool(0.5) {
                "heads"
            } else {
                "tails"
            }
        })
        .collect();
    let heads = flips.iter().filter(|f| **f == "heads").count();
    Ok(tool_result_json(
        json!({"flips": flips, "heads": heads, "tails": count - heads}),
    ))
}

fn pick(args: &Value) -> Result<String, RegentError> {
    let Some(items) = args.get("items").and_then(Value::as_array) else {
        return Ok(tool_error_json("missing required parameter: items"));
    };
    if items.is_empty() {
        return Ok(tool_error_json("items must not be empty"));
    }
    let count = args.get("count").and_then(Value::as_u64).unwrap_or(1) as usize;
    if count == 0 || count > items.len() {
        return Ok(tool_error_json(format!(
            "count must be between 1 and the list length ({})",
            items.len()
        )));
    }
    let mut rng = rand::rng();
    let picked: Vec<Value> = items.sample(&mut rng, count).cloned().collect();
    Ok(tool_result_json(json!({"picked": picked})))
}

fn shuffle(args: &Value) -> Result<String, RegentError> {
    let Some(items) = args.get("items").and_then(Value::as_array) else {
        return Ok(tool_error_json("missing required parameter: items"));
    };
    let mut shuffled = items.clone();
    let mut rng = rand::rng();
    shuffled.shuffle(&mut rng);
    Ok(tool_result_json(json!({"shuffled": shuffled})))
}

fn password(args: &Value) -> Result<String, RegentError> {
    let length = args
        .get("length")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_PASSWORD_LENGTH) as usize;
    if length == 0 || length > MAX_PASSWORD_LENGTH {
        return Ok(tool_error_json(format!(
            "length must be between 1 and {MAX_PASSWORD_LENGTH}"
        )));
    }
    let opt = |field: &str| args.get(field).and_then(Value::as_bool);
    let (upper, lower, digits, symbols) = match (
        opt("uppercase"),
        opt("lowercase"),
        opt("digits"),
        opt("symbols"),
    ) {
        (None, None, None, None) => (true, true, true, false),
        (u, l, d, s) => (
            u.unwrap_or(false),
            l.unwrap_or(false),
            d.unwrap_or(false),
            s.unwrap_or(false),
        ),
    };
    let mut charset: Vec<u8> = Vec::new();
    for (enabled, chars) in [
        (upper, UPPER),
        (lower, LOWER),
        (digits, DIGITS),
        (symbols, SYMBOLS),
    ] {
        if enabled {
            charset.extend_from_slice(chars);
        }
    }
    if charset.is_empty() {
        return Ok(tool_error_json(
            "at least one of uppercase, lowercase, digits, symbols must be enabled",
        ));
    }
    let password: String = (0..length)
        .map(|_| charset[secure_index(charset.len())] as char)
        .collect();
    Ok(tool_result_json(json!({
        "password": password,
        "length": length,
        "charset": {"uppercase": upper, "lowercase": lower, "digits": digits, "symbols": symbols},
    })))
}

/// Uniformly-distributed index in `0..max` (`max <= 256`) drawn from the OS's
/// cryptographic entropy source, via rejection sampling to avoid modulo bias.
fn secure_index(max: usize) -> usize {
    let cutoff = 256 - (256 % max);
    let mut byte = [0u8; 1];
    loop {
        SysRng
            .try_fill_bytes(&mut byte)
            .expect("the OS entropy source is unavailable");
        let value = byte[0] as usize;
        if value < cutoff {
            return value % max;
        }
    }
}

#[cfg(test)]
#[path = "random_gen_tests.rs"]
mod tests;
