//! What the binary does when it is run.
//!
//! The listings, the usage errors and the exit statuses are checked without
//! any model, since none of them need one. The two tests that do run
//! recognition read `SCRIBE_DETECTION_MODEL` and `SCRIBE_RECOGNITION_MODEL`
//! — `scripts/fetch-models.sh` puts a copy where they can point — and pass
//! with a notice when those are unset.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;

/// Set this to the path of the text detection model.
const DETECTION_VARIABLE: &str = "SCRIBE_DETECTION_MODEL";

/// Set this to the path of the text recognition model.
const RECOGNITION_VARIABLE: &str = "SCRIBE_RECOGNITION_MODEL";

/// The fixtures the whole project shares, by the name each one's image and
/// layout are called after.
const FIXTURES: &[&str] = &["hello", "paragraph", "rotated", "sparse", "blank"];

/// The status a mistake in the request leaves with.
const USAGE: i32 = 2;

/// The status a failure to carry out the request leaves with.
const FAILURE: i32 = 1;

/// One of the checked-in fixtures.
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}

/// The binary, with any models the environment happens to name taken away so
/// that a test which must not find one never does.
fn scribe() -> Command {
    let mut command =
        Command::cargo_bin("scribe").expect("the binary is built alongside its tests");
    command
        .env_remove(DETECTION_VARIABLE)
        .env_remove(RECOGNITION_VARIABLE);
    command
}

/// Where the models are, or `None` with a printed notice if the environment
/// does not say.
fn models() -> Option<(OsString, OsString)> {
    match (
        std::env::var_os(DETECTION_VARIABLE),
        std::env::var_os(RECOGNITION_VARIABLE),
    ) {
        (Some(detection), Some(recognition)) => Some((detection, recognition)),
        _ => {
            println!(
                "skipped: set {DETECTION_VARIABLE} and {RECOGNITION_VARIABLE} to run this test"
            );
            None
        }
    }
}

/// The binary with the models named in its environment rather than on its
/// command line, which is the other way of naming them.
fn scribe_with_models() -> Option<Command> {
    let (detection, recognition) = models()?;
    let mut command = scribe();
    command
        .env(DETECTION_VARIABLE, detection)
        .env(RECOGNITION_VARIABLE, recognition);
    Some(command)
}

/// A directory of this test run's own, emptied before it is used.
fn work_directory(name: &str) -> PathBuf {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a working directory can be made");
    directory
}

#[test]
fn formats_lists_every_renderer_and_its_options() {
    scribe()
        .arg("formats")
        .assert()
        .success()
        .stdout(predicate::str::contains("\njson\n"))
        .stdout(predicate::str::contains("\nsvg\n"))
        .stdout(predicate::str::contains("\ntemplate\n"))
        .stdout(predicate::str::contains(
            "text_mode  (one of invisible, visible, debug; default \"invisible\")",
        ))
        .stdout(predicate::str::contains(
            "pretty  (true or false; default true)",
        ))
        .stdout(predicate::str::contains(
            "template_source  (text; default \"\")",
        ));
}

#[test]
fn templates_lists_the_built_in_templates() {
    scribe().arg("templates").assert().success().stdout(
        "html-overlay\nsvg-overlay\nhtml-figure\nsr-only-transcript\nfigure-transcript\n\
json-ld\nlayout-json\nhocr\nalto\nmarkdown\ntext\nalt-text\n",
    );
}

#[test]
fn schema_prints_the_layout_schema() {
    let output = scribe().arg("schema").assert().success();
    let schema: serde_json::Value =
        serde_json::from_slice(&output.get_output().stdout).expect("the schema is JSON");
    assert_eq!(schema["title"], "Layout");
}

#[test]
fn a_layout_renders_through_a_template_without_any_model() {
    scribe()
        .args(["render", "--format", "template", "--opt", "template=hocr"])
        .arg(fixture("hello.layout.json"))
        .assert()
        .success()
        .stdout(predicate::str::contains("class=\"ocrx_word\""))
        .stdout(predicate::str::contains("Hello"))
        .stdout(predicate::str::contains("bbox 0 0 284 96"));
}

#[test]
fn a_layout_renders_as_a_text_layer_without_the_image() {
    scribe()
        .args(["render", "--no-image"])
        .arg(fixture("hello.layout.json"))
        .assert()
        .success()
        .stdout(predicate::str::starts_with("<?xml"))
        .stdout(predicate::str::contains(">Hello</tspan>"))
        .stdout(predicate::str::contains("<image").not());
}

#[test]
fn a_layout_that_carries_the_image_renders_with_it() {
    let embedded = work_directory("render-a-self-contained-layout").join("hello.json");
    scribe()
        .args(["render", "--format", "json", "--opt", "include_image=true"])
        .arg(fixture("hello.layout.json"))
        .arg("--image")
        .arg(fixture("hello.png"))
        .arg("-o")
        .arg(&embedded)
        .assert()
        .success();
    assert!(
        std::fs::read_to_string(&embedded)
            .expect("the document was written")
            .contains(r#""image_data_uri": "data:image/png;base64,"#)
    );

    scribe()
        .arg("render")
        .arg(&embedded)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            r#"<image href="data:image/png;base64,"#,
        ))
        .stdout(predicate::str::contains(">Hello</tspan>"));
}

#[test]
fn rendering_writes_where_it_is_told() {
    let out = work_directory("render-to-a-file").join("layout.txt");
    scribe()
        .args(["render", "--format", "template", "--opt", "template=text"])
        .arg(fixture("hello.layout.json"))
        .arg("-o")
        .arg(&out)
        .assert()
        .success()
        .stdout("");
    assert_eq!(
        std::fs::read_to_string(&out).expect("the output was written"),
        "Hello World\n"
    );
}

#[test]
fn a_missing_model_says_where_models_come_from() {
    scribe()
        .arg("ocr")
        .arg(fixture("hello.png"))
        .assert()
        .code(USAGE)
        .stderr(predicate::str::contains("--detection-model"))
        .stderr(predicate::str::contains(DETECTION_VARIABLE))
        .stderr(predicate::str::contains("scribe never downloads models"))
        .stderr(predicate::str::contains("ocrs"));
}

#[test]
fn a_format_nobody_offers_is_a_usage_error() {
    scribe()
        .args(["render", "--format", "postscript"])
        .arg(fixture("hello.layout.json"))
        .assert()
        .code(USAGE)
        .stderr(predicate::str::contains(
            "there is no `postscript` format; scribe renders json, svg, template",
        ));
}

#[test]
fn an_option_the_format_does_not_take_is_a_usage_error() {
    scribe()
        .args(["render", "--opt", "nosuch=1"])
        .arg(fixture("hello.layout.json"))
        .assert()
        .code(USAGE)
        .stderr(predicate::str::contains(
            "the svg renderer has no `nosuch` option",
        ));
}

#[test]
fn an_option_value_of_the_wrong_kind_is_a_usage_error() {
    scribe()
        .args(["render", "--opt", "precision=deep"])
        .arg(fixture("hello.layout.json"))
        .assert()
        .code(USAGE)
        .stderr(predicate::str::contains("takes a whole number"));
}

#[test]
fn an_option_that_is_not_a_pair_is_a_usage_error() {
    scribe()
        .args(["render", "--opt", "precision"])
        .arg(fixture("hello.layout.json"))
        .assert()
        .code(USAGE)
        .stderr(predicate::str::contains(
            "`--opt precision` is not of the form `name=value`",
        ));
}

#[test]
fn a_shorthand_the_format_does_not_take_names_the_flag_that_set_it() {
    scribe()
        .args(["render", "--format", "json", "--debug"])
        .arg(fixture("hello.layout.json"))
        .assert()
        .code(USAGE)
        .stderr(predicate::str::contains(
            "`--debug` sets the `text_mode` option, which the `json` format does not take",
        ));
}

#[test]
fn every_fixture_layout_renders_into_a_directory() {
    let directory = work_directory("render-every-fixture");
    let mut scribe = scribe();
    scribe
        .args(["render", "--no-image", "--out-dir"])
        .arg(&directory);
    for stem in FIXTURES {
        scribe.arg(fixture(&format!("{stem}.layout.json")));
    }
    scribe.assert().success().stdout("");

    let mut written: Vec<String> = std::fs::read_dir(&directory)
        .expect("the directory was made")
        .map(|entry| entry.expect("the directory can be read").file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .collect();
    written.sort();
    let mut expected: Vec<String> = FIXTURES.iter().map(|stem| format!("{stem}.svg")).collect();
    expected.sort();
    assert_eq!(written, expected, "one output per layout, named after it");

    for stem in FIXTURES {
        let path = directory.join(format!("{stem}.svg"));
        let svg = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("{} cannot be read: {error}", path.display()));
        let document = roxmltree::Document::parse(&svg).unwrap_or_else(|error| {
            panic!("{} should be well-formed XML: {error}", path.display())
        });
        assert_eq!(document.root_element().tag_name().name(), "svg");
    }
}

#[test]
fn several_layouts_need_somewhere_to_put_their_outputs() {
    scribe()
        .args(["render", "one.layout.json", "two.layout.json"])
        .assert()
        .code(USAGE)
        .stderr(predicate::str::contains("needs --out-dir"));
}

#[test]
fn one_image_cannot_stand_for_several_layouts() {
    scribe()
        .args(["render", "one.layout.json", "two.layout.json"])
        .args(["--out-dir", "out", "--image", "one.png"])
        .assert()
        .code(USAGE)
        .stderr(predicate::str::contains("--image names one image"));
}

#[test]
fn several_images_need_somewhere_to_put_their_outputs() {
    scribe()
        .args(["ocr", "one.png", "two.png"])
        .assert()
        .code(USAGE)
        .stderr(predicate::str::contains("needs --out-dir"));
}

#[test]
fn one_output_path_cannot_hold_several_layouts() {
    scribe()
        .args(["ocr", "one.png", "two.png", "--out-dir", "out"])
        .arg("--layout-json")
        .arg("layouts.json")
        .assert()
        .code(USAGE)
        .stderr(predicate::str::contains("for one image only"));
}

#[test]
fn writing_to_one_file_and_to_a_directory_at_once_is_a_usage_error() {
    scribe()
        .args(["ocr", "one.png", "-o", "out.svg", "--out-dir", "out"])
        .assert()
        .code(USAGE);
}

#[test]
fn a_document_that_is_not_a_layout_is_a_processing_error() {
    scribe()
        .arg("render")
        .arg(fixture("hello.png"))
        .assert()
        .code(FAILURE)
        .stderr(predicate::str::contains("not UTF-8 text"));
}

#[test]
fn a_layout_that_is_not_there_is_a_processing_error() {
    scribe()
        .args(["render", "nowhere/at/all.json"])
        .assert()
        .code(FAILURE)
        .stderr(predicate::str::contains("cannot be read"));
}

#[test]
fn every_command_has_help() {
    for command in ["ocr", "render", "formats", "templates", "schema"] {
        scribe()
            .args([command, "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Usage: scribe"));
    }
}

#[test]
fn reading_an_image_writes_an_svg_to_standard_output() {
    let Some((detection, recognition)) = models() else {
        return;
    };
    scribe()
        .arg("ocr")
        .arg(fixture("hello.png"))
        .arg("--detection-model")
        .arg(detection)
        .arg("--recognition-model")
        .arg(recognition)
        .assert()
        .success()
        .stdout(predicate::str::starts_with("<?xml"))
        .stdout(predicate::str::contains(">Hello</tspan>"))
        .stdout(predicate::str::contains("data:image/png;base64,"));
}

#[test]
fn each_image_in_a_directory_is_named_after_the_image_it_came_from() {
    let Some(mut scribe) = scribe_with_models() else {
        return;
    };
    let directory = work_directory("one-output-per-image");
    scribe
        .args(["ocr", "--format", "template", "--opt", "template=text"])
        .arg(fixture("hello.png"))
        .arg("--out-dir")
        .arg(&directory)
        .arg("--layout-json")
        .assert()
        .success()
        .stdout("");

    assert_eq!(
        std::fs::read_to_string(directory.join("hello.txt")).expect("the text was written"),
        "Hello World\n"
    );
    let layout = std::fs::read_to_string(directory.join("hello.layout.json"))
        .expect("the layout was written");
    assert!(
        layout.contains("\"Hello World\""),
        "the layout should carry the line it read, but is {layout}"
    );
}
