//! Carrying out what the command line asked for.
//!
//! The work itself belongs to `scribe-core`; what is here is the order it
//! happens in, the timings worth reporting, and the translation between a
//! person's flags and a renderer's options.

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use image::ImageFormat;
use scribe_core::image_source::ImageSource;
use scribe_core::layout::{Layout, LayoutDocument};
use scribe_core::ocr::{DecodeMethod, Engine, OcrOptions, PixelImage};
use scribe_core::render::{
    OptionSpec, Options, Registry, RenderError, RenderOutput, Renderer, list_templates, registry,
};

use crate::cli::{
    Cli, Command, OcrCommand, OutputArgs, RecognitionArgs, RenderArgs, RenderCommand,
};
use crate::error::{self, usage};
use crate::io::{self, Destination};
use crate::models;

/// The option `--template-file` sets.
const TEMPLATE_SOURCE: &str = "template_source";

/// The option `--embed`, `--link` and `--no-image` set.
const IMAGE_MODE: &str = "image_mode";

/// How wide the lines of the format listing are.
const LISTING_WIDTH: usize = 74;

/// Does what the command line said, or explains why it could not.
pub fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Ocr(command) => ocr(command),
        Command::Render(command) => render(command),
        Command::Formats => formats(),
        Command::Templates => templates(),
        Command::Schema => schema(),
    }
}

/// Reads every image with one engine and renders each result.
fn ocr(command: OcrCommand) -> Result<()> {
    let OcrCommand {
        images,
        models: model_args,
        recognition,
        output,
        batch,
        render,
    } = command;

    if images.len() > 1 {
        if output.out_dir.is_none() {
            return Err(usage(
                "reading several images needs --out-dir, since -o names one file",
            ));
        }
        if matches!(output.layout_json, Some(Some(_))) {
            return Err(usage(
                "--layout-json takes a path for one image only; give it no path to write a layout beside each output",
            ));
        }
    }

    let registry = registry();
    let renderer = choose(&registry, render.format())?;
    let options = options(renderer, &render)?;
    if let Some(directory) = &output.out_dir {
        io::make_directory(directory)?;
    }

    let started = Instant::now();
    let engine = Engine::new(models::load(&model_args)?, ocr_options(&recognition))
        .context("the models could not be loaded")?;
    log::info!("loaded the models in {:.1?}", started.elapsed());

    each_input(&images, "images", batch.fail_fast, |image| {
        one_image(image, &engine, renderer, &options, &render, &output)
    })
}

/// Does something to every input, and says at the end how much of it failed.
///
/// One unreadable file among many leaves the rest still worth doing, so a
/// failure over one input is reported as it happens, named, and the run goes
/// on; what did not come out is counted at the end and the run leaves with a
/// failing status. A single input, or `--fail-fast`, stops at the first
/// failure instead. So does a mistake in the request noticed only once an
/// input is under way, since it is the same mistake for every one of them.
fn each_input(
    inputs: &[PathBuf],
    kind: &str,
    fail_fast: bool,
    mut one: impl FnMut(&Path) -> Result<()>,
) -> Result<()> {
    if inputs.len() < 2 || fail_fast {
        return inputs.iter().try_for_each(|path| one(path));
    }

    let mut failed = 0;
    for path in inputs {
        if let Err(error) = one(path) {
            if error::is_usage(&error) {
                return Err(error);
            }
            error::report_about(&io::shown(path), &error);
            failed += 1;
        }
    }
    if failed > 0 {
        bail!("{failed} of {} {kind} could not be processed", inputs.len());
    }
    Ok(())
}

/// Reads one image and writes what it was rendered into.
fn one_image(
    path: &Path,
    engine: &Engine,
    renderer: &dyn Renderer,
    options: &Options,
    render: &RenderArgs,
    output: &OutputArgs,
) -> Result<()> {
    let name = io::shown(path);
    let encoded = io::read(path)?;

    let started = Instant::now();
    let decoded =
        PixelImage::decode(&encoded).with_context(|| format!("{name} could not be decoded"))?;
    log::info!(
        "{name}: decoded {} by {} pixels in {:.1?}",
        decoded.width,
        decoded.height,
        started.elapsed()
    );

    let started = Instant::now();
    let layout = engine
        .analyze(&decoded.as_pixel_image())
        .with_context(|| format!("{name} could not be read"))?;
    log::info!(
        "{name}: recognised {} lines in {:.1?}",
        layout.lines.len(),
        started.elapsed()
    );

    let link = href(
        render,
        Some(path),
        output_directory(output.out_dir.as_deref(), output.out.as_deref()).as_deref(),
    );
    let source = ImageSource {
        width: decoded.width,
        height: decoded.height,
        mime: mime_of(&encoded),
        bytes: Some(&encoded),
        href: link.as_deref(),
    };

    let started = Instant::now();
    let document = renderer.render(&layout, &source, options)?;
    log::info!(
        "{name}: rendered {} bytes of {} in {:.1?}",
        document.bytes.len(),
        document.mime,
        started.elapsed()
    );

    let destination = destination(
        output.out_dir.as_deref(),
        output.out.as_deref(),
        &io::stem(path),
        &document,
    );
    destination.write(&document.bytes)?;
    log::debug!("{name}: wrote {}", destination.shown());

    if let Some(asked) = &output.layout_json {
        let path = match asked {
            Some(given) => given.clone(),
            None => destination.layout_beside(path).ok_or_else(|| {
                usage(
                    "--layout-json needs a path when the image is read from standard input and the output goes to standard output",
                )
            })?,
        };
        let mut json = layout
            .to_json_pretty()
            .context("the layout could not be written as JSON")?;
        json.push('\n');
        Destination::File(path.clone()).write(json.as_bytes())?;
        log::debug!("{name}: wrote {}", path.display());
    }
    Ok(())
}

/// Renders layouts that were read earlier, without loading any model.
fn render(command: RenderCommand) -> Result<()> {
    let RenderCommand {
        layouts,
        image,
        out,
        out_dir,
        batch,
        render,
    } = command;

    if layouts.len() > 1 {
        if out_dir.is_none() {
            return Err(usage(
                "rendering several layouts needs --out-dir, since -o names one file",
            ));
        }
        if image.is_some() {
            return Err(usage(
                "--image names one image, so it goes with one layout; the layouts of several images are rendered one at a time",
            ));
        }
    }

    let registry = registry();
    let renderer = choose(&registry, render.format())?;
    let options = options(renderer, &render)?;
    if let Some(directory) = &out_dir {
        io::make_directory(directory)?;
    }
    let encoded = match image.as_deref() {
        Some(path) => Some(readable_image(path)?),
        None => None,
    };

    // Nothing on the command line said what to do with the image and none was
    // named, so the plainest command writes the text layer on its own rather
    // than stopping over a picture nobody asked for. Whether a layout carries
    // its own image is not known until it is read, so that half of the
    // question is asked again for each one; the library keeps `embed`.
    let leave_image_out = takes_option(renderer, IMAGE_MODE)
        && encoded.is_none()
        && options.get(IMAGE_MODE).is_none();

    each_input(&layouts, "layouts", batch.fail_fast, |path| {
        let name = io::shown(path);
        let json = io::read_text(path)?;
        let LayoutDocument {
            layout,
            image: carried,
        } = LayoutDocument::from_json(&json)
            .with_context(|| format!("{name} could not be read as a layout"))?;

        // A document written with the image inside it renders on its own; an
        // image named on the command line is the one that was asked for, so
        // it wins over the one the document happens to carry.
        let (mime, bytes) = match (encoded.as_deref(), carried.as_ref()) {
            (Some(encoded), _) => (mime_of(encoded), Some(encoded)),
            (None, Some(carried)) => (Some(carried.mime.as_str()), Some(carried.bytes.as_slice())),
            (None, None) => (None, None),
        };
        let link = href(
            &render,
            image.as_deref(),
            output_directory(out_dir.as_deref(), out.as_deref()).as_deref(),
        );
        let source = ImageSource {
            width: layout.image.width,
            height: layout.image.height,
            mime,
            bytes,
            href: link.as_deref(),
        };
        let options = if leave_image_out && carried.is_none() {
            log::info!("{name}: no image was given, so the text layer stands on its own");
            Cow::Owned(options.clone().with(IMAGE_MODE, "none"))
        } else {
            Cow::Borrowed(&options)
        };

        let started = Instant::now();
        let document = renderer
            .render(&layout, &source, &options)
            .map_err(where_the_image_comes_from)?;
        log::info!(
            "{name}: rendered {} bytes of {} in {:.1?}",
            document.bytes.len(),
            document.mime,
            started.elapsed()
        );

        let destination = destination(
            out_dir.as_deref(),
            out.as_deref(),
            &io::layout_stem(path),
            &document,
        );
        destination.write(&document.bytes)?;
        log::debug!("{name}: wrote {}", destination.shown());
        Ok(())
    })
}

/// Lists every renderer this build knows and the options each one takes.
fn formats() -> Result<()> {
    let registry = registry();
    let mut listing = String::from("Set any of these with `--opt name=value`.\n");
    for name in registry.names() {
        let renderer = registry
            .get(name)
            .expect("the registry lists what it holds");
        listing.push_str(&format!("\n{name}\n"));
        let specs = renderer.describe_options();
        if specs.is_empty() {
            listing.push_str("    takes no options\n");
        }
        for spec in specs {
            listing.push_str(&format!("    {}  ({})\n", spec.name, accepts(&spec)));
            for line in wrap(spec.help, LISTING_WIDTH - 8) {
                listing.push_str(&format!("        {line}\n"));
            }
        }
    }
    io::print(listing.as_bytes())
}

/// Lists the templates that ship with scribe.
fn templates() -> Result<()> {
    let listing: String = list_templates()
        .iter()
        .map(|name| format!("{name}\n"))
        .collect();
    io::print(listing.as_bytes())
}

/// Prints the JSON Schema of the layout model.
fn schema() -> Result<()> {
    let mut schema = serde_json::to_string_pretty(&Layout::json_schema())
        .context("the layout schema could not be written")?;
    schema.push('\n');
    io::print(schema.as_bytes())
}

/// The renderer of that name, or a message listing the ones there are.
fn choose<'a>(registry: &'a Registry, name: &str) -> Result<&'a dyn Renderer> {
    registry.get(name).ok_or_else(|| {
        usage(format!(
            "there is no `{name}` format; scribe renders {}",
            registry.names().join(", ")
        ))
    })
}

/// The options for a render: the named flags first, then `--opt`, which
/// overrides them.
///
/// A flag standing for an option the chosen format does not take is refused
/// here, in the words it was written in, rather than reaching the renderer as
/// an option name nobody typed.
fn options(renderer: &dyn Renderer, args: &RenderArgs) -> Result<Options> {
    let specs = renderer.describe_options();
    let takes = |option: &str| specs.iter().any(|spec| spec.name == option);
    let mut options = Options::new();

    for shorthand in args.shorthands() {
        if !takes(shorthand.option) {
            return Err(unsupported(renderer, shorthand.flag, shorthand.option));
        }
        options.set(shorthand.option, shorthand.value);
    }
    if let Some(path) = &args.template_file {
        if !takes(TEMPLATE_SOURCE) {
            return Err(unsupported(renderer, "--template-file", TEMPLATE_SOURCE));
        }
        options.set(TEMPLATE_SOURCE, io::read_text(path)?);
    }
    for opt in &args.opts {
        let (name, value) = opt
            .split_once('=')
            .ok_or_else(|| usage(format!("`--opt {opt}` is not of the form `name=value`")))?;
        options.set(name, value);
    }
    Ok(options)
}

/// Whether a renderer takes an option of that name.
fn takes_option(renderer: &dyn Renderer, option: &str) -> bool {
    renderer
        .describe_options()
        .iter()
        .any(|spec| spec.name == option)
}

/// The bytes of an image the run was given, refused here rather than half way
/// through a render if nothing about them says what kind of image it is.
fn readable_image(path: &Path) -> Result<Vec<u8>> {
    let encoded = io::read(path)?;
    if mime_of(&encoded).is_none() {
        let name = io::shown(path);
        return Err(usage(format!(
            "the media type of {name} could not be worked out from its bytes, so it cannot be embedded or described; convert it to PNG or JPEG and pass that"
        )));
    }
    Ok(encoded)
}

/// Says which flags supply an image to a renderer that wanted one and was not
/// given it, and leaves every other failure to speak for itself.
fn where_the_image_comes_from(error: RenderError) -> anyhow::Error {
    let missing = matches!(error, RenderError::MissingImage { .. });
    let error = anyhow::Error::new(error);
    if missing {
        error.context("pass `--image PATH`, or `--no-image` to leave it out")
    } else {
        error
    }
}

/// The message for a flag whose option the chosen format has never heard of.
fn unsupported(renderer: &dyn Renderer, flag: &str, option: &str) -> anyhow::Error {
    usage(format!(
        "`{flag}` sets the `{option}` option, which the `{}` format does not take",
        renderer.name()
    ))
}

/// How the engine is asked to read.
fn ocr_options(args: &RecognitionArgs) -> OcrOptions {
    OcrOptions {
        alphabet: args.alphabet.clone(),
        allowed_chars: args.allowed_chars.clone(),
        decode_method: match args.beam_width {
            Some(width) => DecodeMethod::BeamSearch { width },
            None => DecodeMethod::Greedy,
        },
        include_chars: !args.no_chars,
    }
}

/// What an output points at when it links to the image rather than carrying
/// it.
///
/// `--link-href` is written out as it was given, whatever it says. Otherwise
/// it is the path of the image the run was given, spelled out from the
/// directory the output lands in, since that is what a reader of the output
/// resolves it against. An output going to standard output has no directory
/// to be read from, so there the path stands as it was typed.
fn href(args: &RenderArgs, image: Option<&Path>, directory: Option<&Path>) -> Option<String> {
    if let Some(given) = &args.link_href {
        return Some(given.clone());
    }
    let image = image.filter(|path| !io::is_stream(path))?;
    let as_typed = || image.to_str().map(str::to_owned);
    match directory {
        Some(directory) => io::href_from(directory, image).or_else(as_typed),
        None => as_typed(),
    }
}

/// The directory a rendered document is written into, or `None` when it goes
/// to standard output.
///
/// What the file is called is not settled until the render is over, since the
/// extension is the renderer's to choose, but the directory is known before
/// it starts — and the directory is all a link out of the document is
/// resolved against.
fn output_directory(out_dir: Option<&Path>, out: Option<&Path>) -> Option<PathBuf> {
    if let Some(directory) = out_dir {
        return Some(directory.to_path_buf());
    }
    match out {
        Some(path) if io::is_stream(path) => None,
        Some(path) => Some(path.parent().unwrap_or(Path::new("")).to_path_buf()),
        None => None,
    }
}

/// The media type of an encoded image, as far as its bytes give it away.
fn mime_of(encoded: &[u8]) -> Option<&'static str> {
    image::guess_format(encoded)
        .ok()
        .map(|format| ImageFormat::to_mime_type(&format))
}

/// Where one rendered document goes: into the directory outputs are gathered
/// in, under the name its input had, or wherever a single output was asked
/// for.
fn destination(
    out_dir: Option<&Path>,
    out: Option<&Path>,
    stem: &str,
    document: &RenderOutput,
) -> Destination {
    if let Some(directory) = out_dir {
        return Destination::File(directory.join(format!("{stem}.{}", document.extension)));
    }
    match out {
        Some(path) if io::is_stream(path) => Destination::Stdout,
        Some(path) => Destination::File(path.to_path_buf()),
        None => Destination::Stdout,
    }
}

/// How an option's listing describes the values it accepts.
fn accepts(spec: &OptionSpec) -> String {
    let kind = if spec.choices.is_empty() {
        spec.kind.to_string()
    } else {
        format!("one of {}", spec.choices.join(", "))
    };
    format!("{kind}; default {}", spec.default)
}

/// Breaks text into lines of at most `width` characters, at the spaces.
///
/// A word longer than the width goes on a line of its own rather than being
/// cut in half.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_wrapped_at_spaces() {
        assert_eq!(wrap("one two three four", 9), ["one two", "three", "four"]);
    }

    #[test]
    fn a_word_longer_than_the_width_keeps_a_line_to_itself() {
        assert_eq!(wrap("a lengthening b", 4), ["a", "lengthening", "b"]);
    }

    #[test]
    fn text_that_fits_stays_on_one_line() {
        assert_eq!(wrap("short enough", 40), ["short enough"]);
    }

    #[test]
    fn an_option_listing_names_its_choices_and_its_default() {
        let registry = registry();
        let svg = registry.get("svg").expect("svg is built in");
        let specs = svg.describe_options();
        let text_mode = specs
            .iter()
            .find(|spec| spec.name == "text_mode")
            .expect("svg has a text mode");
        assert_eq!(
            accepts(text_mode),
            "one of invisible, visible, debug; default \"invisible\""
        );
    }
}
