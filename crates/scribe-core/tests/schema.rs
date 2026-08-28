//! Keeps the checked-in JSON Schema honest.
//!
//! The schema is published alongside the crate so other languages can
//! generate their own types from the layout model. It is generated from the
//! Rust types, so it can only drift if it is not regenerated when they
//! change; this test is what stops that.

use std::path::{Path, PathBuf};

/// Set this in the environment to rewrite the checked-in schema instead of
/// failing when it differs.
const UPDATE_VARIABLE: &str = "SCRIBE_UPDATE_SCHEMA";

fn schema_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schema/layout.schema.json")
}

fn generated_schema() -> String {
    let mut schema = serde_json::to_string_pretty(&scribe_core::layout::Layout::json_schema())
        .expect("the layout schema serialises");
    schema.push('\n');
    schema
}

#[test]
fn checked_in_schema_is_up_to_date() {
    let path = schema_path();
    let generated = generated_schema();

    if std::env::var_os(UPDATE_VARIABLE).is_some() {
        std::fs::write(&path, &generated).expect("the schema can be written");
        return;
    }

    let checked_in = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} cannot be read: {error}", path.display()));
    assert_eq!(
        checked_in,
        generated,
        "{} is out of date; regenerate it with {UPDATE_VARIABLE}=1 cargo test",
        path.display()
    );
}

#[test]
fn schema_describes_the_layout_model() {
    let schema: serde_json::Value =
        serde_json::from_str(&generated_schema()).expect("the schema is JSON");
    assert_eq!(schema["title"], "Layout");
    assert!(schema["properties"]["lines"].is_object());
    assert!(schema["$defs"]["RotatedBox"]["properties"]["angle_deg"].is_object());
}
