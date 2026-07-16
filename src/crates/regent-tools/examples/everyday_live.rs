//! Live-network proof for the everyday toolset (the unit tests stay offline
//! by design). Hits Open-Meteo, dictionaryapi.dev, and frankfurter.dev once
//! each, plus a few offline dispatches. Run when an endpoint is suspected of
//! having moved: `cargo run -p regent-tools --example everyday_live`

use regent_tools::{DenyAll, ToolContext, core_catalog};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let catalog = core_catalog();
    let ctx = ToolContext::new(std::path::PathBuf::from("."), Arc::new(DenyAll));
    let calls = [
        ("weather", r#"{"place": "Manila"}"#),
        ("sun_moon", r#"{"place": "Manila"}"#),
        ("dictionary", r#"{"word": "serve"}"#),
        ("convert", r#"{"value": 25, "from": "USD", "to": "PHP"}"#),
        ("convert", r#"{"value": 5, "from": "km", "to": "mi"}"#),
        ("calc", r#"{"expression": "10 / 3", "precision": 2}"#),
        ("world_time", r#"{"zones": ["Asia/Manila", "America/New_York"]}"#),
        ("date_calc", r#"{"action": "days_between", "date": "2026-01-01", "date2": "2026-07-16"}"#),
        ("random_gen", r#"{"mode": "dice", "notation": "2d6"}"#),
        ("qr_code", r#"{"text": "https://github.com/Regent33/Regent"}"#),
    ];
    let mut failures = 0;
    for (tool, args) in calls {
        let result = catalog.dispatch(tool, args, &ctx).await;
        let ok = !result.contains("\"error\"");
        let head: String = result.chars().take(140).collect();
        println!("[{}] {tool} {args}\n    {head}", if ok { "ok" } else { "FAIL" });
        failures += usize::from(!ok);
    }
    assert_eq!(failures, 0, "{failures} everyday tool call(s) failed");
    println!("all everyday tools answered");
}
