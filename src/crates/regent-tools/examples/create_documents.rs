//! Generates one sample of each `create_document` format into `--out <dir>`
//! (default `./samples-out`) — the runnable proof that the four writers
//! produce files real readers open. Run:
//! `cargo run -p regent-tools --example create_documents -- --out <dir>`

use regent_tools::{DenyAll, ToolContext, core_catalog};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let out = std::env::args()
        .skip_while(|a| a != "--out")
        .nth(1)
        .unwrap_or_else(|| "samples-out".to_owned());
    std::fs::create_dir_all(&out).expect("create output dir");
    let catalog = core_catalog();
    let ctx = ToolContext::new(std::path::PathBuf::from(&out), Arc::new(DenyAll));

    let sections = serde_json::json!([
        {"heading": "Built to serve", "paragraphs": [
            "Regent generates office documents natively — no Python, no HTML round-trip.",
            "This sample was produced by the create_document tool's runnable proof."
        ]},
        {"heading": "Formats", "bullets": ["PDF via lopdf", "DOCX via docx-rs",
            "PPTX hand-rolled OOXML", "XLSX via rust_xlsxwriter"]}
    ]);
    let jobs = [
        serde_json::json!({"format": "pdf", "path": "sample.pdf",
            "title": "Regent Sample PDF", "sections": sections}),
        serde_json::json!({"format": "docx", "path": "sample.docx",
            "title": "Regent Sample DOCX", "sections": sections}),
        serde_json::json!({"format": "pptx", "path": "sample.pptx",
        "title": "Regent Sample Deck", "slides": [
            {"title": "Built to serve", "bullets": ["Native doc generation",
                "Four formats, one tool"], "notes": "Speaker notes survive too."},
            {"title": "Zero Python", "bullets": ["lopdf", "docx-rs",
                "hand-rolled OOXML", "rust_xlsxwriter"]}
        ]}),
        serde_json::json!({"format": "xlsx", "path": "sample.xlsx", "sheets": [
            {"name": "Formats", "header": true, "rows": [
                ["Format", "Library", "Bytes are real"],
                ["PDF", "lopdf", 1], ["DOCX", "docx-rs", 2],
                ["PPTX", "hand-rolled", 3], ["XLSX", "rust_xlsxwriter", 4]]}
        ]}),
    ];

    for job in jobs {
        let result = catalog
            .dispatch("create_document", &job.to_string(), &ctx)
            .await;
        println!("{result}");
        assert!(
            !result.contains("\"error\""),
            "create_document failed: {result}"
        );
    }
    println!("all four samples written to {out}");
}
