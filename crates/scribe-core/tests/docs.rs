//! Keeps the option tables in the format documentation honest.
//!
//! Every option a renderer takes is described once, in the renderer itself,
//! and the tables in `docs/formats.md` are written from those descriptions.
//! They can only drift if they are not written again when a renderer changes;
//! this test is what stops that.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use scribe_core::render::{OptionKind, OptionSpec, OptionValue, registry};

/// Set this in the environment to rewrite the tables instead of failing when
/// they differ.
const UPDATE_VARIABLE: &str = "SCRIBE_UPDATE_DOCS";

/// What the line opening a generated table starts with, followed by the name
/// of the format the table is of.
const OPEN: &str = "<!-- options: ";

/// The line closing a generated table.
const CLOSE: &str = "<!-- end options -->";

fn formats_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/formats.md")
}

/// The table of one format's options, as the documentation carries it.
fn table(specs: &[OptionSpec]) -> String {
    if specs.is_empty() {
        return "This format takes no options.\n".to_string();
    }
    let mut table = String::from("| Option | Takes | Default | What it does |\n");
    table.push_str("| --- | --- | --- | --- |\n");
    for spec in specs {
        let _ = writeln!(
            table,
            "| `{}` | {} | {} | {} |",
            spec.name,
            takes(spec),
            shown(&spec.default),
            cell(spec.help),
        );
    }
    table
}

/// What an option accepts: the words it is narrowed to, or the kind of value
/// it takes.
fn takes(spec: &OptionSpec) -> String {
    if spec.choices.is_empty() {
        return match spec.kind {
            OptionKind::Bool => "`true` or `false`",
            OptionKind::Int => "a whole number",
            OptionKind::Float => "a number",
            OptionKind::Str => "text",
        }
        .to_string();
    }
    spec.choices
        .iter()
        .map(|choice| format!("`{choice}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// A default as a table shows it, with an empty string named rather than
/// written as nothing at all.
fn shown(value: &OptionValue) -> String {
    match value {
        OptionValue::Bool(value) => format!("`{value}`"),
        OptionValue::Int(value) => format!("`{value}`"),
        OptionValue::Float(value) => format!("`{value}`"),
        OptionValue::Str(text) if text.is_empty() => "empty".to_string(),
        OptionValue::Str(text) => format!("`{}`", cell(text)),
    }
}

/// Text as it can stand inside a table cell, where a bar would end it early.
fn cell(text: &str) -> String {
    text.replace('|', "\\|")
}

/// The documentation with every generated table written again from the
/// renderers.
///
/// # Panics
///
/// Panics if a table is opened and never closed, or if it names a format
/// this build does not have.
fn regenerated(document: &str) -> String {
    let registry = registry();
    let mut written = String::with_capacity(document.len());
    let mut lines = document.lines();
    while let Some(line) = lines.next() {
        written.push_str(line);
        written.push('\n');
        let Some(name) = line
            .trim()
            .strip_prefix(OPEN)
            .and_then(|rest| rest.strip_suffix("-->").map(|name| name.trim().to_string()))
        else {
            continue;
        };
        let renderer = registry
            .get(&name)
            .unwrap_or_else(|| panic!("`{name}` is documented but is not a format"));
        written.push_str(&table(&renderer.describe_options()));
        let closed = lines.by_ref().any(|line| line.trim() == CLOSE);
        assert!(closed, "the table of `{name}` options is never closed");
        written.push_str(CLOSE);
        written.push('\n');
    }
    written
}

#[test]
fn option_tables_are_up_to_date() {
    let path = formats_path();
    let document = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} cannot be read: {error}", path.display()));
    let regenerated = regenerated(&document);

    if std::env::var_os(UPDATE_VARIABLE).is_some() {
        std::fs::write(&path, &regenerated).expect("the documentation can be written");
        return;
    }

    assert_eq!(
        document,
        regenerated,
        "{} is out of date; regenerate it with {UPDATE_VARIABLE}=1 cargo test",
        path.display()
    );
}

#[test]
fn every_format_has_a_table() {
    let path = formats_path();
    let document = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} cannot be read: {error}", path.display()));
    for name in registry().names() {
        assert!(
            document.contains(&format!("{OPEN}{name} -->")),
            "the `{name}` format has no option table in {}",
            path.display()
        );
    }
}
