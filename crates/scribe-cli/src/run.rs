//! Carrying out what the command line asked for.
//!
//! The work itself belongs to `scribe-core`; what is here is the order it
//! happens in, the timings worth reporting, and the translation between a
//! person's flags and a renderer's options.

use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result};
use image::ImageFormat;
use scribe_core::image_source::ImageSource;
use scribe_core::layout::Layout;
use scribe_core::ocr::{DecodeMethod, Engine, OcrOptions, PixelImage};
use scribe_core::render::{
    OptionSpec, Options, Registry, RenderOutput, Renderer, list_templates, registry,
};

use crate::cli::{
    Cli, Command, OcrCommand, OutputArgs, RecognitionArgs, RenderArgs, RenderCommand,
};
use crate::error::usage;
use crate::io::{self, Destination};
use crate::models;

/// The option `--template-file` sets.
const TEMPLATE_SOURCE: &str = "template_source";

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
    let engine = {
        // The engine keeps its own copy of the models, and they are large
        // enough to be worth letting go of before any image is read.
        let models = models::load(&model_args)?;
        Engine::new(&models, ocr_options(&recognition)).context("the models could not be loaded")?
    };
    log::info!("loaded the models in {:.1?}", started.elapsed());

    for image in &images {
        one_image(image, &engine, renderer, &options, &render, &output)?;
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

    let source = ImageSource {
        width: decoded.width,
        height: decoded.height,
        mime: mime_of(&encoded),
        bytes: Some(&encoded),
        href: href(render, Some(path)),
    };

    let started = Instant::now();
    let document = renderer.render(&layout, &source, options)?;
    log::info!(
        "{name}: rendered {} bytes of {} in {:.1?}",
        document.bytes.len(),
        document.mime,
        started.elapsed()
    );

    let destination = destination(output, path, &document);
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

/// Renders a layout that was read earlier, without loading any model.
fn render(command: RenderCommand) -> Result<()> {
    let RenderCommand {
        layout: path,
        image,
        out,
        render,
    } = command;

    let registry = registry();
    let renderer = choose(&registry, render.format())?;
    let options = options(renderer, &render)?;

    let json = io::read_text(&path)?;
    let layout = Layout::from_json(&json)
        .with_context(|| format!("{} is not a layout document", io::shown(&path)))?;

    let encoded = image.as_deref().map(io::read).transpose()?;
    let source = ImageSource {
        width: layout.image.width,
        height: layout.image.height,
        mime: encoded.as_deref().and_then(mime_of),
        bytes: encoded.as_deref(),
        href: href(&render, image.as_deref()),
    };

    let started = Instant::now();
    let document = renderer.render(&layout, &source, &options)?;
    log::info!(
        "rendered {} bytes of {} in {:.1?}",
        document.bytes.len(),
        document.mime,
        started.elapsed()
    );

    let destination = match out {
        Some(path) if io::is_stream(&path) => Destination::Stdout,
        Some(path) => Destination::File(path),
        None => Destination::Stdout,
    };
    destination.write(&document.bytes)
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
/// it: whatever `--link-href` said, or the path as it was written.
fn href<'a>(args: &'a RenderArgs, image: Option<&'a Path>) -> Option<&'a str> {
    args.link_href.as_deref().or_else(|| {
        image
            .filter(|path| !io::is_stream(path))
            .and_then(Path::to_str)
    })
}

/// The media type of an encoded image, as far as its bytes give it away.
fn mime_of(encoded: &[u8]) -> Option<&'static str> {
    image::guess_format(encoded)
        .ok()
        .map(|format| ImageFormat::to_mime_type(&format))
}

/// Where one rendered document goes.
fn destination(output: &OutputArgs, input: &Path, document: &RenderOutput) -> Destination {
    if let Some(directory) = &output.out_dir {
        let name = format!("{}.{}", io::stem(input), document.extension);
        return Destination::File(directory.join(name));
    }
    match &output.out {
        Some(path) if io::is_stream(path) => Destination::Stdout,
        Some(path) => Destination::File(path.clone()),
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
