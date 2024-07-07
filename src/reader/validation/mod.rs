//! Validation iterators for performing on-demand checks of the correctness of the XML events.

use std::fmt;

/// An error returned if [well-formedless constraint][WFC] or [validaty constraint][VC]
/// is violated.
///
/// [WFC]: https://www.w3.org/TR/xml11/#dt-wfc
/// [VC]: https://www.w3.org/TR/xml11/#dt-vc
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ValidationError {
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
