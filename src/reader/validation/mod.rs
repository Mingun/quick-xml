use std::fmt;
use std::str::Chars;

/// An error returned if [well-formedless constraint][WFC] or [validaty constraint][VC]
/// is violated.
///
/// [WFC]: https://www.w3.org/TR/xml11/#dt-wfc
/// [VC]: https://www.w3.org/TR/xml11/#dt-vc
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ValidationError {
    /// Input contains character which is not allowed in XML.
    RestrictedChar(char),
    /// The name of an element or target of a processing instruction are empty.
    EmptyName,
    /// The name of an element, target of a processing instruction contains characters
    /// that are not allowed in names.
    InvalidName,
    /// The parser started to parse `<!`, but the input ended before it can recognize
    /// anything.
    UnknownMarkup,
    /// The parser started to parse entity or character reference (`&...;`) in text,
    /// but the input ended before the closing `;` character was found.
    UnclosedReference,
    /// A comment contains forbidden double-hyphen (`--`) sequence inside.
    ///
    /// According to the [specification], for compatibility, comments MUST NOT contain
    /// double-hyphen (`--`) sequence, in particular, they cannot end by `--->`.
    ///
    /// [specification]: https://www.w3.org/TR/xml11/#sec-comments
    DoubleHyphenInComment,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::EmptyName => f.write_str("empty name"),
            Self::InvalidName => f.write_str("invalid character in name"),
            Self::RestrictedChar(ch) => write!(
                f,
                "character `{}` (0x{:x}) are not allowed in XML",
                ch, *ch as u32
            ),
            Self::UnclosedReference => f.write_str(
                "entity or character reference not closed: `;` not found before end of input",
            ),
            Self::DoubleHyphenInComment => f.write_str(
                "forbidden string `--` was found in a comment"
            ),
            _ => write!(f, "{:?}", self), // TODO: implement correct text errors
        }
    }
}

impl std::error::Error for ValidationError {}

////////////////////////////////////////////////////////////////////////////////////////////////////

mod cdata;
mod comment;
mod name;
mod text;

pub use cdata::*;
pub use comment::*;
pub use name::*;
pub use text::*;

/// Checks if the character corresponds to the [`Char`] production of
/// the XML 1.0 specification.
///
/// Any Unicode character, excluding the surrogate blocks, FFFE, and FFFF.
///
/// [`Char`]: https://www.w3.org/TR/xml/#NT-Char
pub fn is_xml10_char(ch: char) -> bool {
    matches!(ch,
        '\u{9}' | '\u{A}' | '\u{D}'
        | '\u{0020}'..='\u{D7FF}'
        | '\u{E000}'..='\u{FFFD}'
        | '\u{10000}'..='\u{10FFFF}'
    )
}

/// Checks if the character corresponds to the [`Char`] production of
/// the XML 1.1 specification.
///
/// Any Unicode character, excluding the surrogate blocks, FFFE, and FFFF.
///
/// [`Char`]: https://www.w3.org/TR/xml11/#NT-Char
pub fn is_xml11_char(ch: char) -> bool {
    matches!(ch,
        | '\u{0001}'..='\u{D7FF}'
        | '\u{E000}'..='\u{FFFD}'
        | '\u{10000}'..='\u{10FFFF}'
    )
}

/// Checks if the specified character cannot be present in the XML either literally
/// or as a _character reference_ [according to the rules].
///
/// [according to the rules]: https://www.w3.org/TR/xml11/#NT-RestrictedChar
#[inline]
pub fn is_xml11_discouraged_char(ch: char) -> bool {
    matches!(ch, '\u{0}'
        // Restricted characters
        | '\u{01}'..='\u{08}'
        | '\u{0b}'..='\u{0c}'
        | '\u{0e}'..='\u{1f}'
        | '\u{7f}'..='\u{84}'
        | '\u{86}'..='\u{9f}'

        // Discouraged characters
        // Up to FDEF instead of FDDF -- see https://www.w3.org/XML/xml-V11-2e-errata
        | '\u{FDD0}'..='\u{FDEF}'
        // The characters below are permitted in names according to the
        // Name definition: https://www.w3.org/TR/xml11/#NT-Name
        | '\u{1FFFE}'..='\u{1FFFF}'
        | '\u{2FFFE}'..='\u{2FFFF}'
        | '\u{3FFFE}'..='\u{3FFFF}'
        | '\u{4FFFE}'..='\u{4FFFF}'
        | '\u{5FFFE}'..='\u{5FFFF}'
        | '\u{6FFFE}'..='\u{6FFFF}'
        | '\u{7FFFE}'..='\u{7FFFF}'
        | '\u{8FFFE}'..='\u{8FFFF}'
        | '\u{9FFFE}'..='\u{9FFFF}'
        | '\u{AFFFE}'..='\u{AFFFF}'
        | '\u{BFFFE}'..='\u{BFFFF}'
        | '\u{CFFFE}'..='\u{CFFFF}'
        | '\u{DFFFE}'..='\u{DFFFF}'
        | '\u{EFFFE}'..='\u{EFFFF}'
        | '\u{FFFFE}'..='\u{FFFFF}'
        | '\u{10FFFE}'..='\u{10FFFF}'
    )
}

fn valid_chars(iter: &mut Chars) -> Option<ValidationError> {
    for ch in iter {
        if !is_xml11_char(ch) {
            return Some(ValidationError::RestrictedChar(ch));
        }
    }
    None
}
