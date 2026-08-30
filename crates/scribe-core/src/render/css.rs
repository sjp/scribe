//! What a value has to be to go into a stylesheet, a selector or a name.
//!
//! A document that carries a text layer carries a stylesheet with it, and the
//! caller's own colours, font families and class names are written into that
//! stylesheet unescaped: escaping is what a document does to its text, and a
//! stylesheet is not text. An escaped name would select something else, and an
//! escape inside a `<style>` does not mean the same thing to the XML parser
//! reading a standalone document as to the HTML parser reading an inlined one.
//!
//! So a value is refused rather than escaped. Each check here says why it
//! cannot be written, in a sentence that follows the value; whoever calls it
//! says which value it was.

/// The characters a value written into a stylesheet may not hold, each of
/// which could end the declaration, the rule or the element it sits in.
const FORBIDDEN: &[char] = &['<', '>', '&', '{', '}', ';', '@', '\\'];

/// Checks that a value can be written into a name, rather than escaped into
/// one: a name lands in a selector, where the escaped form would name
/// something else.
///
/// # Errors
///
/// Returns the reason when the value holds a character a CSS identifier
/// cannot.
pub(super) fn check_ident(value: &str) -> Result<(), String> {
    match value
        .chars()
        .find(|character| !(character.is_ascii_alphanumeric() || matches!(character, '-' | '_')))
    {
        Some(character) => Err(format!("a CSS identifier cannot hold {character:?}")),
        None => Ok(()),
    }
}

/// Checks that a name a document actually writes begins the way a CSS
/// identifier must: not with a digit, and not with a hyphen followed by one.
///
/// # Errors
///
/// Returns the reason, naming the composed value, when it begins any other
/// way.
pub(super) fn check_ident_start(value: &str) -> Result<(), String> {
    let mut characters = value.chars();
    let first = characters.next();
    let starts = match (first, characters.next()) {
        (Some('-'), Some(second)) => !second.is_ascii_digit(),
        (Some('-'), None) => false,
        (Some(first), _) => !first.is_ascii_digit(),
        (None, _) => true,
    };
    if starts {
        return Ok(());
    }
    Err(format!(
        "`{value}` is not a valid CSS identifier, which cannot begin with a digit"
    ))
}

/// Checks that a value can stand in a declaration without ending it, so that
/// no colour and no font family can write a rule of its own into a stylesheet
/// that reaches the whole of the page the document is placed in.
///
/// # Errors
///
/// Returns the reason when the value holds a character that could close what
/// it is written into, opens a comment, or leaves a bracket or a quote
/// unclosed.
pub(super) fn check_value(value: &str) -> Result<(), String> {
    if let Some(character) = value
        .chars()
        .find(|character| FORBIDDEN.contains(character) || (*character as u32) < 0x20)
    {
        return Err(format!(
            "a value written into a stylesheet cannot hold {character:?}"
        ));
    }
    if value.contains("/*") || value.contains("*/") {
        return Err("a value written into a stylesheet cannot open or close a comment".to_string());
    }
    let mut depth = 0_i32;
    for character in value.chars() {
        match character {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        if depth < 0 {
            return Err(
                "a value written into a stylesheet cannot close a bracket it did not open"
                    .to_string(),
            );
        }
    }
    if depth != 0 {
        return Err(
            "a value written into a stylesheet has to close every bracket it opens".to_string(),
        );
    }
    for quote in ['"', '\''] {
        if value.matches(quote).count() % 2 != 0 {
            return Err(format!(
                "a value written into a stylesheet has to close the {quote} it opens"
            ));
        }
    }
    Ok(())
}
