//! Manage xml character escapes

use memchr::memchr2_iter;
use std::borrow::Cow;
use std::ops::Range;

#[cfg(test)]
use pretty_assertions::assert_eq;

/// Error for XML escape / unescape.
#[derive(Clone, Debug)]
pub enum EscapeError {
    /// Entity with Null character
    EntityWithNull(Range<usize>),
    /// Unrecognized escape symbol
    UnrecognizedSymbol(Range<usize>, String),
    /// Cannot find `;` after `&`
    UnterminatedEntity(Range<usize>),
    /// Cannot convert Hexa to utf8
    TooLongHexadecimal,
    /// Character is not a valid hexadecimal value
    InvalidHexadecimal(char),
    /// Cannot convert decimal to hexa
    TooLongDecimal,
    /// Character is not a valid decimal value
    InvalidDecimal(char),
    /// Not a valid unicode codepoint
    InvalidCodepoint(u32),
}

impl std::fmt::Display for EscapeError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            EscapeError::EntityWithNull(e) => write!(
                f,
                "Error while escaping character at range {:?}: Null character entity not allowed",
                e
            ),
            EscapeError::UnrecognizedSymbol(rge, res) => write!(
                f,
                "Error while escaping character at range {:?}: Unrecognized escape symbol: {:?}",
                rge, res
            ),
            EscapeError::UnterminatedEntity(e) => write!(
                f,
                "Error while escaping character at range {:?}: Cannot find ';' after '&'",
                e
            ),
            EscapeError::TooLongHexadecimal => write!(f, "Cannot convert hexadecimal to utf8"),
            EscapeError::InvalidHexadecimal(e) => {
                write!(f, "'{}' is not a valid hexadecimal character", e)
            }
            EscapeError::TooLongDecimal => write!(f, "Cannot convert decimal to utf8"),
            EscapeError::InvalidDecimal(e) => write!(f, "'{}' is not a valid decimal character", e),
            EscapeError::InvalidCodepoint(n) => write!(f, "'{}' is not a valid codepoint", n),
        }
    }
}

impl std::error::Error for EscapeError {}

/// Escapes an `&str` and replaces all xml special characters (`<`, `>`, `&`, `'`, `"`)
/// with their corresponding xml escaped value.
///
/// This function performs following replacements:
///
/// | Character | Replacement
/// |-----------|------------
/// | `<`       | `&lt;`
/// | `>`       | `&gt;`
/// | `&`       | `&amp;`
/// | `'`       | `&apos;`
/// | `"`       | `&quot;`
///
/// This function performs following replacements:
///
/// | Character | Replacement
/// |-----------|------------
/// | `<`       | `&lt;`
/// | `>`       | `&gt;`
/// | `&`       | `&amp;`
/// | `'`       | `&apos;`
/// | `"`       | `&quot;`
pub fn escape(raw: &str) -> Cow<str> {
    _escape(raw, |ch| matches!(ch, b'<' | b'>' | b'&' | b'\'' | b'\"'))
}

/// Escapes an `&str` and replaces xml special characters (`<`, `>`, `&`)
/// with their corresponding xml escaped value.
///
/// Should only be used for escaping text content. In XML text content, it is allowed
/// (though not recommended) to leave the quote special characters `"` and `'` unescaped.
///
/// This function performs following replacements:
///
/// | Character | Replacement
/// |-----------|------------
/// | `<`       | `&lt;`
/// | `>`       | `&gt;`
/// | `&`       | `&amp;`
///
/// This function performs following replacements:
///
/// | Character | Replacement
/// |-----------|------------
/// | `<`       | `&lt;`
/// | `>`       | `&gt;`
/// | `&`       | `&amp;`
pub fn partial_escape(raw: &str) -> Cow<str> {
    _escape(raw, |ch| matches!(ch, b'<' | b'>' | b'&'))
}

/// XML standard [requires] that only `<` and `&` was escaped in text content or
/// attribute value. All other characters not necessary to be escaped, although
/// for compatibility with SGML they also should be escaped. Practically, escaping
/// only those characters is enough.
///
/// This function performs following replacements:
///
/// | Character | Replacement
/// |-----------|------------
/// | `<`       | `&lt;`
/// | `&`       | `&amp;`
///
/// [requires]: https://www.w3.org/TR/xml11/#syntax
pub fn minimal_escape(raw: &str) -> Cow<str> {
    _escape(raw, |ch| matches!(ch, b'<' | b'&'))
}

/// Escapes an `&str` and replaces a subset of xml special characters (`<`, `>`,
/// `&`, `'`, `"`) with their corresponding xml escaped value.
pub(crate) fn _escape<F: Fn(u8) -> bool>(raw: &str, escape_chars: F) -> Cow<str> {
    let bytes = raw.as_bytes();
    let mut escaped = None;
    let mut iter = bytes.iter();
    let mut pos = 0;
    while let Some(i) = iter.position(|&b| escape_chars(b)) {
        if escaped.is_none() {
            escaped = Some(Vec::with_capacity(raw.len()));
        }
        let escaped = escaped.as_mut().expect("initialized");
        let new_pos = pos + i;
        escaped.extend_from_slice(&bytes[pos..new_pos]);
        match bytes[new_pos] {
            b'<' => escaped.extend_from_slice(b"&lt;"),
            b'>' => escaped.extend_from_slice(b"&gt;"),
            b'\'' => escaped.extend_from_slice(b"&apos;"),
            b'&' => escaped.extend_from_slice(b"&amp;"),
            b'"' => escaped.extend_from_slice(b"&quot;"),

            // This set of escapes handles characters that should be escaped
            // in elements of xs:lists, because those characters works as
            // delimiters of list elements
            b'\t' => escaped.extend_from_slice(b"&#9;"),
            b'\n' => escaped.extend_from_slice(b"&#10;"),
            b'\r' => escaped.extend_from_slice(b"&#13;"),
            b' ' => escaped.extend_from_slice(b"&#32;"),
            _ => unreachable!(
                "Only '<', '>','\', '&', '\"', '\\t', '\\r', '\\n', and ' ' are escaped"
            ),
        }
        pos = new_pos + 1;
    }

    if let Some(mut escaped) = escaped {
        if let Some(raw) = bytes.get(pos..) {
            escaped.extend_from_slice(raw);
        }
        // SAFETY: we operate on UTF-8 input and search for an one byte chars only,
        // so all slices that was put to the `escaped` is a valid UTF-8 encoded strings
        // TODO: Can be replaced with `unsafe { String::from_utf8_unchecked() }`
        // if unsafe code will be allowed
        Cow::Owned(String::from_utf8(escaped).unwrap())
    } else {
        Cow::Borrowed(raw)
    }
}

/// Unescape an `&str` and replaces all xml escaped characters (`&...;`) into
/// their corresponding value.
///
/// If feature [`escape-html`] is enabled, then recognizes all [HTML5 escapes].
///
/// [`escape-html`]: ../index.html#escape-html
/// [HTML5 escapes]: https://dev.w3.org/html5/html-author/charref
pub fn unescape(raw: &str) -> Result<Cow<str>, EscapeError> {
    unescape_with(raw, resolve_predefined_entity)
}

/// Unescape an `&str` and replaces all xml escaped characters (`&...;`) into
/// their corresponding value, using a resolver function for custom entities.
///
/// If feature [`escape-html`] is enabled, then recognizes all [HTML5 escapes].
///
/// Predefined entities will be resolved _after_ trying to resolve with `resolve_entity`,
/// which allows you to override default behavior which required in some XML dialects.
///
/// Character references (`&#hh;`) cannot be overridden, they are resolved before
/// calling `resolve_entity`.
///
/// Note, that entities will not be resolved recursively. In order to satisfy the
/// XML [requirements] you should unescape nested entities by yourself.
///
/// # Example
///
/// ```
/// use quick_xml::escape::resolve_xml_entity;
/// # use quick_xml::escape::unescape_with;
/// # use pretty_assertions::assert_eq;
/// let override_named_entities = |entity: &str| match entity {
///     // Override standard entities
///     "lt" => Some("FOO"),
///     "gt" => Some("BAR"),
///     // Resolve custom entities
///     "baz" => Some("&lt;"),
///     // Delegate other entities to the default implementation
///     _ => resolve_xml_entity(entity),
/// };
///
/// assert_eq!(
///     unescape_with("&amp;&lt;test&gt;&baz;", override_named_entities).unwrap(),
///     "&FOOtestBAR&lt;"
/// );
/// ```
///
/// [`escape-html`]: ../index.html#escape-html
/// [HTML5 escapes]: https://dev.w3.org/html5/html-author/charref
/// [requirements]: https://www.w3.org/TR/xml11/#intern-replacement
pub fn unescape_with<'input, 'entity, F>(
    raw: &'input str,
    mut resolve_entity: F,
) -> Result<Cow<'input, str>, EscapeError>
where
    // the lifetime of the output comes from a capture or is `'static`
    F: FnMut(&str) -> Option<&'entity str>,
{
    let bytes = raw.as_bytes();
    let mut unescaped = None;
    let mut last_end = 0;
    let mut iter = memchr2_iter(b'&', b';', bytes);
    while let Some(start) = iter.by_ref().find(|p| bytes[*p] == b'&') {
        match iter.next() {
            Some(end) if bytes[end] == b';' => {
                // append valid data
                if unescaped.is_none() {
                    unescaped = Some(String::with_capacity(raw.len()));
                }
                let unescaped = unescaped.as_mut().expect("initialized");
                unescaped.push_str(&raw[last_end..start]);

                // search for character correctness
                let pat = &raw[start + 1..end];
                if let Some(entity) = pat.strip_prefix('#') {
                    let codepoint = parse_number(entity, start..end)?;
                    unescaped.push_str(codepoint.encode_utf8(&mut [0u8; 4]));
                } else if let Some(value) = resolve_entity(pat) {
                    unescaped.push_str(value);
                } else {
                    return Err(EscapeError::UnrecognizedSymbol(
                        start + 1..end,
                        pat.to_string(),
                    ));
                }

                last_end = end + 1;
            }
            _ => return Err(EscapeError::UnterminatedEntity(start..raw.len())),
        }
    }

    if let Some(mut unescaped) = unescaped {
        if let Some(raw) = raw.get(last_end..) {
            unescaped.push_str(raw);
        }
        Ok(Cow::Owned(unescaped))
    } else {
        Ok(Cow::Borrowed(raw))
    }
}

/// Resolves predefined XML entities or all HTML5 entities depending on the feature
/// [`escape-html`](https://docs.rs/quick-xml/latest/quick_xml/#escape-html).
///
/// Behaves like [`resolve_xml_entity`] if feature is not enabled and as
/// [`resolve_html5_entity`] if enabled.
#[inline]
pub const fn resolve_predefined_entity(entity: &str) -> Option<&'static str> {
    #[cfg(not(feature = "escape-html"))]
    {
        resolve_xml_entity(entity)
    }

    #[cfg(feature = "escape-html")]
    {
        resolve_html5_entity(entity)
    }
}

/// Resolves predefined XML entities. If specified entity is not a predefined XML
/// entity, `None` is returned.
///
/// The complete list of predefined entities are defined in the [specification].
///
/// ```
/// # use quick_xml::escape::resolve_xml_entity;
/// # use pretty_assertions::assert_eq;
/// assert_eq!(resolve_xml_entity("lt"), Some("<"));
/// assert_eq!(resolve_xml_entity("gt"), Some(">"));
/// assert_eq!(resolve_xml_entity("amp"), Some("&"));
/// assert_eq!(resolve_xml_entity("apos"), Some("'"));
/// assert_eq!(resolve_xml_entity("quot"), Some("\""));
///
/// assert_eq!(resolve_xml_entity("foo"), None);
/// ```
///
/// [specification]: https://www.w3.org/TR/xml11/#sec-predefined-ent
pub const fn resolve_xml_entity(entity: &str) -> Option<&'static str> {
    // match over strings are not allowed in const functions
    let s = match entity.as_bytes() {
        b"lt" => "<",
        b"gt" => ">",
        b"amp" => "&",
        b"apos" => "'",
        b"quot" => "\"",
        _ => return None,
    };
    Some(s)
}

/// Resolves all HTML5 entities. For complete list see <https://dev.w3.org/html5/html-author/charref>.
#[cfg(feature = "escape-html")]
pub const fn resolve_html5_entity(entity: &str) -> Option<&'static str> {
    // imported from https://dev.w3.org/html5/html-author/charref
    // match over strings are not allowed in const functions
    //TODO: automate up-to-dating using https://html.spec.whatwg.org/entities.json
    //TODO: building this function increases compilation time by 10+ seconds (or 5x times)
    // Maybe this is because of very long match
    // See https://github.com/tafia/quick-xml/issues/763
    let s = match entity.as_bytes() {
        b"Tab" => "\u{09}",
        b"NewLine" => "\u{0A}",
        b"excl" => "\u{21}",
        b"quot" | b"QUOT" => "\u{22}",
        b"num" => "\u{23}",
        b"dollar" => "\u{24}",
        b"percnt" => "\u{25}",
        b"amp" | b"AMP" => "\u{26}",
        b"apos" => "\u{27}",
        b"lpar" => "\u{28}",
        b"rpar" => "\u{29}",
        b"ast" | b"midast" => "\u{2A}",
        b"plus" => "\u{2B}",
        b"comma" => "\u{2C}",
        b"period" => "\u{2E}",
        b"sol" => "\u{2F}",
        b"colon" => "\u{3A}",
        b"semi" => "\u{3B}",
        b"lt" | b"LT" => "\u{3C}",
        b"equals" => "\u{3D}",
        b"gt" | b"GT" => "\u{3E}",
        b"quest" => "\u{3F}",
        b"commat" => "\u{40}",
        b"lsqb" | b"lbrack" => "\u{5B}",
        b"bsol" => "\u{5C}",
        b"rsqb" | b"rbrack" => "\u{5D}",
        b"Hat" => "\u{5E}",
        b"lowbar" => "\u{5F}",
        b"grave" | b"DiacriticalGrave" => "\u{60}",
        b"lcub" | b"lbrace" => "\u{7B}",
        b"verbar" | b"vert" | b"VerticalLine" => "\u{7C}",
        b"rcub" | b"rbrace" => "\u{7D}",
        b"nbsp" | b"NonBreakingSpace" => "\u{A0}",
        b"iexcl" => "\u{A1}",
        b"cent" => "\u{A2}",
        b"pound" => "\u{A3}",
        b"curren" => "\u{A4}",
        b"yen" => "\u{A5}",
        b"brvbar" => "\u{A6}",
        b"sect" => "\u{A7}",
        b"Dot" | b"die" | b"DoubleDot" | b"uml" => "\u{A8}",
        b"copy" | b"COPY" => "\u{A9}",
        b"ordf" => "\u{AA}",
        b"laquo" => "\u{AB}",
        b"not" => "\u{AC}",
        b"shy" => "\u{AD}",
        b"reg" | b"circledR" | b"REG" => "\u{AE}",
        b"macr" | b"OverBar" | b"strns" => "\u{AF}",
        b"deg" => "\u{B0}",
        b"plusmn" | b"pm" | b"PlusMinus" => "\u{B1}",
        b"sup2" => "\u{B2}",
        b"sup3" => "\u{B3}",
        b"acute" | b"DiacriticalAcute" => "\u{B4}",
        b"micro" => "\u{B5}",
        b"para" => "\u{B6}",
        b"middot" | b"centerdot" | b"CenterDot" => "\u{B7}",
        b"cedil" | b"Cedilla" => "\u{B8}",
        b"sup1" => "\u{B9}",
        b"ordm" => "\u{BA}",
        b"raquo" => "\u{BB}",
        b"frac14" => "\u{BC}",
        b"frac12" | b"half" => "\u{BD}",
        b"frac34" => "\u{BE}",
        b"iquest" => "\u{BF}",
        b"Agrave" => "\u{C0}",
        b"Aacute" => "\u{C1}",
        b"Acirc" => "\u{C2}",
        b"Atilde" => "\u{C3}",
        b"Auml" => "\u{C4}",
        b"Aring" => "\u{C5}",
        b"AElig" => "\u{C6}",
        b"Ccedil" => "\u{C7}",
        b"Egrave" => "\u{C8}",
        b"Eacute" => "\u{C9}",
        b"Ecirc" => "\u{CA}",
        b"Euml" => "\u{CB}",
        b"Igrave" => "\u{CC}",
        b"Iacute" => "\u{CD}",
        b"Icirc" => "\u{CE}",
        b"Iuml" => "\u{CF}",
        b"ETH" => "\u{D0}",
        b"Ntilde" => "\u{D1}",
        b"Ograve" => "\u{D2}",
        b"Oacute" => "\u{D3}",
        b"Ocirc" => "\u{D4}",
        b"Otilde" => "\u{D5}",
        b"Ouml" => "\u{D6}",
        b"times" => "\u{D7}",
        b"Oslash" => "\u{D8}",
        b"Ugrave" => "\u{D9}",
        b"Uacute" => "\u{DA}",
        b"Ucirc" => "\u{DB}",
        b"Uuml" => "\u{DC}",
        b"Yacute" => "\u{DD}",
        b"THORN" => "\u{DE}",
        b"szlig" => "\u{DF}",
        b"agrave" => "\u{E0}",
        b"aacute" => "\u{E1}",
        b"acirc" => "\u{E2}",
        b"atilde" => "\u{E3}",
        b"auml" => "\u{E4}",
        b"aring" => "\u{E5}",
        b"aelig" => "\u{E6}",
        b"ccedil" => "\u{E7}",
        b"egrave" => "\u{E8}",
        b"eacute" => "\u{E9}",
        b"ecirc" => "\u{EA}",
        b"euml" => "\u{EB}",
        b"igrave" => "\u{EC}",
        b"iacute" => "\u{ED}",
        b"icirc" => "\u{EE}",
        b"iuml" => "\u{EF}",
        b"eth" => "\u{F0}",
        b"ntilde" => "\u{F1}",
        b"ograve" => "\u{F2}",
        b"oacute" => "\u{F3}",
        b"ocirc" => "\u{F4}",
        b"otilde" => "\u{F5}",
        b"ouml" => "\u{F6}",
        b"divide" | b"div" => "\u{F7}",
        b"oslash" => "\u{F8}",
        b"ugrave" => "\u{F9}",
        b"uacute" => "\u{FA}",
        b"ucirc" => "\u{FB}",
        b"uuml" => "\u{FC}",
        b"yacute" => "\u{FD}",
        b"thorn" => "\u{FE}",
        b"yuml" => "\u{FF}",
        b"Amacr" => "\u{10}",
        b"amacr" => "\u{10}",
        b"Abreve" => "\u{10}",
        b"abreve" => "\u{10}",
        b"Aogon" => "\u{10}",
        b"aogon" => "\u{10}",
        b"Cacute" => "\u{10}",
        b"cacute" => "\u{10}",
        b"Ccirc" => "\u{10}",
        b"ccirc" => "\u{10}",
        b"Cdot" => "\u{10}",
        b"cdot" => "\u{10}",
        b"Ccaron" => "\u{10}",
        b"ccaron" => "\u{10}",
        b"Dcaron" => "\u{10}",
        b"dcaron" => "\u{10}",
        b"Dstrok" => "\u{11}",
        b"dstrok" => "\u{11}",
        b"Emacr" => "\u{11}",
        b"emacr" => "\u{11}",
        b"Edot" => "\u{11}",
        b"edot" => "\u{11}",
        b"Eogon" => "\u{11}",
        b"eogon" => "\u{11}",
        b"Ecaron" => "\u{11}",
        b"ecaron" => "\u{11}",
        b"Gcirc" => "\u{11}",
        b"gcirc" => "\u{11}",
        b"Gbreve" => "\u{11}",
        b"gbreve" => "\u{11}",
        b"Gdot" => "\u{12}",
        b"gdot" => "\u{12}",
        b"Gcedil" => "\u{12}",
        b"Hcirc" => "\u{12}",
        b"hcirc" => "\u{12}",
        b"Hstrok" => "\u{12}",
        b"hstrok" => "\u{12}",
        b"Itilde" => "\u{12}",
        b"itilde" => "\u{12}",
        b"Imacr" => "\u{12}",
        b"imacr" => "\u{12}",
        b"Iogon" => "\u{12}",
        b"iogon" => "\u{12}",
        b"Idot" => "\u{13}",
        b"imath" | b"inodot" => "\u{13}",
        b"IJlig" => "\u{13}",
        b"ijlig" => "\u{13}",
        b"Jcirc" => "\u{13}",
        b"jcirc" => "\u{13}",
        b"Kcedil" => "\u{13}",
        b"kcedil" => "\u{13}",
        b"kgreen" => "\u{13}",
        b"Lacute" => "\u{13}",
        b"lacute" => "\u{13}",
        b"Lcedil" => "\u{13}",
        b"lcedil" => "\u{13}",
        b"Lcaron" => "\u{13}",
        b"lcaron" => "\u{13}",
        b"Lmidot" => "\u{13}",
        b"lmidot" => "\u{14}",
        b"Lstrok" => "\u{14}",
        b"lstrok" => "\u{14}",
        b"Nacute" => "\u{14}",
        b"nacute" => "\u{14}",
        b"Ncedil" => "\u{14}",
        b"ncedil" => "\u{14}",
        b"Ncaron" => "\u{14}",
        b"ncaron" => "\u{14}",
        b"napos" => "\u{14}",
        b"ENG" => "\u{14}",
        b"eng" => "\u{14}",
        b"Omacr" => "\u{14}",
        b"omacr" => "\u{14}",
        b"Odblac" => "\u{15}",
        b"odblac" => "\u{15}",
        b"OElig" => "\u{15}",
        b"oelig" => "\u{15}",
        b"Racute" => "\u{15}",
        b"racute" => "\u{15}",
        b"Rcedil" => "\u{15}",
        b"rcedil" => "\u{15}",
        b"Rcaron" => "\u{15}",
        b"rcaron" => "\u{15}",
        b"Sacute" => "\u{15}",
        b"sacute" => "\u{15}",
        b"Scirc" => "\u{15}",
        b"scirc" => "\u{15}",
        b"Scedil" => "\u{15}",
        b"scedil" => "\u{15}",
        b"Scaron" => "\u{16}",
        b"scaron" => "\u{16}",
        b"Tcedil" => "\u{16}",
        b"tcedil" => "\u{16}",
        b"Tcaron" => "\u{16}",
        b"tcaron" => "\u{16}",
        b"Tstrok" => "\u{16}",
        b"tstrok" => "\u{16}",
        b"Utilde" => "\u{16}",
        b"utilde" => "\u{16}",
        b"Umacr" => "\u{16}",
        b"umacr" => "\u{16}",
        b"Ubreve" => "\u{16}",
        b"ubreve" => "\u{16}",
        b"Uring" => "\u{16}",
        b"uring" => "\u{16}",
        b"Udblac" => "\u{17}",
        b"udblac" => "\u{17}",
        b"Uogon" => "\u{17}",
        b"uogon" => "\u{17}",
        b"Wcirc" => "\u{17}",
        b"wcirc" => "\u{17}",
        b"Ycirc" => "\u{17}",
        b"ycirc" => "\u{17}",
        b"Yuml" => "\u{17}",
        b"Zacute" => "\u{17}",
        b"zacute" => "\u{17}",
        b"Zdot" => "\u{17}",
        b"zdot" => "\u{17}",
        b"Zcaron" => "\u{17}",
        b"zcaron" => "\u{17}",
        b"fnof" => "\u{19}",
        b"imped" => "\u{1B}",
        b"gacute" => "\u{1F}",
        b"jmath" => "\u{23}",
        b"circ" => "\u{2C}",
        b"caron" | b"Hacek" => "\u{2C}",
        b"breve" | b"Breve" => "\u{2D}",
        b"dot" | b"DiacriticalDot" => "\u{2D}",
        b"ring" => "\u{2D}",
        b"ogon" => "\u{2D}",
        b"tilde" | b"DiacriticalTilde" => "\u{2D}",
        b"dblac" | b"DiacriticalDoubleAcute" => "\u{2D}",
        b"DownBreve" => "\u{31}",
        b"UnderBar" => "\u{33}",
        b"Alpha" => "\u{39}",
        b"Beta" => "\u{39}",
        b"Gamma" => "\u{39}",
        b"Delta" => "\u{39}",
        b"Epsilon" => "\u{39}",
        b"Zeta" => "\u{39}",
        b"Eta" => "\u{39}",
        b"Theta" => "\u{39}",
        b"Iota" => "\u{39}",
        b"Kappa" => "\u{39}",
        b"Lambda" => "\u{39}",
        b"Mu" => "\u{39}",
        b"Nu" => "\u{39}",
        b"Xi" => "\u{39}",
        b"Omicron" => "\u{39}",
        b"Pi" => "\u{3A}",
        b"Rho" => "\u{3A}",
        b"Sigma" => "\u{3A}",
        b"Tau" => "\u{3A}",
        b"Upsilon" => "\u{3A}",
        b"Phi" => "\u{3A}",
        b"Chi" => "\u{3A}",
        b"Psi" => "\u{3A}",
        b"Omega" => "\u{3A}",
        b"alpha" => "\u{3B}",
        b"beta" => "\u{3B}",
        b"gamma" => "\u{3B}",
        b"delta" => "\u{3B}",
        b"epsiv" | b"varepsilon" | b"epsilon" => "\u{3B}",
        b"zeta" => "\u{3B}",
        b"eta" => "\u{3B}",
        b"theta" => "\u{3B}",
        b"iota" => "\u{3B}",
        b"kappa" => "\u{3B}",
        b"lambda" => "\u{3B}",
        b"mu" => "\u{3B}",
        b"nu" => "\u{3B}",
        b"xi" => "\u{3B}",
        b"omicron" => "\u{3B}",
        b"pi" => "\u{3C}",
        b"rho" => "\u{3C}",
        b"sigmav" | b"varsigma" | b"sigmaf" => "\u{3C}",
        b"sigma" => "\u{3C}",
        b"tau" => "\u{3C}",
        b"upsi" | b"upsilon" => "\u{3C}",
        b"phi" | b"phiv" | b"varphi" => "\u{3C}",
        b"chi" => "\u{3C}",
        b"psi" => "\u{3C}",
        b"omega" => "\u{3C}",
        b"thetav" | b"vartheta" | b"thetasym" => "\u{3D}",
        b"Upsi" | b"upsih" => "\u{3D}",
        b"straightphi" => "\u{3D}",
        b"piv" | b"varpi" => "\u{3D}",
        b"Gammad" => "\u{3D}",
        b"gammad" | b"digamma" => "\u{3D}",
        b"kappav" | b"varkappa" => "\u{3F}",
        b"rhov" | b"varrho" => "\u{3F}",
        b"epsi" | b"straightepsilon" => "\u{3F}",
        b"bepsi" | b"backepsilon" => "\u{3F}",
        b"IOcy" => "\u{40}",
        b"DJcy" => "\u{40}",
        b"GJcy" => "\u{40}",
        b"Jukcy" => "\u{40}",
        b"DScy" => "\u{40}",
        b"Iukcy" => "\u{40}",
        b"YIcy" => "\u{40}",
        b"Jsercy" => "\u{40}",
        b"LJcy" => "\u{40}",
        b"NJcy" => "\u{40}",
        b"TSHcy" => "\u{40}",
        b"KJcy" => "\u{40}",
        b"Ubrcy" => "\u{40}",
        b"DZcy" => "\u{40}",
        b"Acy" => "\u{41}",
        b"Bcy" => "\u{41}",
        b"Vcy" => "\u{41}",
        b"Gcy" => "\u{41}",
        b"Dcy" => "\u{41}",
        b"IEcy" => "\u{41}",
        b"ZHcy" => "\u{41}",
        b"Zcy" => "\u{41}",
        b"Icy" => "\u{41}",
        b"Jcy" => "\u{41}",
        b"Kcy" => "\u{41}",
        b"Lcy" => "\u{41}",
        b"Mcy" => "\u{41}",
        b"Ncy" => "\u{41}",
        b"Ocy" => "\u{41}",
        b"Pcy" => "\u{41}",
        b"Rcy" => "\u{42}",
        b"Scy" => "\u{42}",
        b"Tcy" => "\u{42}",
        b"Ucy" => "\u{42}",
        b"Fcy" => "\u{42}",
        b"KHcy" => "\u{42}",
        b"TScy" => "\u{42}",
        b"CHcy" => "\u{42}",
        b"SHcy" => "\u{42}",
        b"SHCHcy" => "\u{42}",
        b"HARDcy" => "\u{42}",
        b"Ycy" => "\u{42}",
        b"SOFTcy" => "\u{42}",
        b"Ecy" => "\u{42}",
        b"YUcy" => "\u{42}",
        b"YAcy" => "\u{42}",
        b"acy" => "\u{43}",
        b"bcy" => "\u{43}",
        b"vcy" => "\u{43}",
        b"gcy" => "\u{43}",
        b"dcy" => "\u{43}",
        b"iecy" => "\u{43}",
        b"zhcy" => "\u{43}",
        b"zcy" => "\u{43}",
        b"icy" => "\u{43}",
        b"jcy" => "\u{43}",
        b"kcy" => "\u{43}",
        b"lcy" => "\u{43}",
        b"mcy" => "\u{43}",
        b"ncy" => "\u{43}",
        b"ocy" => "\u{43}",
        b"pcy" => "\u{43}",
        b"rcy" => "\u{44}",
        b"scy" => "\u{44}",
        b"tcy" => "\u{44}",
        b"ucy" => "\u{44}",
        b"fcy" => "\u{44}",
        b"khcy" => "\u{44}",
        b"tscy" => "\u{44}",
        b"chcy" => "\u{44}",
        b"shcy" => "\u{44}",
        b"shchcy" => "\u{44}",
        b"hardcy" => "\u{44}",
        b"ycy" => "\u{44}",
        b"softcy" => "\u{44}",
        b"ecy" => "\u{44}",
        b"yucy" => "\u{44}",
        b"yacy" => "\u{44}",
        b"iocy" => "\u{45}",
        b"djcy" => "\u{45}",
        b"gjcy" => "\u{45}",
        b"jukcy" => "\u{45}",
        b"dscy" => "\u{45}",
        b"iukcy" => "\u{45}",
        b"yicy" => "\u{45}",
        b"jsercy" => "\u{45}",
        b"ljcy" => "\u{45}",
        b"njcy" => "\u{45}",
        b"tshcy" => "\u{45}",
        b"kjcy" => "\u{45}",
        b"ubrcy" => "\u{45}",
        b"dzcy" => "\u{45}",
        b"ensp" => "\u{2002}",
        b"emsp" => "\u{2003}",
        b"emsp13" => "\u{2004}",
        b"emsp14" => "\u{2005}",
        b"numsp" => "\u{2007}",
        b"puncsp" => "\u{2008}",
        b"thinsp" | b"ThinSpace" => "\u{2009}",
        b"hairsp" | b"VeryThinSpace" => "\u{200A}",
        b"ZeroWidthSpace"
        | b"NegativeVeryThinSpace"
        | b"NegativeThinSpace"
        | b"NegativeMediumSpace"
        | b"NegativeThickSpace" => "\u{200B}",
        b"zwnj" => "\u{200C}",
        b"zwj" => "\u{200D}",
        b"lrm" => "\u{200E}",
        b"rlm" => "\u{200F}",
        b"hyphen" | b"dash" => "\u{2010}",
        b"ndash" => "\u{2013}",
        b"mdash" => "\u{2014}",
        b"horbar" => "\u{2015}",
        b"Verbar" | b"Vert" => "\u{2016}",
        b"lsquo" | b"OpenCurlyQuote" => "\u{2018}",
        b"rsquo" | b"rsquor" | b"CloseCurlyQuote" => "\u{2019}",
        b"lsquor" | b"sbquo" => "\u{201A}",
        b"ldquo" | b"OpenCurlyDoubleQuote" => "\u{201C}",
        b"rdquo" | b"rdquor" | b"CloseCurlyDoubleQuote" => "\u{201D}",
        b"ldquor" | b"bdquo" => "\u{201E}",
        b"dagger" => "\u{2020}",
        b"Dagger" | b"ddagger" => "\u{2021}",
        b"bull" | b"bullet" => "\u{2022}",
        b"nldr" => "\u{2025}",
        b"hellip" | b"mldr" => "\u{2026}",
        b"permil" => "\u{2030}",
        b"pertenk" => "\u{2031}",
        b"prime" => "\u{2032}",
        b"Prime" => "\u{2033}",
        b"tprime" => "\u{2034}",
        b"bprime" | b"backprime" => "\u{2035}",
        b"lsaquo" => "\u{2039}",
        b"rsaquo" => "\u{203A}",
        b"oline" => "\u{203E}",
        b"caret" => "\u{2041}",
        b"hybull" => "\u{2043}",
        b"frasl" => "\u{2044}",
        b"bsemi" => "\u{204F}",
        b"qprime" => "\u{2057}",
        b"MediumSpace" => "\u{205F}",
        b"NoBreak" => "\u{2060}",
        b"ApplyFunction" | b"af" => "\u{2061}",
        b"InvisibleTimes" | b"it" => "\u{2062}",
        b"InvisibleComma" | b"ic" => "\u{2063}",
        b"euro" => "\u{20AC}",
        b"tdot" | b"TripleDot" => "\u{20DB}",
        b"DotDot" => "\u{20DC}",
        b"Copf" | b"complexes" => "\u{2102}",
        b"incare" => "\u{2105}",
        b"gscr" => "\u{210A}",
        b"hamilt" | b"HilbertSpace" | b"Hscr" => "\u{210B}",
        b"Hfr" | b"Poincareplane" => "\u{210C}",
        b"quaternions" | b"Hopf" => "\u{210D}",
        b"planckh" => "\u{210E}",
        b"planck" | b"hbar" | b"plankv" | b"hslash" => "\u{210F}",
        b"Iscr" | b"imagline" => "\u{2110}",
        b"image" | b"Im" | b"imagpart" | b"Ifr" => "\u{2111}",
        b"Lscr" | b"lagran" | b"Laplacetrf" => "\u{2112}",
        b"ell" => "\u{2113}",
        b"Nopf" | b"naturals" => "\u{2115}",
        b"numero" => "\u{2116}",
        b"copysr" => "\u{2117}",
        b"weierp" | b"wp" => "\u{2118}",
        b"Popf" | b"primes" => "\u{2119}",
        b"rationals" | b"Qopf" => "\u{211A}",
        b"Rscr" | b"realine" => "\u{211B}",
        b"real" | b"Re" | b"realpart" | b"Rfr" => "\u{211C}",
        b"reals" | b"Ropf" => "\u{211D}",
        b"rx" => "\u{211E}",
        b"trade" | b"TRADE" => "\u{2122}",
        b"integers" | b"Zopf" => "\u{2124}",
        b"ohm" => "\u{2126}",
        b"mho" => "\u{2127}",
        b"Zfr" | b"zeetrf" => "\u{2128}",
        b"iiota" => "\u{2129}",
        b"angst" => "\u{212B}",
        b"bernou" | b"Bernoullis" | b"Bscr" => "\u{212C}",
        b"Cfr" | b"Cayleys" => "\u{212D}",
        b"escr" => "\u{212F}",
        b"Escr" | b"expectation" => "\u{2130}",
        b"Fscr" | b"Fouriertrf" => "\u{2131}",
        b"phmmat" | b"Mellintrf" | b"Mscr" => "\u{2133}",
        b"order" | b"orderof" | b"oscr" => "\u{2134}",
        b"alefsym" | b"aleph" => "\u{2135}",
        b"beth" => "\u{2136}",
        b"gimel" => "\u{2137}",
        b"daleth" => "\u{2138}",
        b"CapitalDifferentialD" | b"DD" => "\u{2145}",
        b"DifferentialD" | b"dd" => "\u{2146}",
        b"ExponentialE" | b"exponentiale" | b"ee" => "\u{2147}",
        b"ImaginaryI" | b"ii" => "\u{2148}",
        b"frac13" => "\u{2153}",
        b"frac23" => "\u{2154}",
        b"frac15" => "\u{2155}",
        b"frac25" => "\u{2156}",
        b"frac35" => "\u{2157}",
        b"frac45" => "\u{2158}",
        b"frac16" => "\u{2159}",
        b"frac56" => "\u{215A}",
        b"frac18" => "\u{215B}",
        b"frac38" => "\u{215C}",
        b"frac58" => "\u{215D}",
        b"frac78" => "\u{215E}",
        b"larr" | b"leftarrow" | b"LeftArrow" | b"slarr" | b"ShortLeftArrow" => "\u{2190}",
        b"uarr" | b"uparrow" | b"UpArrow" | b"ShortUpArrow" => "\u{2191}",
        b"rarr" | b"rightarrow" | b"RightArrow" | b"srarr" | b"ShortRightArrow" => "\u{2192}",
        b"darr" | b"downarrow" | b"DownArrow" | b"ShortDownArrow" => "\u{2193}",
        b"harr" | b"leftrightarrow" | b"LeftRightArrow" => "\u{2194}",
        b"varr" | b"updownarrow" | b"UpDownArrow" => "\u{2195}",
        b"nwarr" | b"UpperLeftArrow" | b"nwarrow" => "\u{2196}",
        b"nearr" | b"UpperRightArrow" | b"nearrow" => "\u{2197}",
        b"searr" | b"searrow" | b"LowerRightArrow" => "\u{2198}",
        b"swarr" | b"swarrow" | b"LowerLeftArrow" => "\u{2199}",
        b"nlarr" | b"nleftarrow" => "\u{219A}",
        b"nrarr" | b"nrightarrow" => "\u{219B}",
        b"rarrw" | b"rightsquigarrow" => "\u{219D}",
        b"Larr" | b"twoheadleftarrow" => "\u{219E}",
        b"Uarr" => "\u{219F}",
        b"Rarr" | b"twoheadrightarrow" => "\u{21A0}",
        b"Darr" => "\u{21A1}",
        b"larrtl" | b"leftarrowtail" => "\u{21A2}",
        b"rarrtl" | b"rightarrowtail" => "\u{21A3}",
        b"LeftTeeArrow" | b"mapstoleft" => "\u{21A4}",
        b"UpTeeArrow" | b"mapstoup" => "\u{21A5}",
        b"map" | b"RightTeeArrow" | b"mapsto" => "\u{21A6}",
        b"DownTeeArrow" | b"mapstodown" => "\u{21A7}",
        b"larrhk" | b"hookleftarrow" => "\u{21A9}",
        b"rarrhk" | b"hookrightarrow" => "\u{21AA}",
        b"larrlp" | b"looparrowleft" => "\u{21AB}",
        b"rarrlp" | b"looparrowright" => "\u{21AC}",
        b"harrw" | b"leftrightsquigarrow" => "\u{21AD}",
        b"nharr" | b"nleftrightarrow" => "\u{21AE}",
        b"lsh" | b"Lsh" => "\u{21B0}",
        b"rsh" | b"Rsh" => "\u{21B1}",
        b"ldsh" => "\u{21B2}",
        b"rdsh" => "\u{21B3}",
        b"crarr" => "\u{21B5}",
        b"cularr" | b"curvearrowleft" => "\u{21B6}",
        b"curarr" | b"curvearrowright" => "\u{21B7}",
        b"olarr" | b"circlearrowleft" => "\u{21BA}",
        b"orarr" | b"circlearrowright" => "\u{21BB}",
        b"lharu" | b"LeftVector" | b"leftharpoonup" => "\u{21BC}",
        b"lhard" | b"leftharpoondown" | b"DownLeftVector" => "\u{21BD}",
        b"uharr" | b"upharpoonright" | b"RightUpVector" => "\u{21BE}",
        b"uharl" | b"upharpoonleft" | b"LeftUpVector" => "\u{21BF}",
        b"rharu" | b"RightVector" | b"rightharpoonup" => "\u{21C0}",
        b"rhard" | b"rightharpoondown" | b"DownRightVector" => "\u{21C1}",
        b"dharr" | b"RightDownVector" | b"downharpoonright" => "\u{21C2}",
        b"dharl" | b"LeftDownVector" | b"downharpoonleft" => "\u{21C3}",
        b"rlarr" | b"rightleftarrows" | b"RightArrowLeftArrow" => "\u{21C4}",
        b"udarr" | b"UpArrowDownArrow" => "\u{21C5}",
        b"lrarr" | b"leftrightarrows" | b"LeftArrowRightArrow" => "\u{21C6}",
        b"llarr" | b"leftleftarrows" => "\u{21C7}",
        b"uuarr" | b"upuparrows" => "\u{21C8}",
        b"rrarr" | b"rightrightarrows" => "\u{21C9}",
        b"ddarr" | b"downdownarrows" => "\u{21CA}",
        b"lrhar" | b"ReverseEquilibrium" | b"leftrightharpoons" => "\u{21CB}",
        b"rlhar" | b"rightleftharpoons" | b"Equilibrium" => "\u{21CC}",
        b"nlArr" | b"nLeftarrow" => "\u{21CD}",
        b"nhArr" | b"nLeftrightarrow" => "\u{21CE}",
        b"nrArr" | b"nRightarrow" => "\u{21CF}",
        b"lArr" | b"Leftarrow" | b"DoubleLeftArrow" => "\u{21D0}",
        b"uArr" | b"Uparrow" | b"DoubleUpArrow" => "\u{21D1}",
        b"rArr" | b"Rightarrow" | b"Implies" | b"DoubleRightArrow" => "\u{21D2}",
        b"dArr" | b"Downarrow" | b"DoubleDownArrow" => "\u{21D3}",
        b"hArr" | b"Leftrightarrow" | b"DoubleLeftRightArrow" | b"iff" => "\u{21D4}",
        b"vArr" | b"Updownarrow" | b"DoubleUpDownArrow" => "\u{21D5}",
        b"nwArr" => "\u{21D6}",
        b"neArr" => "\u{21D7}",
        b"seArr" => "\u{21D8}",
        b"swArr" => "\u{21D9}",
        b"lAarr" | b"Lleftarrow" => "\u{21DA}",
        b"rAarr" | b"Rrightarrow" => "\u{21DB}",
        b"zigrarr" => "\u{21DD}",
        b"larrb" | b"LeftArrowBar" => "\u{21E4}",
        b"rarrb" | b"RightArrowBar" => "\u{21E5}",
        b"duarr" | b"DownArrowUpArrow" => "\u{21F5}",
        b"loarr" => "\u{21FD}",
        b"roarr" => "\u{21FE}",
        b"hoarr" => "\u{21FF}",
        b"forall" | b"ForAll" => "\u{2200}",
        b"comp" | b"complement" => "\u{2201}",
        b"part" | b"PartialD" => "\u{2202}",
        b"exist" | b"Exists" => "\u{2203}",
        b"nexist" | b"NotExists" | b"nexists" => "\u{2204}",
        b"empty" | b"emptyset" | b"emptyv" | b"varnothing" => "\u{2205}",
        b"nabla" | b"Del" => "\u{2207}",
        b"isin" | b"isinv" | b"Element" | b"in" => "\u{2208}",
        b"notin" | b"NotElement" | b"notinva" => "\u{2209}",
        b"niv" | b"ReverseElement" | b"ni" | b"SuchThat" => "\u{220B}",
        b"notni" | b"notniva" | b"NotReverseElement" => "\u{220C}",
        b"prod" | b"Product" => "\u{220F}",
        b"coprod" | b"Coproduct" => "\u{2210}",
        b"sum" | b"Sum" => "\u{2211}",
        b"minus" => "\u{2212}",
        b"mnplus" | b"mp" | b"MinusPlus" => "\u{2213}",
        b"plusdo" | b"dotplus" => "\u{2214}",
        b"setmn" | b"setminus" | b"Backslash" | b"ssetmn" | b"smallsetminus" => "\u{2216}",
        b"lowast" => "\u{2217}",
        b"compfn" | b"SmallCircle" => "\u{2218}",
        b"radic" | b"Sqrt" => "\u{221A}",
        b"prop" | b"propto" | b"Proportional" | b"vprop" | b"varpropto" => "\u{221D}",
        b"infin" => "\u{221E}",
        b"angrt" => "\u{221F}",
        b"ang" | b"angle" => "\u{2220}",
        b"angmsd" | b"measuredangle" => "\u{2221}",
        b"angsph" => "\u{2222}",
        b"mid" | b"VerticalBar" | b"smid" | b"shortmid" => "\u{2223}",
        b"nmid" | b"NotVerticalBar" | b"nsmid" | b"nshortmid" => "\u{2224}",
        b"par" | b"parallel" | b"DoubleVerticalBar" | b"spar" | b"shortparallel" => "\u{2225}",
        b"npar" | b"nparallel" | b"NotDoubleVerticalBar" | b"nspar" | b"nshortparallel" => {
            "\u{2226}"
        }
        b"and" | b"wedge" => "\u{2227}",
        b"or" | b"vee" => "\u{2228}",
        b"cap" => "\u{2229}",
        b"cup" => "\u{222A}",
        b"int" | b"Integral" => "\u{222B}",
        b"Int" => "\u{222C}",
        b"tint" | b"iiint" => "\u{222D}",
        b"conint" | b"oint" | b"ContourIntegral" => "\u{222E}",
        b"Conint" | b"DoubleContourIntegral" => "\u{222F}",
        b"Cconint" => "\u{2230}",
        b"cwint" => "\u{2231}",
        b"cwconint" | b"ClockwiseContourIntegral" => "\u{2232}",
        b"awconint" | b"CounterClockwiseContourIntegral" => "\u{2233}",
        b"there4" | b"therefore" | b"Therefore" => "\u{2234}",
        b"becaus" | b"because" | b"Because" => "\u{2235}",
        b"ratio" => "\u{2236}",
        b"Colon" | b"Proportion" => "\u{2237}",
        b"minusd" | b"dotminus" => "\u{2238}",
        b"mDDot" => "\u{223A}",
        b"homtht" => "\u{223B}",
        b"sim" | b"Tilde" | b"thksim" | b"thicksim" => "\u{223C}",
        b"bsim" | b"backsim" => "\u{223D}",
        b"ac" | b"mstpos" => "\u{223E}",
        b"acd" => "\u{223F}",
        b"wreath" | b"VerticalTilde" | b"wr" => "\u{2240}",
        b"nsim" | b"NotTilde" => "\u{2241}",
        b"esim" | b"EqualTilde" | b"eqsim" => "\u{2242}",
        b"sime" | b"TildeEqual" | b"simeq" => "\u{2243}",
        b"nsime" | b"nsimeq" | b"NotTildeEqual" => "\u{2244}",
        b"cong" | b"TildeFullEqual" => "\u{2245}",
        b"simne" => "\u{2246}",
        b"ncong" | b"NotTildeFullEqual" => "\u{2247}",
        b"asymp" | b"ap" | b"TildeTilde" | b"approx" | b"thkap" | b"thickapprox" => "\u{2248}",
        b"nap" | b"NotTildeTilde" | b"napprox" => "\u{2249}",
        b"ape" | b"approxeq" => "\u{224A}",
        b"apid" => "\u{224B}",
        b"bcong" | b"backcong" => "\u{224C}",
        b"asympeq" | b"CupCap" => "\u{224D}",
        b"bump" | b"HumpDownHump" | b"Bumpeq" => "\u{224E}",
        b"bumpe" | b"HumpEqual" | b"bumpeq" => "\u{224F}",
        b"esdot" | b"DotEqual" | b"doteq" => "\u{2250}",
        b"eDot" | b"doteqdot" => "\u{2251}",
        b"efDot" | b"fallingdotseq" => "\u{2252}",
        b"erDot" | b"risingdotseq" => "\u{2253}",
        b"colone" | b"coloneq" | b"Assign" => "\u{2254}",
        b"ecolon" | b"eqcolon" => "\u{2255}",
        b"ecir" | b"eqcirc" => "\u{2256}",
        b"cire" | b"circeq" => "\u{2257}",
        b"wedgeq" => "\u{2259}",
        b"veeeq" => "\u{225A}",
        b"trie" | b"triangleq" => "\u{225C}",
        b"equest" | b"questeq" => "\u{225F}",
        b"ne" | b"NotEqual" => "\u{2260}",
        b"equiv" | b"Congruent" => "\u{2261}",
        b"nequiv" | b"NotCongruent" => "\u{2262}",
        b"le" | b"leq" => "\u{2264}",
        b"ge" | b"GreaterEqual" | b"geq" => "\u{2265}",
        b"lE" | b"LessFullEqual" | b"leqq" => "\u{2266}",
        b"gE" | b"GreaterFullEqual" | b"geqq" => "\u{2267}",
        b"lnE" | b"lneqq" => "\u{2268}",
        b"gnE" | b"gneqq" => "\u{2269}",
        b"Lt" | b"NestedLessLess" | b"ll" => "\u{226A}",
        b"Gt" | b"NestedGreaterGreater" | b"gg" => "\u{226B}",
        b"twixt" | b"between" => "\u{226C}",
        b"NotCupCap" => "\u{226D}",
        b"nlt" | b"NotLess" | b"nless" => "\u{226E}",
        b"ngt" | b"NotGreater" | b"ngtr" => "\u{226F}",
        b"nle" | b"NotLessEqual" | b"nleq" => "\u{2270}",
        b"nge" | b"NotGreaterEqual" | b"ngeq" => "\u{2271}",
        b"lsim" | b"LessTilde" | b"lesssim" => "\u{2272}",
        b"gsim" | b"gtrsim" | b"GreaterTilde" => "\u{2273}",
        b"nlsim" | b"NotLessTilde" => "\u{2274}",
        b"ngsim" | b"NotGreaterTilde" => "\u{2275}",
        b"lg" | b"lessgtr" | b"LessGreater" => "\u{2276}",
        b"gl" | b"gtrless" | b"GreaterLess" => "\u{2277}",
        b"ntlg" | b"NotLessGreater" => "\u{2278}",
        b"ntgl" | b"NotGreaterLess" => "\u{2279}",
        b"pr" | b"Precedes" | b"prec" => "\u{227A}",
        b"sc" | b"Succeeds" | b"succ" => "\u{227B}",
        b"prcue" | b"PrecedesSlantEqual" | b"preccurlyeq" => "\u{227C}",
        b"sccue" | b"SucceedsSlantEqual" | b"succcurlyeq" => "\u{227D}",
        b"prsim" | b"precsim" | b"PrecedesTilde" => "\u{227E}",
        b"scsim" | b"succsim" | b"SucceedsTilde" => "\u{227F}",
        b"npr" | b"nprec" | b"NotPrecedes" => "\u{2280}",
        b"nsc" | b"nsucc" | b"NotSucceeds" => "\u{2281}",
        b"sub" | b"subset" => "\u{2282}",
        b"sup" | b"supset" | b"Superset" => "\u{2283}",
        b"nsub" => "\u{2284}",
        b"nsup" => "\u{2285}",
        b"sube" | b"SubsetEqual" | b"subseteq" => "\u{2286}",
        b"supe" | b"supseteq" | b"SupersetEqual" => "\u{2287}",
        b"nsube" | b"nsubseteq" | b"NotSubsetEqual" => "\u{2288}",
        b"nsupe" | b"nsupseteq" | b"NotSupersetEqual" => "\u{2289}",
        b"subne" | b"subsetneq" => "\u{228A}",
        b"supne" | b"supsetneq" => "\u{228B}",
        b"cupdot" => "\u{228D}",
        b"uplus" | b"UnionPlus" => "\u{228E}",
        b"sqsub" | b"SquareSubset" | b"sqsubset" => "\u{228F}",
        b"sqsup" | b"SquareSuperset" | b"sqsupset" => "\u{2290}",
        b"sqsube" | b"SquareSubsetEqual" | b"sqsubseteq" => "\u{2291}",
        b"sqsupe" | b"SquareSupersetEqual" | b"sqsupseteq" => "\u{2292}",
        b"sqcap" | b"SquareIntersection" => "\u{2293}",
        b"sqcup" | b"SquareUnion" => "\u{2294}",
        b"oplus" | b"CirclePlus" => "\u{2295}",
        b"ominus" | b"CircleMinus" => "\u{2296}",
        b"otimes" | b"CircleTimes" => "\u{2297}",
        b"osol" => "\u{2298}",
        b"odot" | b"CircleDot" => "\u{2299}",
        b"ocir" | b"circledcirc" => "\u{229A}",
        b"oast" | b"circledast" => "\u{229B}",
        b"odash" | b"circleddash" => "\u{229D}",
        b"plusb" | b"boxplus" => "\u{229E}",
        b"minusb" | b"boxminus" => "\u{229F}",
        b"timesb" | b"boxtimes" => "\u{22A0}",
        b"sdotb" | b"dotsquare" => "\u{22A1}",
        b"vdash" | b"RightTee" => "\u{22A2}",
        b"dashv" | b"LeftTee" => "\u{22A3}",
        b"top" | b"DownTee" => "\u{22A4}",
        b"bottom" | b"bot" | b"perp" | b"UpTee" => "\u{22A5}",
        b"models" => "\u{22A7}",
        b"vDash" | b"DoubleRightTee" => "\u{22A8}",
        b"Vdash" => "\u{22A9}",
        b"Vvdash" => "\u{22AA}",
        b"VDash" => "\u{22AB}",
        b"nvdash" => "\u{22AC}",
        b"nvDash" => "\u{22AD}",
        b"nVdash" => "\u{22AE}",
        b"nVDash" => "\u{22AF}",
        b"prurel" => "\u{22B0}",
        b"vltri" | b"vartriangleleft" | b"LeftTriangle" => "\u{22B2}",
        b"vrtri" | b"vartriangleright" | b"RightTriangle" => "\u{22B3}",
        b"ltrie" | b"trianglelefteq" | b"LeftTriangleEqual" => "\u{22B4}",
        b"rtrie" | b"trianglerighteq" | b"RightTriangleEqual" => "\u{22B5}",
        b"origof" => "\u{22B6}",
        b"imof" => "\u{22B7}",
        b"mumap" | b"multimap" => "\u{22B8}",
        b"hercon" => "\u{22B9}",
        b"intcal" | b"intercal" => "\u{22BA}",
        b"veebar" => "\u{22BB}",
        b"barvee" => "\u{22BD}",
        b"angrtvb" => "\u{22BE}",
        b"lrtri" => "\u{22BF}",
        b"xwedge" | b"Wedge" | b"bigwedge" => "\u{22C0}",
        b"xvee" | b"Vee" | b"bigvee" => "\u{22C1}",
        b"xcap" | b"Intersection" | b"bigcap" => "\u{22C2}",
        b"xcup" | b"Union" | b"bigcup" => "\u{22C3}",
        b"diam" | b"diamond" | b"Diamond" => "\u{22C4}",
        b"sdot" => "\u{22C5}",
        b"sstarf" | b"Star" => "\u{22C6}",
        b"divonx" | b"divideontimes" => "\u{22C7}",
        b"bowtie" => "\u{22C8}",
        b"ltimes" => "\u{22C9}",
        b"rtimes" => "\u{22CA}",
        b"lthree" | b"leftthreetimes" => "\u{22CB}",
        b"rthree" | b"rightthreetimes" => "\u{22CC}",
        b"bsime" | b"backsimeq" => "\u{22CD}",
        b"cuvee" | b"curlyvee" => "\u{22CE}",
        b"cuwed" | b"curlywedge" => "\u{22CF}",
        b"Sub" | b"Subset" => "\u{22D0}",
        b"Sup" | b"Supset" => "\u{22D1}",
        b"Cap" => "\u{22D2}",
        b"Cup" => "\u{22D3}",
        b"fork" | b"pitchfork" => "\u{22D4}",
        b"epar" => "\u{22D5}",
        b"ltdot" | b"lessdot" => "\u{22D6}",
        b"gtdot" | b"gtrdot" => "\u{22D7}",
        b"Ll" => "\u{22D8}",
        b"Gg" | b"ggg" => "\u{22D9}",
        b"leg" | b"LessEqualGreater" | b"lesseqgtr" => "\u{22DA}",
        b"gel" | b"gtreqless" | b"GreaterEqualLess" => "\u{22DB}",
        b"cuepr" | b"curlyeqprec" => "\u{22DE}",
        b"cuesc" | b"curlyeqsucc" => "\u{22DF}",
        b"nprcue" | b"NotPrecedesSlantEqual" => "\u{22E0}",
        b"nsccue" | b"NotSucceedsSlantEqual" => "\u{22E1}",
        b"nsqsube" | b"NotSquareSubsetEqual" => "\u{22E2}",
        b"nsqsupe" | b"NotSquareSupersetEqual" => "\u{22E3}",
        b"lnsim" => "\u{22E6}",
        b"gnsim" => "\u{22E7}",
        b"prnsim" | b"precnsim" => "\u{22E8}",
        b"scnsim" | b"succnsim" => "\u{22E9}",
        b"nltri" | b"ntriangleleft" | b"NotLeftTriangle" => "\u{22EA}",
        b"nrtri" | b"ntriangleright" | b"NotRightTriangle" => "\u{22EB}",
        b"nltrie" | b"ntrianglelefteq" | b"NotLeftTriangleEqual" => "\u{22EC}",
        b"nrtrie" | b"ntrianglerighteq" | b"NotRightTriangleEqual" => "\u{22ED}",
        b"vellip" => "\u{22EE}",
        b"ctdot" => "\u{22EF}",
        b"utdot" => "\u{22F0}",
        b"dtdot" => "\u{22F1}",
        b"disin" => "\u{22F2}",
        b"isinsv" => "\u{22F3}",
        b"isins" => "\u{22F4}",
        b"isindot" => "\u{22F5}",
        b"notinvc" => "\u{22F6}",
        b"notinvb" => "\u{22F7}",
        b"isinE" => "\u{22F9}",
        b"nisd" => "\u{22FA}",
        b"xnis" => "\u{22FB}",
        b"nis" => "\u{22FC}",
        b"notnivc" => "\u{22FD}",
        b"notnivb" => "\u{22FE}",
        b"barwed" | b"barwedge" => "\u{2305}",
        b"Barwed" | b"doublebarwedge" => "\u{2306}",
        b"lceil" | b"LeftCeiling" => "\u{2308}",
        b"rceil" | b"RightCeiling" => "\u{2309}",
        b"lfloor" | b"LeftFloor" => "\u{230A}",
        b"rfloor" | b"RightFloor" => "\u{230B}",
        b"drcrop" => "\u{230C}",
        b"dlcrop" => "\u{230D}",
        b"urcrop" => "\u{230E}",
        b"ulcrop" => "\u{230F}",
        b"bnot" => "\u{2310}",
        b"profline" => "\u{2312}",
        b"profsurf" => "\u{2313}",
        b"telrec" => "\u{2315}",
        b"target" => "\u{2316}",
        b"ulcorn" | b"ulcorner" => "\u{231C}",
        b"urcorn" | b"urcorner" => "\u{231D}",
        b"dlcorn" | b"llcorner" => "\u{231E}",
        b"drcorn" | b"lrcorner" => "\u{231F}",
        b"frown" | b"sfrown" => "\u{2322}",
        b"smile" | b"ssmile" => "\u{2323}",
        b"cylcty" => "\u{232D}",
        b"profalar" => "\u{232E}",
        b"topbot" => "\u{2336}",
        b"ovbar" => "\u{233D}",
        b"solbar" => "\u{233F}",
        b"angzarr" => "\u{237C}",
        b"lmoust" | b"lmoustache" => "\u{23B0}",
        b"rmoust" | b"rmoustache" => "\u{23B1}",
        b"tbrk" | b"OverBracket" => "\u{23B4}",
        b"bbrk" | b"UnderBracket" => "\u{23B5}",
        b"bbrktbrk" => "\u{23B6}",
        b"OverParenthesis" => "\u{23DC}",
        b"UnderParenthesis" => "\u{23DD}",
        b"OverBrace" => "\u{23DE}",
        b"UnderBrace" => "\u{23DF}",
        b"trpezium" => "\u{23E2}",
        b"elinters" => "\u{23E7}",
        b"blank" => "\u{2423}",
        b"oS" | b"circledS" => "\u{24C8}",
        b"boxh" | b"HorizontalLine" => "\u{2500}",
        b"boxv" => "\u{2502}",
        b"boxdr" => "\u{250C}",
        b"boxdl" => "\u{2510}",
        b"boxur" => "\u{2514}",
        b"boxul" => "\u{2518}",
        b"boxvr" => "\u{251C}",
        b"boxvl" => "\u{2524}",
        b"boxhd" => "\u{252C}",
        b"boxhu" => "\u{2534}",
        b"boxvh" => "\u{253C}",
        b"boxH" => "\u{2550}",
        b"boxV" => "\u{2551}",
        b"boxdR" => "\u{2552}",
        b"boxDr" => "\u{2553}",
        b"boxDR" => "\u{2554}",
        b"boxdL" => "\u{2555}",
        b"boxDl" => "\u{2556}",
        b"boxDL" => "\u{2557}",
        b"boxuR" => "\u{2558}",
        b"boxUr" => "\u{2559}",
        b"boxUR" => "\u{255A}",
        b"boxuL" => "\u{255B}",
        b"boxUl" => "\u{255C}",
        b"boxUL" => "\u{255D}",
        b"boxvR" => "\u{255E}",
        b"boxVr" => "\u{255F}",
        b"boxVR" => "\u{2560}",
        b"boxvL" => "\u{2561}",
        b"boxVl" => "\u{2562}",
        b"boxVL" => "\u{2563}",
        b"boxHd" => "\u{2564}",
        b"boxhD" => "\u{2565}",
        b"boxHD" => "\u{2566}",
        b"boxHu" => "\u{2567}",
        b"boxhU" => "\u{2568}",
        b"boxHU" => "\u{2569}",
        b"boxvH" => "\u{256A}",
        b"boxVh" => "\u{256B}",
        b"boxVH" => "\u{256C}",
        b"uhblk" => "\u{2580}",
        b"lhblk" => "\u{2584}",
        b"block" => "\u{2588}",
        b"blk14" => "\u{2591}",
        b"blk12" => "\u{2592}",
        b"blk34" => "\u{2593}",
        b"squ" | b"square" | b"Square" => "\u{25A1}",
        b"squf" | b"squarf" | b"blacksquare" | b"FilledVerySmallSquare" => "\u{25AA}",
        b"EmptyVerySmallSquare" => "\u{25AB}",
        b"rect" => "\u{25AD}",
        b"marker" => "\u{25AE}",
        b"fltns" => "\u{25B1}",
        b"xutri" | b"bigtriangleup" => "\u{25B3}",
        b"utrif" | b"blacktriangle" => "\u{25B4}",
        b"utri" | b"triangle" => "\u{25B5}",
        b"rtrif" | b"blacktriangleright" => "\u{25B8}",
        b"rtri" | b"triangleright" => "\u{25B9}",
        b"xdtri" | b"bigtriangledown" => "\u{25BD}",
        b"dtrif" | b"blacktriangledown" => "\u{25BE}",
        b"dtri" | b"triangledown" => "\u{25BF}",
        b"ltrif" | b"blacktriangleleft" => "\u{25C2}",
        b"ltri" | b"triangleleft" => "\u{25C3}",
        b"loz" | b"lozenge" => "\u{25CA}",
        b"cir" => "\u{25CB}",
        b"tridot" => "\u{25EC}",
        b"xcirc" | b"bigcirc" => "\u{25EF}",
        b"ultri" => "\u{25F8}",
        b"urtri" => "\u{25F9}",
        b"lltri" => "\u{25FA}",
        b"EmptySmallSquare" => "\u{25FB}",
        b"FilledSmallSquare" => "\u{25FC}",
        b"starf" | b"bigstar" => "\u{2605}",
        b"star" => "\u{2606}",
        b"phone" => "\u{260E}",
        b"female" => "\u{2640}",
        b"male" => "\u{2642}",
        b"spades" | b"spadesuit" => "\u{2660}",
        b"clubs" | b"clubsuit" => "\u{2663}",
        b"hearts" | b"heartsuit" => "\u{2665}",
        b"diams" | b"diamondsuit" => "\u{2666}",
        b"sung" => "\u{266A}",
        b"flat" => "\u{266D}",
        b"natur" | b"natural" => "\u{266E}",
        b"sharp" => "\u{266F}",
        b"check" | b"checkmark" => "\u{2713}",
        b"cross" => "\u{2717}",
        b"malt" | b"maltese" => "\u{2720}",
        b"sext" => "\u{2736}",
        b"VerticalSeparator" => "\u{2758}",
        b"lbbrk" => "\u{2772}",
        b"rbbrk" => "\u{2773}",
        b"lobrk" | b"LeftDoubleBracket" => "\u{27E6}",
        b"robrk" | b"RightDoubleBracket" => "\u{27E7}",
        b"lang" | b"LeftAngleBracket" | b"langle" => "\u{27E8}",
        b"rang" | b"RightAngleBracket" | b"rangle" => "\u{27E9}",
        b"Lang" => "\u{27EA}",
        b"Rang" => "\u{27EB}",
        b"loang" => "\u{27EC}",
        b"roang" => "\u{27ED}",
        b"xlarr" | b"longleftarrow" | b"LongLeftArrow" => "\u{27F5}",
        b"xrarr" | b"longrightarrow" | b"LongRightArrow" => "\u{27F6}",
        b"xharr" | b"longleftrightarrow" | b"LongLeftRightArrow" => "\u{27F7}",
        b"xlArr" | b"Longleftarrow" | b"DoubleLongLeftArrow" => "\u{27F8}",
        b"xrArr" | b"Longrightarrow" | b"DoubleLongRightArrow" => "\u{27F9}",
        b"xhArr" | b"Longleftrightarrow" | b"DoubleLongLeftRightArrow" => "\u{27FA}",
        b"xmap" | b"longmapsto" => "\u{27FC}",
        b"dzigrarr" => "\u{27FF}",
        b"nvlArr" => "\u{2902}",
        b"nvrArr" => "\u{2903}",
        b"nvHarr" => "\u{2904}",
        b"Map" => "\u{2905}",
        b"lbarr" => "\u{290C}",
        b"rbarr" | b"bkarow" => "\u{290D}",
        b"lBarr" => "\u{290E}",
        b"rBarr" | b"dbkarow" => "\u{290F}",
        b"RBarr" | b"drbkarow" => "\u{2910}",
        b"DDotrahd" => "\u{2911}",
        b"UpArrowBar" => "\u{2912}",
        b"DownArrowBar" => "\u{2913}",
        b"Rarrtl" => "\u{2916}",
        b"latail" => "\u{2919}",
        b"ratail" => "\u{291A}",
        b"lAtail" => "\u{291B}",
        b"rAtail" => "\u{291C}",
        b"larrfs" => "\u{291D}",
        b"rarrfs" => "\u{291E}",
        b"larrbfs" => "\u{291F}",
        b"rarrbfs" => "\u{2920}",
        b"nwarhk" => "\u{2923}",
        b"nearhk" => "\u{2924}",
        b"searhk" | b"hksearow" => "\u{2925}",
        b"swarhk" | b"hkswarow" => "\u{2926}",
        b"nwnear" => "\u{2927}",
        b"nesear" | b"toea" => "\u{2928}",
        b"seswar" | b"tosa" => "\u{2929}",
        b"swnwar" => "\u{292A}",
        b"rarrc" => "\u{2933}",
        b"cudarrr" => "\u{2935}",
        b"ldca" => "\u{2936}",
        b"rdca" => "\u{2937}",
        b"cudarrl" => "\u{2938}",
        b"larrpl" => "\u{2939}",
        b"curarrm" => "\u{293C}",
        b"cularrp" => "\u{293D}",
        b"rarrpl" => "\u{2945}",
        b"harrcir" => "\u{2948}",
        b"Uarrocir" => "\u{2949}",
        b"lurdshar" => "\u{294A}",
        b"ldrushar" => "\u{294B}",
        b"LeftRightVector" => "\u{294E}",
        b"RightUpDownVector" => "\u{294F}",
        b"DownLeftRightVector" => "\u{2950}",
        b"LeftUpDownVector" => "\u{2951}",
        b"LeftVectorBar" => "\u{2952}",
        b"RightVectorBar" => "\u{2953}",
        b"RightUpVectorBar" => "\u{2954}",
        b"RightDownVectorBar" => "\u{2955}",
        b"DownLeftVectorBar" => "\u{2956}",
        b"DownRightVectorBar" => "\u{2957}",
        b"LeftUpVectorBar" => "\u{2958}",
        b"LeftDownVectorBar" => "\u{2959}",
        b"LeftTeeVector" => "\u{295A}",
        b"RightTeeVector" => "\u{295B}",
        b"RightUpTeeVector" => "\u{295C}",
        b"RightDownTeeVector" => "\u{295D}",
        b"DownLeftTeeVector" => "\u{295E}",
        b"DownRightTeeVector" => "\u{295F}",
        b"LeftUpTeeVector" => "\u{2960}",
        b"LeftDownTeeVector" => "\u{2961}",
        b"lHar" => "\u{2962}",
        b"uHar" => "\u{2963}",
        b"rHar" => "\u{2964}",
        b"dHar" => "\u{2965}",
        b"luruhar" => "\u{2966}",
        b"ldrdhar" => "\u{2967}",
        b"ruluhar" => "\u{2968}",
        b"rdldhar" => "\u{2969}",
        b"lharul" => "\u{296A}",
        b"llhard" => "\u{296B}",
        b"rharul" => "\u{296C}",
        b"lrhard" => "\u{296D}",
        b"udhar" | b"UpEquilibrium" => "\u{296E}",
        b"duhar" | b"ReverseUpEquilibrium" => "\u{296F}",
        b"RoundImplies" => "\u{2970}",
        b"erarr" => "\u{2971}",
        b"simrarr" => "\u{2972}",
        b"larrsim" => "\u{2973}",
        b"rarrsim" => "\u{2974}",
        b"rarrap" => "\u{2975}",
        b"ltlarr" => "\u{2976}",
        b"gtrarr" => "\u{2978}",
        b"subrarr" => "\u{2979}",
        b"suplarr" => "\u{297B}",
        b"lfisht" => "\u{297C}",
        b"rfisht" => "\u{297D}",
        b"ufisht" => "\u{297E}",
        b"dfisht" => "\u{297F}",
        b"lopar" => "\u{2985}",
        b"ropar" => "\u{2986}",
        b"lbrke" => "\u{298B}",
        b"rbrke" => "\u{298C}",
        b"lbrkslu" => "\u{298D}",
        b"rbrksld" => "\u{298E}",
        b"lbrksld" => "\u{298F}",
        b"rbrkslu" => "\u{2990}",
        b"langd" => "\u{2991}",
        b"rangd" => "\u{2992}",
        b"lparlt" => "\u{2993}",
        b"rpargt" => "\u{2994}",
        b"gtlPar" => "\u{2995}",
        b"ltrPar" => "\u{2996}",
        b"vzigzag" => "\u{299A}",
        b"vangrt" => "\u{299C}",
        b"angrtvbd" => "\u{299D}",
        b"ange" => "\u{29A4}",
        b"range" => "\u{29A5}",
        b"dwangle" => "\u{29A6}",
        b"uwangle" => "\u{29A7}",
        b"angmsdaa" => "\u{29A8}",
        b"angmsdab" => "\u{29A9}",
        b"angmsdac" => "\u{29AA}",
        b"angmsdad" => "\u{29AB}",
        b"angmsdae" => "\u{29AC}",
        b"angmsdaf" => "\u{29AD}",
        b"angmsdag" => "\u{29AE}",
        b"angmsdah" => "\u{29AF}",
        b"bemptyv" => "\u{29B0}",
        b"demptyv" => "\u{29B1}",
        b"cemptyv" => "\u{29B2}",
        b"raemptyv" => "\u{29B3}",
        b"laemptyv" => "\u{29B4}",
        b"ohbar" => "\u{29B5}",
        b"omid" => "\u{29B6}",
        b"opar" => "\u{29B7}",
        b"operp" => "\u{29B9}",
        b"olcross" => "\u{29BB}",
        b"odsold" => "\u{29BC}",
        b"olcir" => "\u{29BE}",
        b"ofcir" => "\u{29BF}",
        b"olt" => "\u{29C0}",
        b"ogt" => "\u{29C1}",
        b"cirscir" => "\u{29C2}",
        b"cirE" => "\u{29C3}",
        b"solb" => "\u{29C4}",
        b"bsolb" => "\u{29C5}",
        b"boxbox" => "\u{29C9}",
        b"trisb" => "\u{29CD}",
        b"rtriltri" => "\u{29CE}",
        b"LeftTriangleBar" => "\u{29CF}",
        b"RightTriangleBar" => "\u{29D0}",
        b"race" => "\u{29DA}",
        b"iinfin" => "\u{29DC}",
        b"infintie" => "\u{29DD}",
        b"nvinfin" => "\u{29DE}",
        b"eparsl" => "\u{29E3}",
        b"smeparsl" => "\u{29E4}",
        b"eqvparsl" => "\u{29E5}",
        b"lozf" | b"blacklozenge" => "\u{29EB}",
        b"RuleDelayed" => "\u{29F4}",
        b"dsol" => "\u{29F6}",
        b"xodot" | b"bigodot" => "\u{2A00}",
        b"xoplus" | b"bigoplus" => "\u{2A01}",
        b"xotime" | b"bigotimes" => "\u{2A02}",
        b"xuplus" | b"biguplus" => "\u{2A04}",
        b"xsqcup" | b"bigsqcup" => "\u{2A06}",
        b"qint" | b"iiiint" => "\u{2A0C}",
        b"fpartint" => "\u{2A0D}",
        b"cirfnint" => "\u{2A10}",
        b"awint" => "\u{2A11}",
        b"rppolint" => "\u{2A12}",
        b"scpolint" => "\u{2A13}",
        b"npolint" => "\u{2A14}",
        b"pointint" => "\u{2A15}",
        b"quatint" => "\u{2A16}",
        b"intlarhk" => "\u{2A17}",
        b"pluscir" => "\u{2A22}",
        b"plusacir" => "\u{2A23}",
        b"simplus" => "\u{2A24}",
        b"plusdu" => "\u{2A25}",
        b"plussim" => "\u{2A26}",
        b"plustwo" => "\u{2A27}",
        b"mcomma" => "\u{2A29}",
        b"minusdu" => "\u{2A2A}",
        b"loplus" => "\u{2A2D}",
        b"roplus" => "\u{2A2E}",
        b"Cross" => "\u{2A2F}",
        b"timesd" => "\u{2A30}",
        b"timesbar" => "\u{2A31}",
        b"smashp" => "\u{2A33}",
        b"lotimes" => "\u{2A34}",
        b"rotimes" => "\u{2A35}",
        b"otimesas" => "\u{2A36}",
        b"Otimes" => "\u{2A37}",
        b"odiv" => "\u{2A38}",
        b"triplus" => "\u{2A39}",
        b"triminus" => "\u{2A3A}",
        b"tritime" => "\u{2A3B}",
        b"iprod" | b"intprod" => "\u{2A3C}",
        b"amalg" => "\u{2A3F}",
        b"capdot" => "\u{2A40}",
        b"ncup" => "\u{2A42}",
        b"ncap" => "\u{2A43}",
        b"capand" => "\u{2A44}",
        b"cupor" => "\u{2A45}",
        b"cupcap" => "\u{2A46}",
        b"capcup" => "\u{2A47}",
        b"cupbrcap" => "\u{2A48}",
        b"capbrcup" => "\u{2A49}",
        b"cupcup" => "\u{2A4A}",
        b"capcap" => "\u{2A4B}",
        b"ccups" => "\u{2A4C}",
        b"ccaps" => "\u{2A4D}",
        b"ccupssm" => "\u{2A50}",
        b"And" => "\u{2A53}",
        b"Or" => "\u{2A54}",
        b"andand" => "\u{2A55}",
        b"oror" => "\u{2A56}",
        b"orslope" => "\u{2A57}",
        b"andslope" => "\u{2A58}",
        b"andv" => "\u{2A5A}",
        b"orv" => "\u{2A5B}",
        b"andd" => "\u{2A5C}",
        b"ord" => "\u{2A5D}",
        b"wedbar" => "\u{2A5F}",
        b"sdote" => "\u{2A66}",
        b"simdot" => "\u{2A6A}",
        b"congdot" => "\u{2A6D}",
        b"easter" => "\u{2A6E}",
        b"apacir" => "\u{2A6F}",
        b"apE" => "\u{2A70}",
        b"eplus" => "\u{2A71}",
        b"pluse" => "\u{2A72}",
        b"Esim" => "\u{2A73}",
        b"Colone" => "\u{2A74}",
        b"Equal" => "\u{2A75}",
        b"eDDot" | b"ddotseq" => "\u{2A77}",
        b"equivDD" => "\u{2A78}",
        b"ltcir" => "\u{2A79}",
        b"gtcir" => "\u{2A7A}",
        b"ltquest" => "\u{2A7B}",
        b"gtquest" => "\u{2A7C}",
        b"les" | b"LessSlantEqual" | b"leqslant" => "\u{2A7D}",
        b"ges" | b"GreaterSlantEqual" | b"geqslant" => "\u{2A7E}",
        b"lesdot" => "\u{2A7F}",
        b"gesdot" => "\u{2A80}",
        b"lesdoto" => "\u{2A81}",
        b"gesdoto" => "\u{2A82}",
        b"lesdotor" => "\u{2A83}",
        b"gesdotol" => "\u{2A84}",
        b"lap" | b"lessapprox" => "\u{2A85}",
        b"gap" | b"gtrapprox" => "\u{2A86}",
        b"lne" | b"lneq" => "\u{2A87}",
        b"gne" | b"gneq" => "\u{2A88}",
        b"lnap" | b"lnapprox" => "\u{2A89}",
        b"gnap" | b"gnapprox" => "\u{2A8A}",
        b"lEg" | b"lesseqqgtr" => "\u{2A8B}",
        b"gEl" | b"gtreqqless" => "\u{2A8C}",
        b"lsime" => "\u{2A8D}",
        b"gsime" => "\u{2A8E}",
        b"lsimg" => "\u{2A8F}",
        b"gsiml" => "\u{2A90}",
        b"lgE" => "\u{2A91}",
        b"glE" => "\u{2A92}",
        b"lesges" => "\u{2A93}",
        b"gesles" => "\u{2A94}",
        b"els" | b"eqslantless" => "\u{2A95}",
        b"egs" | b"eqslantgtr" => "\u{2A96}",
        b"elsdot" => "\u{2A97}",
        b"egsdot" => "\u{2A98}",
        b"el" => "\u{2A99}",
        b"eg" => "\u{2A9A}",
        b"siml" => "\u{2A9D}",
        b"simg" => "\u{2A9E}",
        b"simlE" => "\u{2A9F}",
        b"simgE" => "\u{2AA0}",
        b"LessLess" => "\u{2AA1}",
        b"GreaterGreater" => "\u{2AA2}",
        b"glj" => "\u{2AA4}",
        b"gla" => "\u{2AA5}",
        b"ltcc" => "\u{2AA6}",
        b"gtcc" => "\u{2AA7}",
        b"lescc" => "\u{2AA8}",
        b"gescc" => "\u{2AA9}",
        b"smt" => "\u{2AAA}",
        b"lat" => "\u{2AAB}",
        b"smte" => "\u{2AAC}",
        b"late" => "\u{2AAD}",
        b"bumpE" => "\u{2AAE}",
        b"pre" | b"preceq" | b"PrecedesEqual" => "\u{2AAF}",
        b"sce" | b"succeq" | b"SucceedsEqual" => "\u{2AB0}",
        b"prE" => "\u{2AB3}",
        b"scE" => "\u{2AB4}",
        b"prnE" | b"precneqq" => "\u{2AB5}",
        b"scnE" | b"succneqq" => "\u{2AB6}",
        b"prap" | b"precapprox" => "\u{2AB7}",
        b"scap" | b"succapprox" => "\u{2AB8}",
        b"prnap" | b"precnapprox" => "\u{2AB9}",
        b"scnap" | b"succnapprox" => "\u{2ABA}",
        b"Pr" => "\u{2ABB}",
        b"Sc" => "\u{2ABC}",
        b"subdot" => "\u{2ABD}",
        b"supdot" => "\u{2ABE}",
        b"subplus" => "\u{2ABF}",
        b"supplus" => "\u{2AC0}",
        b"submult" => "\u{2AC1}",
        b"supmult" => "\u{2AC2}",
        b"subedot" => "\u{2AC3}",
        b"supedot" => "\u{2AC4}",
        b"subE" | b"subseteqq" => "\u{2AC5}",
        b"supE" | b"supseteqq" => "\u{2AC6}",
        b"subsim" => "\u{2AC7}",
        b"supsim" => "\u{2AC8}",
        b"subnE" | b"subsetneqq" => "\u{2ACB}",
        b"supnE" | b"supsetneqq" => "\u{2ACC}",
        b"csub" => "\u{2ACF}",
        b"csup" => "\u{2AD0}",
        b"csube" => "\u{2AD1}",
        b"csupe" => "\u{2AD2}",
        b"subsup" => "\u{2AD3}",
        b"supsub" => "\u{2AD4}",
        b"subsub" => "\u{2AD5}",
        b"supsup" => "\u{2AD6}",
        b"suphsub" => "\u{2AD7}",
        b"supdsub" => "\u{2AD8}",
        b"forkv" => "\u{2AD9}",
        b"topfork" => "\u{2ADA}",
        b"mlcp" => "\u{2ADB}",
        b"Dashv" | b"DoubleLeftTee" => "\u{2AE4}",
        b"Vdashl" => "\u{2AE6}",
        b"Barv" => "\u{2AE7}",
        b"vBar" => "\u{2AE8}",
        b"vBarv" => "\u{2AE9}",
        b"Vbar" => "\u{2AEB}",
        b"Not" => "\u{2AEC}",
        b"bNot" => "\u{2AED}",
        b"rnmid" => "\u{2AEE}",
        b"cirmid" => "\u{2AEF}",
        b"midcir" => "\u{2AF0}",
        b"topcir" => "\u{2AF1}",
        b"nhpar" => "\u{2AF2}",
        b"parsim" => "\u{2AF3}",
        b"parsl" => "\u{2AFD}",
        b"fflig" => "\u{FB00}",
        b"filig" => "\u{FB01}",
        b"fllig" => "\u{FB02}",
        b"ffilig" => "\u{FB03}",
        b"ffllig" => "\u{FB04}",
        b"Ascr" => "\u{1D49}",
        b"Cscr" => "\u{1D49}",
        b"Dscr" => "\u{1D49}",
        b"Gscr" => "\u{1D4A}",
        b"Jscr" => "\u{1D4A}",
        b"Kscr" => "\u{1D4A}",
        b"Nscr" => "\u{1D4A}",
        b"Oscr" => "\u{1D4A}",
        b"Pscr" => "\u{1D4A}",
        b"Qscr" => "\u{1D4A}",
        b"Sscr" => "\u{1D4A}",
        b"Tscr" => "\u{1D4A}",
        b"Uscr" => "\u{1D4B}",
        b"Vscr" => "\u{1D4B}",
        b"Wscr" => "\u{1D4B}",
        b"Xscr" => "\u{1D4B}",
        b"Yscr" => "\u{1D4B}",
        b"Zscr" => "\u{1D4B}",
        b"ascr" => "\u{1D4B}",
        b"bscr" => "\u{1D4B}",
        b"cscr" => "\u{1D4B}",
        b"dscr" => "\u{1D4B}",
        b"fscr" => "\u{1D4B}",
        b"hscr" => "\u{1D4B}",
        b"iscr" => "\u{1D4B}",
        b"jscr" => "\u{1D4B}",
        b"kscr" => "\u{1D4C}",
        b"lscr" => "\u{1D4C}",
        b"mscr" => "\u{1D4C}",
        b"nscr" => "\u{1D4C}",
        b"pscr" => "\u{1D4C}",
        b"qscr" => "\u{1D4C}",
        b"rscr" => "\u{1D4C}",
        b"sscr" => "\u{1D4C}",
        b"tscr" => "\u{1D4C}",
        b"uscr" => "\u{1D4C}",
        b"vscr" => "\u{1D4C}",
        b"wscr" => "\u{1D4C}",
        b"xscr" => "\u{1D4C}",
        b"yscr" => "\u{1D4C}",
        b"zscr" => "\u{1D4C}",
        b"Afr" => "\u{1D50}",
        b"Bfr" => "\u{1D50}",
        b"Dfr" => "\u{1D50}",
        b"Efr" => "\u{1D50}",
        b"Ffr" => "\u{1D50}",
        b"Gfr" => "\u{1D50}",
        b"Jfr" => "\u{1D50}",
        b"Kfr" => "\u{1D50}",
        b"Lfr" => "\u{1D50}",
        b"Mfr" => "\u{1D51}",
        b"Nfr" => "\u{1D51}",
        b"Ofr" => "\u{1D51}",
        b"Pfr" => "\u{1D51}",
        b"Qfr" => "\u{1D51}",
        b"Sfr" => "\u{1D51}",
        b"Tfr" => "\u{1D51}",
        b"Ufr" => "\u{1D51}",
        b"Vfr" => "\u{1D51}",
        b"Wfr" => "\u{1D51}",
        b"Xfr" => "\u{1D51}",
        b"Yfr" => "\u{1D51}",
        b"afr" => "\u{1D51}",
        b"bfr" => "\u{1D51}",
        b"cfr" => "\u{1D52}",
        b"dfr" => "\u{1D52}",
        b"efr" => "\u{1D52}",
        b"ffr" => "\u{1D52}",
        b"gfr" => "\u{1D52}",
        b"hfr" => "\u{1D52}",
        b"ifr" => "\u{1D52}",
        b"jfr" => "\u{1D52}",
        b"kfr" => "\u{1D52}",
        b"lfr" => "\u{1D52}",
        b"mfr" => "\u{1D52}",
        b"nfr" => "\u{1D52}",
        b"ofr" => "\u{1D52}",
        b"pfr" => "\u{1D52}",
        b"qfr" => "\u{1D52}",
        b"rfr" => "\u{1D52}",
        b"sfr" => "\u{1D53}",
        b"tfr" => "\u{1D53}",
        b"ufr" => "\u{1D53}",
        b"vfr" => "\u{1D53}",
        b"wfr" => "\u{1D53}",
        b"xfr" => "\u{1D53}",
        b"yfr" => "\u{1D53}",
        b"zfr" => "\u{1D53}",
        b"Aopf" => "\u{1D53}",
        b"Bopf" => "\u{1D53}",
        b"Dopf" => "\u{1D53}",
        b"Eopf" => "\u{1D53}",
        b"Fopf" => "\u{1D53}",
        b"Gopf" => "\u{1D53}",
        b"Iopf" => "\u{1D54}",
        b"Jopf" => "\u{1D54}",
        b"Kopf" => "\u{1D54}",
        b"Lopf" => "\u{1D54}",
        b"Mopf" => "\u{1D54}",
        b"Oopf" => "\u{1D54}",
        b"Sopf" => "\u{1D54}",
        b"Topf" => "\u{1D54}",
        b"Uopf" => "\u{1D54}",
        b"Vopf" => "\u{1D54}",
        b"Wopf" => "\u{1D54}",
        b"Xopf" => "\u{1D54}",
        b"Yopf" => "\u{1D55}",
        b"aopf" => "\u{1D55}",
        b"bopf" => "\u{1D55}",
        b"copf" => "\u{1D55}",
        b"dopf" => "\u{1D55}",
        b"eopf" => "\u{1D55}",
        b"fopf" => "\u{1D55}",
        b"gopf" => "\u{1D55}",
        b"hopf" => "\u{1D55}",
        b"iopf" => "\u{1D55}",
        b"jopf" => "\u{1D55}",
        b"kopf" => "\u{1D55}",
        b"lopf" => "\u{1D55}",
        b"mopf" => "\u{1D55}",
        b"nopf" => "\u{1D55}",
        b"oopf" => "\u{1D56}",
        b"popf" => "\u{1D56}",
        b"qopf" => "\u{1D56}",
        b"ropf" => "\u{1D56}",
        b"sopf" => "\u{1D56}",
        b"topf" => "\u{1D56}",
        b"uopf" => "\u{1D56}",
        b"vopf" => "\u{1D56}",
        b"wopf" => "\u{1D56}",
        b"xopf" => "\u{1D56}",
        b"yopf" => "\u{1D56}",
        b"zopf" => "\u{1D56}",
        _ => return None,
    };
    Some(s)
}

fn parse_number(bytes: &str, range: Range<usize>) -> Result<char, EscapeError> {
    let code = if let Some(hex_digits) = bytes.strip_prefix('x') {
        parse_hexadecimal(hex_digits)
    } else {
        parse_decimal(bytes)
    }?;
    if code == 0 {
        return Err(EscapeError::EntityWithNull(range));
    }
    match std::char::from_u32(code) {
        Some(c) => Ok(c),
        None => Err(EscapeError::InvalidCodepoint(code)),
    }
}

fn parse_hexadecimal(bytes: &str) -> Result<u32, EscapeError> {
    // maximum code is 0x10FFFF => 6 characters
    if bytes.len() > 6 {
        return Err(EscapeError::TooLongHexadecimal);
    }
    let mut code = 0;
    for b in bytes.bytes() {
        code <<= 4;
        code += match b {
            b'0'..=b'9' => b - b'0',
            b'a'..=b'f' => b - b'a' + 10,
            b'A'..=b'F' => b - b'A' + 10,
            b => return Err(EscapeError::InvalidHexadecimal(b as char)),
        } as u32;
    }
    Ok(code)
}

fn parse_decimal(bytes: &str) -> Result<u32, EscapeError> {
    // maximum code is 0x10FFFF = 1114111 => 7 characters
    if bytes.len() > 7 {
        return Err(EscapeError::TooLongDecimal);
    }
    let mut code = 0;
    for b in bytes.bytes() {
        code *= 10;
        code += match b {
            b'0'..=b'9' => b - b'0',
            b => return Err(EscapeError::InvalidDecimal(b as char)),
        } as u32;
    }
    Ok(code)
}

#[test]
fn test_unescape() {
    let unchanged = unescape("test").unwrap();
    // assert_eq does not check that Cow is borrowed, but we explicitly use Cow
    // because it influences diff
    // TODO: use assert_matches! when stabilized and other features will bump MSRV
    assert_eq!(unchanged, Cow::Borrowed("test"));
    assert!(matches!(unchanged, Cow::Borrowed(_)));

    assert_eq!(
        unescape("&lt;&amp;test&apos;&quot;&gt;").unwrap(),
        "<&test'\">"
    );
    assert_eq!(unescape("&#x30;").unwrap(), "0");
    assert_eq!(unescape("&#48;").unwrap(), "0");
    assert!(unescape("&foo;").is_err());
}

#[test]
fn test_unescape_with() {
    let custom_entities = |ent: &str| match ent {
        "foo" => Some("BAR"),
        _ => None,
    };

    let unchanged = unescape_with("test", custom_entities).unwrap();
    // assert_eq does not check that Cow is borrowed, but we explicitly use Cow
    // because it influences diff
    // TODO: use assert_matches! when stabilized and other features will bump MSRV
    assert_eq!(unchanged, Cow::Borrowed("test"));
    assert!(matches!(unchanged, Cow::Borrowed(_)));

    assert!(unescape_with("&lt;", custom_entities).is_err());
    assert_eq!(unescape_with("&#x30;", custom_entities).unwrap(), "0");
    assert_eq!(unescape_with("&#48;", custom_entities).unwrap(), "0");
    assert_eq!(unescape_with("&foo;", custom_entities).unwrap(), "BAR");
    assert!(unescape_with("&fop;", custom_entities).is_err());
}

#[test]
fn test_escape() {
    let unchanged = escape("test");
    // assert_eq does not check that Cow is borrowed, but we explicitly use Cow
    // because it influences diff
    // TODO: use assert_matches! when stabilized and other features will bump MSRV
    assert_eq!(unchanged, Cow::Borrowed("test"));
    assert!(matches!(unchanged, Cow::Borrowed(_)));

    assert_eq!(escape("<&\"'>"), "&lt;&amp;&quot;&apos;&gt;");
    assert_eq!(escape("<test>"), "&lt;test&gt;");
    assert_eq!(escape("\"a\"bc"), "&quot;a&quot;bc");
    assert_eq!(escape("\"a\"b&c"), "&quot;a&quot;b&amp;c");
    assert_eq!(
        escape("prefix_\"a\"b&<>c"),
        "prefix_&quot;a&quot;b&amp;&lt;&gt;c"
    );
}

#[test]
fn test_partial_escape() {
    let unchanged = partial_escape("test");
    // assert_eq does not check that Cow is borrowed, but we explicitly use Cow
    // because it influences diff
    // TODO: use assert_matches! when stabilized and other features will bump MSRV
    assert_eq!(unchanged, Cow::Borrowed("test"));
    assert!(matches!(unchanged, Cow::Borrowed(_)));

    assert_eq!(partial_escape("<&\"'>"), "&lt;&amp;\"'&gt;");
    assert_eq!(partial_escape("<test>"), "&lt;test&gt;");
    assert_eq!(partial_escape("\"a\"bc"), "\"a\"bc");
    assert_eq!(partial_escape("\"a\"b&c"), "\"a\"b&amp;c");
    assert_eq!(
        partial_escape("prefix_\"a\"b&<>c"),
        "prefix_\"a\"b&amp;&lt;&gt;c"
    );
}

#[test]
fn test_minimal_escape() {
    assert_eq!(minimal_escape("test"), Cow::Borrowed("test"));
    assert_eq!(minimal_escape("<&\"'>"), "&lt;&amp;\"'>");
    assert_eq!(minimal_escape("<test>"), "&lt;test>");
    assert_eq!(minimal_escape("\"a\"bc"), "\"a\"bc");
    assert_eq!(minimal_escape("\"a\"b&c"), "\"a\"b&amp;c");
    assert_eq!(
        minimal_escape("prefix_\"a\"b&<>c"),
        "prefix_\"a\"b&amp;&lt;>c"
    );
}
