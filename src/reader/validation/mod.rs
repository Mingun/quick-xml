//! Validation iterators for performing on-demand checks of the correctness of the XML events.

use std::fmt;

/// An error returned if [well-formedless constraint][WFC] or [validaty constraint][VC]
/// is violated.
///
/// [WFC]: https://www.w3.org/TR/xml11/#dt-wfc
/// [VC]: https://www.w3.org/TR/xml11/#dt-vc
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ValidationError {
    /// Event contains character which is not allowed in XML.
    RestrictedChar(char),
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
            Self::RestrictedChar(ch) => write!(
                f,
                "character `{}` (0x{:x}) are not allowed in XML",
                ch, *ch as u32
            ),
            Self::DoubleHyphenInComment => {
                f.write_str("discouraged sequence `--` was found in a comment")
            }
        }
    }
}

impl std::error::Error for ValidationError {}

////////////////////////////////////////////////////////////////////////////////////////////////////

mod comment;

pub use comment::CommentValidationIter;

/// Checks if the character corresponds to the [`Char`] production of
/// the XML 1.0 specification.
///
/// Any Unicode character, excluding the surrogate blocks, FFFE, and FFFF.
///
/// [`Char`]: https://www.w3.org/TR/xml/#NT-Char
pub const fn is_xml10_char(ch: char) -> bool {
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
pub const fn is_xml11_char(ch: char) -> bool {
    matches!(
        ch,
        |'\u{0001}'..='\u{D7FF}'| '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}'
    )
}
