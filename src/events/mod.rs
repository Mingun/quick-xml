//! Defines zero-copy XML events used throughout this library.
//!
//! A XML event often represents part of a XML element.
//! They occur both during reading and writing and are
//! usually used with the stream-oriented API.
//!
//! For example, the XML element
//! ```xml
//! <name attr="value">Inner text</name>
//! ```
//! consists of the three events `Start`, `Text` and `End`.
//! They can also represent other parts in an XML document like the
//! XML declaration. Each Event usually contains further information,
//! like the tag name, the attribute or the inner text.
//!
//! See [`Event`] for a list of all possible events.
//!
//! # Reading
//! When reading a XML stream, the events are emitted by [`Reader::read_event`]
//! and [`Reader::read_event_into`]. You must listen
//! for the different types of events you are interested in.
//!
//! See [`Reader`] for further information.
//!
//! # Writing
//! When writing the XML document, you must create the XML element
//! by constructing the events it consists of and pass them to the writer
//! sequentially.
//!
//! See [`Writer`] for further information.
//!
//! [`Reader::read_event`]: crate::reader::Reader::read_event
//! [`Reader::read_event_into`]: crate::reader::Reader::read_event_into
//! [`Reader`]: crate::reader::Reader
//! [`Writer`]: crate::writer::Writer
//! [`Event`]: crate::events::Event

pub mod attributes;

#[cfg(feature = "encoding")]
use encoding_rs::Encoding;
use std::borrow::Cow;
use std::fmt::{self, Debug, Formatter};
use std::iter::FusedIterator;
use std::mem::replace;
use std::ops::Deref;
use std::str::from_utf8;

use crate::encoding::{Decoder, EncodingError};
use crate::errors::{Error, IllFormedError};
use crate::escape::{escape, minimal_escape, parse_number, partial_escape, EscapeError};
use crate::name::{LocalName, QName};
#[cfg(feature = "serialize")]
use crate::utils::CowRef;
use crate::utils::{name_len, trim_xml_end, trim_xml_start, write_cow_string, Bytes};
use attributes::{AttrError, Attribute, Attributes};

/// Opening tag data (`Event::Start`), with optional attributes: `<name attr="value">`.
///
/// The name can be accessed using the [`name`] or [`local_name`] methods.
/// An iterator over the attributes is returned by the [`attributes`] method.
///
/// This event implements `Deref<Target = [u8]>`. The `deref()` implementation
/// returns the content of this event between `<` and `>` or `/>`:
///
/// ```
/// # use quick_xml::events::{BytesStart, Event};
/// # use quick_xml::reader::Reader;
/// # use pretty_assertions::assert_eq;
/// // Remember, that \ at the end of string literal strips
/// // all space characters to the first non-space character
/// let mut reader = Reader::from_str("\
///     <element a1 = 'val1' a2=\"val2\" />\
///     <element a1 = 'val1' a2=\"val2\" >"
/// );
/// let content = "element a1 = 'val1' a2=\"val2\" ";
/// let event = BytesStart::from_content(content, 7);
///
/// assert_eq!(reader.read_event().unwrap(), Event::Empty(event.borrow()));
/// assert_eq!(reader.read_event().unwrap(), Event::Start(event.borrow()));
/// // deref coercion of &BytesStart to &[u8]
/// assert_eq!(&event as &[u8], content.as_bytes());
/// // AsRef<[u8]> for &T + deref coercion
/// assert_eq!(event.as_ref(), content.as_bytes());
/// ```
///
/// [`name`]: Self::name
/// [`local_name`]: Self::local_name
/// [`attributes`]: Self::attributes
#[derive(Clone, Eq, PartialEq)]
pub struct BytesStart<'a> {
    /// content of the element, before any utf8 conversion
    pub(crate) buf: Cow<'a, [u8]>,
    /// end of the element name, the name starts at that the start of `buf`
    pub(crate) name_len: usize,
}

impl<'a> BytesStart<'a> {
    /// Internal constructor, used by `Reader`. Supplies data in reader's encoding
    #[inline]
    pub(crate) const fn wrap(content: &'a [u8], name_len: usize) -> Self {
        BytesStart {
            buf: Cow::Borrowed(content),
            name_len,
        }
    }

    /// Creates a new `BytesStart` from the given name.
    ///
    /// # Warning
    ///
    /// `name` must be a valid name.
    #[inline]
    pub fn new<C: Into<Cow<'a, str>>>(name: C) -> Self {
        let buf = str_cow_to_bytes(name);
        BytesStart {
            name_len: buf.len(),
            buf,
        }
    }

    /// Creates a new `BytesStart` from the given content (name + attributes).
    ///
    /// # Warning
    ///
    /// `&content[..name_len]` must be a valid name, and the remainder of `content`
    /// must be correctly-formed attributes. Neither are checked, it is possible
    /// to generate invalid XML if `content` or `name_len` are incorrect.
    #[inline]
    pub fn from_content<C: Into<Cow<'a, str>>>(content: C, name_len: usize) -> Self {
        BytesStart {
            buf: str_cow_to_bytes(content),
            name_len,
        }
    }

    /// Converts the event into an owned event.
    pub fn into_owned(self) -> BytesStart<'static> {
        BytesStart {
            buf: Cow::Owned(self.buf.into_owned()),
            name_len: self.name_len,
        }
    }

    /// Converts the event into an owned event without taking ownership of Event
    pub fn to_owned(&self) -> BytesStart<'static> {
        BytesStart {
            buf: Cow::Owned(self.buf.clone().into_owned()),
            name_len: self.name_len,
        }
    }

    /// Converts the event into a borrowed event. Most useful when paired with [`to_end`].
    ///
    /// # Example
    ///
    /// ```
    /// use quick_xml::events::{BytesStart, Event};
    /// # use quick_xml::writer::Writer;
    /// # use quick_xml::Error;
    ///
    /// struct SomeStruct<'a> {
    ///     attrs: BytesStart<'a>,
    ///     // ...
    /// }
    /// # impl<'a> SomeStruct<'a> {
    /// # fn example(&self) -> Result<(), Error> {
    /// # let mut writer = Writer::new(Vec::new());
    ///
    /// writer.write_event(Event::Start(self.attrs.borrow()))?;
    /// // ...
    /// writer.write_event(Event::End(self.attrs.to_end()))?;
    /// # Ok(())
    /// # }}
    /// ```
    ///
    /// [`to_end`]: Self::to_end
    pub fn borrow(&self) -> BytesStart {
        BytesStart {
            buf: Cow::Borrowed(&self.buf),
            name_len: self.name_len,
        }
    }

    /// Creates new paired close tag
    #[inline]
    pub fn to_end(&self) -> BytesEnd {
        BytesEnd::from(self.name())
    }

    /// Gets the undecoded raw tag name, as present in the input stream.
    #[inline]
    pub fn name(&self) -> QName {
        QName(&self.buf[..self.name_len])
    }

    /// Gets the undecoded raw local tag name (excluding namespace) as present
    /// in the input stream.
    ///
    /// All content up to and including the first `:` character is removed from the tag name.
    #[inline]
    pub fn local_name(&self) -> LocalName {
        self.name().into()
    }

    /// Edit the name of the BytesStart in-place
    ///
    /// # Warning
    ///
    /// `name` must be a valid name.
    pub fn set_name(&mut self, name: &[u8]) -> &mut BytesStart<'a> {
        let bytes = self.buf.to_mut();
        bytes.splice(..self.name_len, name.iter().cloned());
        self.name_len = name.len();
        self
    }

    /// Gets the undecoded raw tag name, as present in the input stream, which
    /// is borrowed either to the input, or to the event.
    ///
    /// # Lifetimes
    ///
    /// - `'a`: Lifetime of the input data from which this event is borrow
    /// - `'e`: Lifetime of the concrete event instance
    // TODO: We should made this is a part of public API, but with safe wrapped for a name
    #[cfg(feature = "serialize")]
    pub(crate) fn raw_name<'e>(&'e self) -> CowRef<'a, 'e, [u8]> {
        match self.buf {
            Cow::Borrowed(b) => CowRef::Input(&b[..self.name_len]),
            Cow::Owned(ref o) => CowRef::Slice(&o[..self.name_len]),
        }
    }

    /// Well-formedness constraints
    /// ===========================
    ///
    /// [WFC: Unique Att Spec]
    /// ----------------------
    /// An attribute name MUST NOT appear more than once in the same start-tag
    /// or empty-element tag.
    ///
    /// [WFC: No External Entity References]
    /// ------------------------------------
    /// Attribute values MUST NOT contain direct or indirect entity references
    /// to external entities.
    ///
    /// [WFC: No < in Attribute Values]
    /// -------------------------------
    /// The [replacement text] of any entity referred to directly or indirectly
    /// in an attribute value MUST NOT contain a `<`.
    ///
    /// [WFC: Unique Att Spec]: https://www.w3.org/TR/xml11/#uniqattspec
    /// [WFC: No External Entity References]: https://www.w3.org/TR/xml11/#NoExternalRefs
    /// [WFC: No < in Attribute Values]: https://www.w3.org/TR/xml11/#CleanAttrVals
    /// [replacement text]: https://www.w3.org/TR/xml11/#dt-repltext
    fn check_well_formedness(&self) -> bool {
        todo!()
    }
}

/// Attribute-related methods
impl<'a> BytesStart<'a> {
    /// Consumes `self` and yield a new `BytesStart` with additional attributes from an iterator.
    ///
    /// The yielded items must be convertible to [`Attribute`] using `Into`.
    pub fn with_attributes<'b, I>(mut self, attributes: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<Attribute<'b>>,
    {
        self.extend_attributes(attributes);
        self
    }

    /// Add additional attributes to this tag using an iterator.
    ///
    /// The yielded items must be convertible to [`Attribute`] using `Into`.
    pub fn extend_attributes<'b, I>(&mut self, attributes: I) -> &mut BytesStart<'a>
    where
        I: IntoIterator,
        I::Item: Into<Attribute<'b>>,
    {
        for attr in attributes {
            self.push_attribute(attr);
        }
        self
    }

    /// Adds an attribute to this element.
    pub fn push_attribute<'b, A>(&mut self, attr: A)
    where
        A: Into<Attribute<'b>>,
    {
        self.buf.to_mut().push(b' ');
        self.push_attr(attr.into());
    }

    /// Remove all attributes from the ByteStart
    pub fn clear_attributes(&mut self) -> &mut BytesStart<'a> {
        self.buf.to_mut().truncate(self.name_len);
        self
    }

    /// Returns an iterator over the attributes of this tag.
    pub fn attributes(&self) -> Attributes {
        Attributes::wrap(&self.buf, self.name_len, false)
    }

    /// Returns an iterator over the HTML-like attributes of this tag (no mandatory quotes or `=`).
    pub fn html_attributes(&self) -> Attributes {
        Attributes::wrap(&self.buf, self.name_len, true)
    }

    /// Gets the undecoded raw string with the attributes of this tag as a `&[u8]`,
    /// including the whitespace after the tag name if there is any.
    #[inline]
    pub fn attributes_raw(&self) -> &[u8] {
        &self.buf[self.name_len..]
    }

    /// Try to get an attribute
    pub fn try_get_attribute<N: AsRef<[u8]> + Sized>(
        &'a self,
        attr_name: N,
    ) -> Result<Option<Attribute<'a>>, AttrError> {
        for a in self.attributes().with_checks(false) {
            let a = a?;
            if a.key.as_ref() == attr_name.as_ref() {
                return Ok(Some(a));
            }
        }
        Ok(None)
    }

    /// Adds an attribute to this element.
    pub(crate) fn push_attr<'b>(&mut self, attr: Attribute<'b>) {
        let bytes = self.buf.to_mut();
        bytes.extend_from_slice(attr.key.as_ref());
        bytes.extend_from_slice(b"=\"");
        // FIXME: need to escape attribute content
        bytes.extend_from_slice(attr.value.as_ref());
        bytes.push(b'"');
    }

    /// Adds new line in existing element
    pub(crate) fn push_newline(&mut self) {
        self.buf.to_mut().push(b'\n');
    }

    /// Adds indentation bytes in existing element
    pub(crate) fn push_indent(&mut self, indent: &[u8]) {
        self.buf.to_mut().extend_from_slice(indent);
    }
}

impl<'a> Debug for BytesStart<'a> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "BytesStart {{ buf: ")?;
        write_cow_string(f, &self.buf)?;
        write!(f, ", name_len: {} }}", self.name_len)
    }
}

impl<'a> Deref for BytesStart<'a> {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.buf
    }
}

impl<'a> From<QName<'a>> for BytesStart<'a> {
    #[inline]
    fn from(name: QName<'a>) -> Self {
        let name = name.into_inner();
        Self::wrap(name, name.len())
    }
}

#[cfg(feature = "arbitrary")]
impl<'a> arbitrary::Arbitrary<'a> for BytesStart<'a> {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        let s = <&str>::arbitrary(u)?;
        if s.is_empty() || !s.chars().all(char::is_alphanumeric) {
            return Err(arbitrary::Error::IncorrectFormat);
        }
        let mut result = Self::new(s);
        result.extend_attributes(Vec::<(&str, &str)>::arbitrary(u)?.into_iter());
        Ok(result)
    }

    fn size_hint(depth: usize) -> (usize, Option<usize>) {
        return <&str as arbitrary::Arbitrary>::size_hint(depth);
    }
}
////////////////////////////////////////////////////////////////////////////////////////////////////

/// Closing tag data (`Event::End`): `</name>`.
///
/// The name can be accessed using the [`name`] or [`local_name`] methods.
///
/// This event implements `Deref<Target = [u8]>`. The `deref()` implementation
/// returns the content of this event between `</` and `>`.
///
/// Note, that inner text will not contain `>` character inside:
///
/// ```
/// # use quick_xml::events::{BytesEnd, Event};
/// # use quick_xml::reader::Reader;
/// # use pretty_assertions::assert_eq;
/// let mut reader = Reader::from_str(r#"<element></element a1 = 'val1' a2="val2" >"#);
/// // Note, that this entire string considered as a .name()
/// let content = "element a1 = 'val1' a2=\"val2\" ";
/// let event = BytesEnd::new(content);
///
/// reader.config_mut().trim_markup_names_in_closing_tags = false;
/// reader.config_mut().check_end_names = false;
/// reader.read_event().unwrap(); // Skip `<element>`
///
/// assert_eq!(reader.read_event().unwrap(), Event::End(event.borrow()));
/// assert_eq!(event.name().as_ref(), content.as_bytes());
/// // deref coercion of &BytesEnd to &[u8]
/// assert_eq!(&event as &[u8], content.as_bytes());
/// // AsRef<[u8]> for &T + deref coercion
/// assert_eq!(event.as_ref(), content.as_bytes());
/// ```
///
/// [`name`]: Self::name
/// [`local_name`]: Self::local_name
#[derive(Clone, Eq, PartialEq)]
pub struct BytesEnd<'a> {
    name: Cow<'a, [u8]>,
}

impl<'a> BytesEnd<'a> {
    /// Internal constructor, used by `Reader`. Supplies data in reader's encoding
    #[inline]
    pub(crate) const fn wrap(name: Cow<'a, [u8]>) -> Self {
        BytesEnd { name }
    }

    /// Creates a new `BytesEnd` borrowing a slice.
    ///
    /// # Warning
    ///
    /// `name` must be a valid name.
    #[inline]
    pub fn new<C: Into<Cow<'a, str>>>(name: C) -> Self {
        Self::wrap(str_cow_to_bytes(name))
    }

    /// Converts the event into an owned event.
    pub fn into_owned(self) -> BytesEnd<'static> {
        BytesEnd {
            name: Cow::Owned(self.name.into_owned()),
        }
    }

    /// Converts the event into a borrowed event.
    #[inline]
    pub fn borrow(&self) -> BytesEnd {
        BytesEnd {
            name: Cow::Borrowed(&self.name),
        }
    }

    /// Gets the undecoded raw tag name, as present in the input stream.
    #[inline]
    pub fn name(&self) -> QName {
        QName(&self.name)
    }

    /// Gets the undecoded raw local tag name (excluding namespace) as present
    /// in the input stream.
    ///
    /// All content up to and including the first `:` character is removed from the tag name.
    #[inline]
    pub fn local_name(&self) -> LocalName {
        self.name().into()
    }

    /// Well-formedness constraints
    /// ===========================
    ///
    /// [WFC: Element Type Match]
    /// -------------------------
    /// The Name in an element's end-tag MUST match the element type in the start-tag.
    ///
    /// [WFC: Element Type Match]: https://www.w3.org/TR/xml11/#GIMatch
    fn check_well_formedness(&self) -> bool {
        todo!()
    }
}

impl<'a> Debug for BytesEnd<'a> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "BytesEnd {{ name: ")?;
        write_cow_string(f, &self.name)?;
        write!(f, " }}")
    }
}

impl<'a> Deref for BytesEnd<'a> {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.name
    }
}

impl<'a> From<QName<'a>> for BytesEnd<'a> {
    #[inline]
    fn from(name: QName<'a>) -> Self {
        Self::wrap(name.into_inner().into())
    }
}

#[cfg(feature = "arbitrary")]
impl<'a> arbitrary::Arbitrary<'a> for BytesEnd<'a> {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        Ok(Self::new(<&str>::arbitrary(u)?))
    }
    fn size_hint(depth: usize) -> (usize, Option<usize>) {
        return <&str as arbitrary::Arbitrary>::size_hint(depth);
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////

/// Data from various events (most notably, `Event::Text`) that stored in XML
/// in escaped form. Internally data is stored in escaped form.
///
/// This event implements `Deref<Target = [u8]>`. The `deref()` implementation
/// returns the content of this event. In case of comment this is everything
/// between `<!--` and `-->` and the text of comment will not contain `-->` inside.
/// In case of DTD this is everything between `<!DOCTYPE` + spaces and closing `>`
/// (i.e. in case of DTD the first character is never space):
///
/// ```
/// # use quick_xml::events::{BytesText, Event};
/// # use quick_xml::reader::Reader;
/// # use pretty_assertions::assert_eq;
/// // Remember, that \ at the end of string literal strips
/// // all space characters to the first non-space character
/// let mut reader = Reader::from_str("\
///     <!DOCTYPE comment or text >\
///     comment or text \
///     <!--comment or text -->"
/// );
/// let content = "comment or text ";
/// let event = BytesText::new(content);
///
/// assert_eq!(reader.read_event().unwrap(), Event::DocType(event.borrow()));
/// assert_eq!(reader.read_event().unwrap(), Event::Text(event.borrow()));
/// assert_eq!(reader.read_event().unwrap(), Event::Comment(event.borrow()));
/// // deref coercion of &BytesText to &[u8]
/// assert_eq!(&event as &[u8], content.as_bytes());
/// // AsRef<[u8]> for &T + deref coercion
/// assert_eq!(event.as_ref(), content.as_bytes());
/// ```
#[derive(Clone, Eq, PartialEq)]
pub struct BytesText<'a> {
    /// Escaped then encoded content of the event. Content is encoded in the XML
    /// document encoding when event comes from the reader and should be in the
    /// document encoding when event passed to the writer
    content: Cow<'a, [u8]>,
    /// Encoding in which the `content` is stored inside the event
    decoder: Decoder,
}

impl<'a> BytesText<'a> {
    /// Creates a new `BytesText` from an escaped byte sequence in the specified encoding.
    #[inline]
    pub(crate) fn wrap<C: Into<Cow<'a, [u8]>>>(content: C, decoder: Decoder) -> Self {
        Self {
            content: content.into(),
            decoder,
        }
    }

    /// Creates a new `BytesText` from an escaped string.
    #[inline]
    pub fn from_escaped<C: Into<Cow<'a, str>>>(content: C) -> Self {
        Self::wrap(str_cow_to_bytes(content), Decoder::utf8())
    }

    /// Creates a new `BytesText` from a string. The string is expected not to
    /// be escaped.
    #[inline]
    pub fn new(content: &'a str) -> Self {
        Self::from_escaped(escape(content))
    }

    /// Ensures that all data is owned to extend the object's lifetime if
    /// necessary.
    #[inline]
    pub fn into_owned(self) -> BytesText<'static> {
        BytesText {
            content: self.content.into_owned().into(),
            decoder: self.decoder,
        }
    }

    /// Extracts the inner `Cow` from the `BytesText` event container.
    #[inline]
    pub fn into_inner(self) -> Cow<'a, [u8]> {
        self.content
    }

    /// Converts the event into a borrowed event.
    #[inline]
    pub fn borrow(&self) -> BytesText {
        BytesText {
            content: Cow::Borrowed(&self.content),
            decoder: self.decoder,
        }
    }

    /// Decodes the content of the event.
    ///
    /// This will allocate if the value contains any escape sequences or in
    /// non-UTF-8 encoding.
    pub fn decode(&self) -> Result<Cow<'a, str>, EncodingError> {
        self.decoder.decode_cow(&self.content)
    }

    /// Removes leading XML whitespace bytes from text content.
    ///
    /// Returns `true` if content is empty after that
    pub fn inplace_trim_start(&mut self) -> bool {
        self.content = trim_cow(
            replace(&mut self.content, Cow::Borrowed(b"")),
            trim_xml_start,
        );
        self.content.is_empty()
    }

    /// Removes trailing XML whitespace bytes from text content.
    ///
    /// Returns `true` if content is empty after that
    pub fn inplace_trim_end(&mut self) -> bool {
        self.content = trim_cow(replace(&mut self.content, Cow::Borrowed(b"")), trim_xml_end);
        self.content.is_empty()
    }
}

impl<'a> Debug for BytesText<'a> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "BytesText {{ content: ")?;
        write_cow_string(f, &self.content)?;
        write!(f, " }}")
    }
}

impl<'a> Deref for BytesText<'a> {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.content
    }
}

#[cfg(feature = "arbitrary")]
impl<'a> arbitrary::Arbitrary<'a> for BytesText<'a> {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        let s = <&str>::arbitrary(u)?;
        if !s.chars().all(char::is_alphanumeric) {
            return Err(arbitrary::Error::IncorrectFormat);
        }
        Ok(Self::new(s))
    }

    fn size_hint(depth: usize) -> (usize, Option<usize>) {
        return <&str as arbitrary::Arbitrary>::size_hint(depth);
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////

/// CDATA content contains unescaped data from the reader. If you want to write them as a text,
/// [convert](Self::escape) it to [`BytesText`].
///
/// This event implements `Deref<Target = [u8]>`. The `deref()` implementation
/// returns the content of this event between `<![CDATA[` and `]]>`.
///
/// Note, that inner text will not contain `]]>` sequence inside:
///
/// ```
/// # use quick_xml::events::{BytesCData, Event};
/// # use quick_xml::reader::Reader;
/// # use pretty_assertions::assert_eq;
/// let mut reader = Reader::from_str("<![CDATA[ CDATA section ]]>");
/// let content = " CDATA section ";
/// let event = BytesCData::new(content);
///
/// assert_eq!(reader.read_event().unwrap(), Event::CData(event.borrow()));
/// // deref coercion of &BytesCData to &[u8]
/// assert_eq!(&event as &[u8], content.as_bytes());
/// // AsRef<[u8]> for &T + deref coercion
/// assert_eq!(event.as_ref(), content.as_bytes());
/// ```
#[derive(Clone, Eq, PartialEq)]
pub struct BytesCData<'a> {
    content: Cow<'a, [u8]>,
    /// Encoding in which the `content` is stored inside the event
    decoder: Decoder,
}

impl<'a> BytesCData<'a> {
    /// Creates a new `BytesCData` from a byte sequence in the specified encoding.
    #[inline]
    pub(crate) fn wrap<C: Into<Cow<'a, [u8]>>>(content: C, decoder: Decoder) -> Self {
        Self {
            content: content.into(),
            decoder,
        }
    }

    /// Creates a new `BytesCData` from a string.
    ///
    /// # Warning
    ///
    /// `content` must not contain the `]]>` sequence. You can use
    /// [`BytesCData::escaped`] to escape the content instead.
    #[inline]
    pub fn new<C: Into<Cow<'a, str>>>(content: C) -> Self {
        Self::wrap(str_cow_to_bytes(content), Decoder::utf8())
    }

    /// Creates an iterator of `BytesCData` from a string.
    ///
    /// If a string contains `]]>`, it needs to be split into multiple `CDATA`
    /// sections, splitting the `]]` and `>` characters, because the CDATA closing
    /// sequence cannot be escaped. This iterator yields a `BytesCData` instance
    /// for each of those sections.
    ///
    /// # Examples
    ///
    /// ```
    /// # use quick_xml::events::BytesCData;
    /// # use pretty_assertions::assert_eq;
    /// let content = "";
    /// let cdata = BytesCData::escaped(content).collect::<Vec<_>>();
    /// assert_eq!(cdata, &[BytesCData::new("")]);
    ///
    /// let content = "Certain tokens like ]]> can be difficult and <invalid>";
    /// let cdata = BytesCData::escaped(content).collect::<Vec<_>>();
    /// assert_eq!(cdata, &[
    ///     BytesCData::new("Certain tokens like ]]"),
    ///     BytesCData::new("> can be difficult and <invalid>"),
    /// ]);
    ///
    /// let content = "foo]]>bar]]>baz]]>quux";
    /// let cdata = BytesCData::escaped(content).collect::<Vec<_>>();
    /// assert_eq!(cdata, &[
    ///     BytesCData::new("foo]]"),
    ///     BytesCData::new(">bar]]"),
    ///     BytesCData::new(">baz]]"),
    ///     BytesCData::new(">quux"),
    /// ]);
    /// ```
    #[inline]
    pub fn escaped(content: &'a str) -> CDataIterator<'a> {
        CDataIterator {
            unprocessed: content.as_bytes(),
            finished: false,
        }
    }

    /// Ensures that all data is owned to extend the object's lifetime if
    /// necessary.
    #[inline]
    pub fn into_owned(self) -> BytesCData<'static> {
        BytesCData {
            content: self.content.into_owned().into(),
            decoder: self.decoder,
        }
    }

    /// Extracts the inner `Cow` from the `BytesCData` event container.
    #[inline]
    pub fn into_inner(self) -> Cow<'a, [u8]> {
        self.content
    }

    /// Converts the event into a borrowed event.
    #[inline]
    pub fn borrow(&self) -> BytesCData {
        BytesCData {
            content: Cow::Borrowed(&self.content),
            decoder: self.decoder,
        }
    }

    /// Converts this CDATA content to an escaped version, that can be written
    /// as an usual text in XML.
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
    pub fn escape(self) -> Result<BytesText<'a>, EncodingError> {
        let decoded = self.decode()?;
        Ok(BytesText::wrap(
            match escape(decoded) {
                Cow::Borrowed(escaped) => Cow::Borrowed(escaped.as_bytes()),
                Cow::Owned(escaped) => Cow::Owned(escaped.into_bytes()),
            },
            Decoder::utf8(),
        ))
    }

    /// Converts this CDATA content to an escaped version, that can be written
    /// as an usual text in XML.
    ///
    /// In XML text content, it is allowed (though not recommended) to leave
    /// the quote special characters `"` and `'` unescaped.
    ///
    /// This function performs following replacements:
    ///
    /// | Character | Replacement
    /// |-----------|------------
    /// | `<`       | `&lt;`
    /// | `>`       | `&gt;`
    /// | `&`       | `&amp;`
    pub fn partial_escape(self) -> Result<BytesText<'a>, EncodingError> {
        let decoded = self.decode()?;
        Ok(BytesText::wrap(
            match partial_escape(decoded) {
                Cow::Borrowed(escaped) => Cow::Borrowed(escaped.as_bytes()),
                Cow::Owned(escaped) => Cow::Owned(escaped.into_bytes()),
            },
            Decoder::utf8(),
        ))
    }

    /// Converts this CDATA content to an escaped version, that can be written
    /// as an usual text in XML. This method escapes only those characters that
    /// must be escaped according to the [specification].
    ///
    /// This function performs following replacements:
    ///
    /// | Character | Replacement
    /// |-----------|------------
    /// | `<`       | `&lt;`
    /// | `&`       | `&amp;`
    ///
    /// [specification]: https://www.w3.org/TR/xml11/#syntax
    pub fn minimal_escape(self) -> Result<BytesText<'a>, EncodingError> {
        let decoded = self.decode()?;
        Ok(BytesText::wrap(
            match minimal_escape(decoded) {
                Cow::Borrowed(escaped) => Cow::Borrowed(escaped.as_bytes()),
                Cow::Owned(escaped) => Cow::Owned(escaped.into_bytes()),
            },
            Decoder::utf8(),
        ))
    }

    /// Gets content of this text buffer in the specified encoding
    pub(crate) fn decode(&self) -> Result<Cow<'a, str>, EncodingError> {
        Ok(self.decoder.decode_cow(&self.content)?)
    }
}

impl<'a> Debug for BytesCData<'a> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "BytesCData {{ content: ")?;
        write_cow_string(f, &self.content)?;
        write!(f, " }}")
    }
}

impl<'a> Deref for BytesCData<'a> {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.content
    }
}

#[cfg(feature = "arbitrary")]
impl<'a> arbitrary::Arbitrary<'a> for BytesCData<'a> {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        Ok(Self::new(<&str>::arbitrary(u)?))
    }
    fn size_hint(depth: usize) -> (usize, Option<usize>) {
        return <&str as arbitrary::Arbitrary>::size_hint(depth);
    }
}

/// Iterator over `CDATA` sections in a string.
///
/// This iterator is created by the [`BytesCData::escaped`] method.
#[derive(Clone)]
pub struct CDataIterator<'a> {
    /// The unprocessed data which should be emitted as `BytesCData` events.
    /// At each iteration, the processed data is cut from this slice.
    unprocessed: &'a [u8],
    finished: bool,
}

impl<'a> Debug for CDataIterator<'a> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("CDataIterator")
            .field("unprocessed", &Bytes(self.unprocessed))
            .field("finished", &self.finished)
            .finish()
    }
}

impl<'a> Iterator for CDataIterator<'a> {
    type Item = BytesCData<'a>;

    fn next(&mut self) -> Option<BytesCData<'a>> {
        if self.finished {
            return None;
        }

        for gt in memchr::memchr_iter(b'>', self.unprocessed) {
            if self.unprocessed[..gt].ends_with(b"]]") {
                let (slice, rest) = self.unprocessed.split_at(gt);
                self.unprocessed = rest;
                return Some(BytesCData::wrap(slice, Decoder::utf8()));
            }
        }

        self.finished = true;
        Some(BytesCData::wrap(self.unprocessed, Decoder::utf8()))
    }
}

impl FusedIterator for CDataIterator<'_> {}

////////////////////////////////////////////////////////////////////////////////////////////////////

/// [Processing instructions][PI] (PIs) allow documents to contain instructions for applications.
///
/// This event implements `Deref<Target = [u8]>`. The `deref()` implementation
/// returns the content of this event between `<?` and `?>`.
///
/// Note, that inner text will not contain `?>` sequence inside:
///
/// ```
/// # use quick_xml::events::{BytesPI, Event};
/// # use quick_xml::reader::Reader;
/// # use pretty_assertions::assert_eq;
/// let mut reader = Reader::from_str("<?processing instruction >:-<~ ?>");
/// let content = "processing instruction >:-<~ ";
/// let event = BytesPI::new(content);
///
/// assert_eq!(reader.read_event().unwrap(), Event::PI(event.borrow()));
/// // deref coercion of &BytesPI to &[u8]
/// assert_eq!(&event as &[u8], content.as_bytes());
/// // AsRef<[u8]> for &T + deref coercion
/// assert_eq!(event.as_ref(), content.as_bytes());
/// ```
///
/// [PI]: https://www.w3.org/TR/xml11/#sec-pi
#[derive(Clone, Eq, PartialEq)]
pub struct BytesPI<'a> {
    content: BytesStart<'a>,
}

impl<'a> BytesPI<'a> {
    /// Creates a new `BytesPI` from a byte sequence in the specified encoding.
    #[inline]
    pub(crate) const fn wrap(content: &'a [u8], target_len: usize) -> Self {
        Self {
            content: BytesStart::wrap(content, target_len),
        }
    }

    /// Creates a new `BytesPI` from a string.
    ///
    /// # Warning
    ///
    /// `content` must not contain the `?>` sequence.
    #[inline]
    pub fn new<C: Into<Cow<'a, str>>>(content: C) -> Self {
        let buf = str_cow_to_bytes(content);
        let name_len = name_len(&buf);
        Self {
            content: BytesStart { buf, name_len },
        }
    }

    /// Ensures that all data is owned to extend the object's lifetime if
    /// necessary.
    #[inline]
    pub fn into_owned(self) -> BytesPI<'static> {
        BytesPI {
            content: self.content.into_owned().into(),
        }
    }

    /// Extracts the inner `Cow` from the `BytesPI` event container.
    #[inline]
    pub fn into_inner(self) -> Cow<'a, [u8]> {
        self.content.buf
    }

    /// Converts the event into a borrowed event.
    #[inline]
    pub fn borrow(&self) -> BytesPI {
        BytesPI {
            content: self.content.borrow(),
        }
    }

    /// A target used to identify the application to which the instruction is directed.
    ///
    /// # Example
    ///
    /// ```
    /// # use pretty_assertions::assert_eq;
    /// use quick_xml::events::BytesPI;
    ///
    /// let instruction = BytesPI::new(r#"xml-stylesheet href="style.css""#);
    /// assert_eq!(instruction.target(), b"xml-stylesheet");
    /// ```
    #[inline]
    pub fn target(&self) -> &[u8] {
        self.content.name().0
    }

    /// Content of the processing instruction. Contains everything between target
    /// name and the end of the instruction. A direct consequence is that the first
    /// character is always a space character.
    ///
    /// # Example
    ///
    /// ```
    /// # use pretty_assertions::assert_eq;
    /// use quick_xml::events::BytesPI;
    ///
    /// let instruction = BytesPI::new(r#"xml-stylesheet href="style.css""#);
    /// assert_eq!(instruction.content(), br#" href="style.css""#);
    /// ```
    #[inline]
    pub fn content(&self) -> &[u8] {
        self.content.attributes_raw()
    }

    /// A view of the processing instructions' content as a list of key-value pairs.
    ///
    /// Key-value pairs are used in some processing instructions, for example in
    /// `<?xml-stylesheet?>`.
    ///
    /// Returned iterator does not validate attribute values as may required by
    /// target's rules. For example, it doesn't check that substring `?>` is not
    /// present in the attribute value. That shouldn't be the problem when event
    /// is produced by the reader, because reader detects end of processing instruction
    /// by the first `?>` sequence, as required by the specification, and therefore
    /// this sequence cannot appear inside it.
    ///
    /// # Example
    ///
    /// ```
    /// # use pretty_assertions::assert_eq;
    /// use std::borrow::Cow;
    /// use quick_xml::events::attributes::Attribute;
    /// use quick_xml::events::BytesPI;
    /// use quick_xml::name::QName;
    ///
    /// let instruction = BytesPI::new(r#"xml-stylesheet href="style.css""#);
    /// for attr in instruction.attributes() {
    ///     assert_eq!(attr, Ok(Attribute {
    ///         key: QName(b"href"),
    ///         value: Cow::Borrowed(b"style.css"),
    ///     }));
    /// }
    /// ```
    #[inline]
    pub fn attributes(&self) -> Attributes {
        self.content.attributes()
    }
}

impl<'a> Debug for BytesPI<'a> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "BytesPI {{ content: ")?;
        write_cow_string(f, &self.content.buf)?;
        write!(f, " }}")
    }
}

impl<'a> Deref for BytesPI<'a> {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.content
    }
}

#[cfg(feature = "arbitrary")]
impl<'a> arbitrary::Arbitrary<'a> for BytesPI<'a> {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        Ok(Self::new(<&str>::arbitrary(u)?))
    }
    fn size_hint(depth: usize) -> (usize, Option<usize>) {
        return <&str as arbitrary::Arbitrary>::size_hint(depth);
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////

/// An XML declaration (`Event::Decl`).
///
/// [W3C XML 1.1 Prolog and Document Type Declaration](http://w3.org/TR/xml11/#sec-prolog-dtd)
///
/// This event implements `Deref<Target = [u8]>`. The `deref()` implementation
/// returns the content of this event between `<?` and `?>`.
///
/// Note, that inner text will not contain `?>` sequence inside:
///
/// ```
/// # use quick_xml::events::{BytesDecl, BytesStart, Event};
/// # use quick_xml::reader::Reader;
/// # use pretty_assertions::assert_eq;
/// let mut reader = Reader::from_str("<?xml version = '1.0' ?>");
/// let content = "xml version = '1.0' ";
/// let event = BytesDecl::from_start(BytesStart::from_content(content, 3));
///
/// assert_eq!(reader.read_event().unwrap(), Event::Decl(event.borrow()));
/// // deref coercion of &BytesDecl to &[u8]
/// assert_eq!(&event as &[u8], content.as_bytes());
/// // AsRef<[u8]> for &T + deref coercion
/// assert_eq!(event.as_ref(), content.as_bytes());
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BytesDecl<'a> {
    content: BytesStart<'a>,
}

impl<'a> BytesDecl<'a> {
    /// Constructs a new `XmlDecl` from the (mandatory) _version_ (should be `1.0` or `1.1`),
    /// the optional _encoding_ (e.g., `UTF-8`) and the optional _standalone_ (`yes` or `no`)
    /// attribute.
    ///
    /// Does not escape any of its inputs. Always uses double quotes to wrap the attribute values.
    /// The caller is responsible for escaping attribute values. Shouldn't usually be relevant since
    /// the double quote character is not allowed in any of the attribute values.
    pub fn new(
        version: &str,
        encoding: Option<&str>,
        standalone: Option<&str>,
    ) -> BytesDecl<'static> {
        // Compute length of the buffer based on supplied attributes
        // ' encoding=""'   => 12
        let encoding_attr_len = if let Some(xs) = encoding {
            12 + xs.len()
        } else {
            0
        };
        // ' standalone=""' => 14
        let standalone_attr_len = if let Some(xs) = standalone {
            14 + xs.len()
        } else {
            0
        };
        // 'xml version=""' => 14
        let mut buf = String::with_capacity(14 + encoding_attr_len + standalone_attr_len);

        buf.push_str("xml version=\"");
        buf.push_str(version);

        if let Some(encoding_val) = encoding {
            buf.push_str("\" encoding=\"");
            buf.push_str(encoding_val);
        }

        if let Some(standalone_val) = standalone {
            buf.push_str("\" standalone=\"");
            buf.push_str(standalone_val);
        }
        buf.push('"');

        BytesDecl {
            content: BytesStart::from_content(buf, 3),
        }
    }

    /// Creates a `BytesDecl` from a `BytesStart`
    pub const fn from_start(start: BytesStart<'a>) -> Self {
        Self { content: start }
    }

    /// Gets xml version, excluding quotes (`'` or `"`).
    ///
    /// According to the [grammar], the version *must* be the first thing in the declaration.
    /// This method tries to extract the first thing in the declaration and return it.
    /// In case of multiple attributes value of the first one is returned.
    ///
    /// If version is missed in the declaration, or the first thing is not a version,
    /// [`IllFormedError::MissingDeclVersion`] will be returned.
    ///
    /// # Examples
    ///
    /// ```
    /// use quick_xml::errors::{Error, IllFormedError};
    /// use quick_xml::events::{BytesDecl, BytesStart};
    ///
    /// // <?xml version='1.1'?>
    /// let decl = BytesDecl::from_start(BytesStart::from_content(" version='1.1'", 0));
    /// assert_eq!(decl.version().unwrap(), b"1.1".as_ref());
    ///
    /// // <?xml version='1.0' version='1.1'?>
    /// let decl = BytesDecl::from_start(BytesStart::from_content(" version='1.0' version='1.1'", 0));
    /// assert_eq!(decl.version().unwrap(), b"1.0".as_ref());
    ///
    /// // <?xml encoding='utf-8'?>
    /// let decl = BytesDecl::from_start(BytesStart::from_content(" encoding='utf-8'", 0));
    /// match decl.version() {
    ///     Err(Error::IllFormed(IllFormedError::MissingDeclVersion(Some(key)))) => assert_eq!(key, "encoding"),
    ///     _ => assert!(false),
    /// }
    ///
    /// // <?xml encoding='utf-8' version='1.1'?>
    /// let decl = BytesDecl::from_start(BytesStart::from_content(" encoding='utf-8' version='1.1'", 0));
    /// match decl.version() {
    ///     Err(Error::IllFormed(IllFormedError::MissingDeclVersion(Some(key)))) => assert_eq!(key, "encoding"),
    ///     _ => assert!(false),
    /// }
    ///
    /// // <?xml?>
    /// let decl = BytesDecl::from_start(BytesStart::from_content("", 0));
    /// match decl.version() {
    ///     Err(Error::IllFormed(IllFormedError::MissingDeclVersion(None))) => {},
    ///     _ => assert!(false),
    /// }
    /// ```
    ///
    /// [grammar]: https://www.w3.org/TR/xml11/#NT-XMLDecl
    pub fn version(&self) -> Result<Cow<[u8]>, Error> {
        // The version *must* be the first thing in the declaration.
        match self.content.attributes().with_checks(false).next() {
            Some(Ok(a)) if a.key.as_ref() == b"version" => Ok(a.value),
            // first attribute was not "version"
            Some(Ok(a)) => {
                let found = from_utf8(a.key.as_ref())
                    .map_err(|_| IllFormedError::MissingDeclVersion(None))?
                    .to_string();
                Err(Error::IllFormed(IllFormedError::MissingDeclVersion(Some(
                    found,
                ))))
            }
            // error parsing attributes
            Some(Err(e)) => Err(e.into()),
            // no attributes
            None => Err(Error::IllFormed(IllFormedError::MissingDeclVersion(None))),
        }
    }

    /// Gets xml encoding, excluding quotes (`'` or `"`).
    ///
    /// Although according to the [grammar] encoding must appear before `"standalone"`
    /// and after `"version"`, this method does not check that. The first occurrence
    /// of the attribute will be returned even if there are several. Also, method does
    /// not restrict symbols that can forming the encoding, so the returned encoding
    /// name may not correspond to the grammar.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::borrow::Cow;
    /// use quick_xml::Error;
    /// use quick_xml::events::{BytesDecl, BytesStart};
    ///
    /// // <?xml version='1.1'?>
    /// let decl = BytesDecl::from_start(BytesStart::from_content(" version='1.1'", 0));
    /// assert!(decl.encoding().is_none());
    ///
    /// // <?xml encoding='utf-8'?>
    /// let decl = BytesDecl::from_start(BytesStart::from_content(" encoding='utf-8'", 0));
    /// match decl.encoding() {
    ///     Some(Ok(Cow::Borrowed(encoding))) => assert_eq!(encoding, b"utf-8"),
    ///     _ => assert!(false),
    /// }
    ///
    /// // <?xml encoding='something_WRONG' encoding='utf-8'?>
    /// let decl = BytesDecl::from_start(BytesStart::from_content(" encoding='something_WRONG' encoding='utf-8'", 0));
    /// match decl.encoding() {
    ///     Some(Ok(Cow::Borrowed(encoding))) => assert_eq!(encoding, b"something_WRONG"),
    ///     _ => assert!(false),
    /// }
    /// ```
    ///
    /// [grammar]: https://www.w3.org/TR/xml11/#NT-XMLDecl
    pub fn encoding(&self) -> Option<Result<Cow<[u8]>, AttrError>> {
        self.content
            .try_get_attribute("encoding")
            .map(|a| a.map(|a| a.value))
            .transpose()
    }

    /// Gets xml standalone, excluding quotes (`'` or `"`).
    ///
    /// Although according to the [grammar] standalone flag must appear after `"version"`
    /// and `"encoding"`, this method does not check that. The first occurrence of the
    /// attribute will be returned even if there are several. Also, method does not
    /// restrict symbols that can forming the value, so the returned flag name may not
    /// correspond to the grammar.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::borrow::Cow;
    /// use quick_xml::Error;
    /// use quick_xml::events::{BytesDecl, BytesStart};
    ///
    /// // <?xml version='1.1'?>
    /// let decl = BytesDecl::from_start(BytesStart::from_content(" version='1.1'", 0));
    /// assert!(decl.standalone().is_none());
    ///
    /// // <?xml standalone='yes'?>
    /// let decl = BytesDecl::from_start(BytesStart::from_content(" standalone='yes'", 0));
    /// match decl.standalone() {
    ///     Some(Ok(Cow::Borrowed(encoding))) => assert_eq!(encoding, b"yes"),
    ///     _ => assert!(false),
    /// }
    ///
    /// // <?xml standalone='something_WRONG' encoding='utf-8'?>
    /// let decl = BytesDecl::from_start(BytesStart::from_content(" standalone='something_WRONG' encoding='utf-8'", 0));
    /// match decl.standalone() {
    ///     Some(Ok(Cow::Borrowed(flag))) => assert_eq!(flag, b"something_WRONG"),
    ///     _ => assert!(false),
    /// }
    /// ```
    ///
    /// [grammar]: https://www.w3.org/TR/xml11/#NT-XMLDecl
    pub fn standalone(&self) -> Option<Result<Cow<[u8]>, AttrError>> {
        self.content
            .try_get_attribute("standalone")
            .map(|a| a.map(|a| a.value))
            .transpose()
    }

    /// Gets the actual encoding using [_get an encoding_](https://encoding.spec.whatwg.org/#concept-encoding-get)
    /// algorithm.
    ///
    /// If encoding in not known, or `encoding` key was not found, returns `None`.
    /// In case of duplicated `encoding` key, encoding, corresponding to the first
    /// one, is returned.
    #[cfg(feature = "encoding")]
    pub fn encoder(&self) -> Option<&'static Encoding> {
        self.encoding()
            .and_then(|e| e.ok())
            .and_then(|e| Encoding::for_label(&e))
    }

    /// Converts the event into an owned event.
    pub fn into_owned(self) -> BytesDecl<'static> {
        BytesDecl {
            content: self.content.into_owned(),
        }
    }

    /// Converts the event into a borrowed event.
    #[inline]
    pub fn borrow(&self) -> BytesDecl {
        BytesDecl {
            content: self.content.borrow(),
        }
    }
}

impl<'a> Deref for BytesDecl<'a> {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.content
    }
}

#[cfg(feature = "arbitrary")]
impl<'a> arbitrary::Arbitrary<'a> for BytesDecl<'a> {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        Ok(Self::new(
            <&str>::arbitrary(u)?,
            Option::<&str>::arbitrary(u)?,
            Option::<&str>::arbitrary(u)?,
        ))
    }

    fn size_hint(depth: usize) -> (usize, Option<usize>) {
        return <&str as arbitrary::Arbitrary>::size_hint(depth);
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////

/// Character or general entity reference (`Event::GeneralRef`): `&ref;` or `&#<number>;`.
///
/// This event implements `Deref<Target = [u8]>`. The `deref()` implementation
/// returns the content of this event between `&` and `;`:
///
/// ```
/// # use quick_xml::events::{BytesRef, Event};
/// # use quick_xml::reader::Reader;
/// # use pretty_assertions::assert_eq;
/// let mut reader = Reader::from_str(r#"&entity;"#);
/// let content = "entity";
/// let event = BytesRef::new(content);
///
/// assert_eq!(reader.read_event().unwrap(), Event::GeneralRef(event.borrow()));
/// // deref coercion of &BytesRef to &[u8]
/// assert_eq!(&event as &[u8], content.as_bytes());
/// // AsRef<[u8]> for &T + deref coercion
/// assert_eq!(event.as_ref(), content.as_bytes());
/// ```
#[derive(Clone, Eq, PartialEq)]
pub struct BytesRef<'a> {
    content: Cow<'a, [u8]>,
    /// Encoding in which the `content` is stored inside the event.
    decoder: Decoder,
}

impl<'a> BytesRef<'a> {
    /// Internal constructor, used by `Reader`. Supplies data in reader's encoding
    #[inline]
    pub(crate) const fn wrap(content: &'a [u8], decoder: Decoder) -> Self {
        Self {
            content: Cow::Borrowed(content),
            decoder,
        }
    }

    /// Creates a new `BytesRef` borrowing a slice.
    ///
    /// # Warning
    ///
    /// `name` must be a valid name.
    #[inline]
    pub fn new<C: Into<Cow<'a, str>>>(name: C) -> Self {
        Self {
            content: str_cow_to_bytes(name),
            decoder: Decoder::utf8(),
        }
    }

    /// Converts the event into an owned event.
    pub fn into_owned(self) -> BytesRef<'static> {
        BytesRef {
            content: Cow::Owned(self.content.into_owned()),
            decoder: self.decoder,
        }
    }

    /// Extracts the inner `Cow` from the `BytesRef` event container.
    #[inline]
    pub fn into_inner(self) -> Cow<'a, [u8]> {
        self.content
    }

    /// Converts the event into a borrowed event.
    #[inline]
    pub fn borrow(&self) -> BytesRef {
        BytesRef {
            content: Cow::Borrowed(&self.content),
            decoder: self.decoder,
        }
    }

    /// Decodes the content of the event.
    ///
    /// This will allocate if the value contains any escape sequences or in
    /// non-UTF-8 encoding.
    pub fn decode(&self) -> Result<Cow<'a, str>, EncodingError> {
        self.decoder.decode_cow(&self.content)
    }

    /// Returns `true` if the specified reference represents the character reference
    /// (`&#<number>;`).
    ///
    /// ```
    /// # use quick_xml::events::BytesRef;
    /// # use pretty_assertions::assert_eq;
    /// assert_eq!(BytesRef::new("#x30").is_char_ref(), true);
    /// assert_eq!(BytesRef::new("#49" ).is_char_ref(), true);
    /// assert_eq!(BytesRef::new("lt"  ).is_char_ref(), false);
    /// ```
    pub fn is_char_ref(&self) -> bool {
        matches!(self.content.first(), Some(b'#'))
    }

    /// If this reference represents character reference, then resolves it and
    /// returns the character, otherwise returns `None`.
    ///
    /// This method does not check if character is allowed for XML, in other words,
    /// well-formedness constraint [WFC: Legal Char] is not enforced.
    /// The character `0x0`, however, will return `EscapeError::InvalidCharRef`.
    ///
    /// ```
    /// # use quick_xml::events::BytesRef;
    /// # use pretty_assertions::assert_eq;
    /// assert_eq!(BytesRef::new("#x30").resolve_char_ref().unwrap(), Some('0'));
    /// assert_eq!(BytesRef::new("#49" ).resolve_char_ref().unwrap(), Some('1'));
    /// assert_eq!(BytesRef::new("lt"  ).resolve_char_ref().unwrap(), None);
    /// ```
    ///
    /// [WFC: Legal Char]: https://www.w3.org/TR/xml11/#wf-Legalchar
    pub fn resolve_char_ref(&self) -> Result<Option<char>, Error> {
        if let Some(num) = self.decode()?.strip_prefix('#') {
            let ch = parse_number(num).map_err(EscapeError::InvalidCharRef)?;
            return Ok(Some(ch));
        }
        Ok(None)
    }
}

impl<'a> Debug for BytesRef<'a> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "BytesRef {{ content: ")?;
        write_cow_string(f, &self.content)?;
        write!(f, " }}")
    }
}

impl<'a> Deref for BytesRef<'a> {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.content
    }
}

#[cfg(feature = "arbitrary")]
impl<'a> arbitrary::Arbitrary<'a> for BytesRef<'a> {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        Ok(Self::new(<&str>::arbitrary(u)?))
    }

    fn size_hint(depth: usize) -> (usize, Option<usize>) {
        <&str as arbitrary::Arbitrary>::size_hint(depth)
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////

/// Event emitted by [`Reader::read_event_into`].
///
/// [`Reader::read_event_into`]: crate::reader::Reader::read_event_into
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub enum Event<'a> {
    /// Start tag (with attributes) `<tag attr="value">`.
    Start(BytesStart<'a>),
    /// End tag `</tag>`.
    End(BytesEnd<'a>),
    /// Empty element tag (with attributes) `<tag attr="value" />`.
    Empty(BytesStart<'a>),
    /// Escaped character data between tags.
    Text(BytesText<'a>),
    /// Unescaped character data stored in `<![CDATA[...]]>`.
    CData(BytesCData<'a>),
    /// Comment `<!-- ... -->`.
    Comment(BytesText<'a>),
    /// XML declaration `<?xml ...?>`.
    Decl(BytesDecl<'a>),
    /// Processing instruction `<?...?>`.
    PI(BytesPI<'a>),
    /// Document type definition data (DTD) stored in `<!DOCTYPE ...>`.
    DocType(BytesText<'a>),
    /// General reference `&entity;` in the textual data. Can be either an entity
    /// reference, or a character reference.
    GeneralRef(BytesRef<'a>),
    /// End of XML document.
    Eof,
}

impl<'a> Event<'a> {
    /// Converts the event to an owned version, untied to the lifetime of
    /// buffer used when reading but incurring a new, separate allocation.
    pub fn into_owned(self) -> Event<'static> {
        match self {
            Event::Start(e) => Event::Start(e.into_owned()),
            Event::End(e) => Event::End(e.into_owned()),
            Event::Empty(e) => Event::Empty(e.into_owned()),
            Event::Text(e) => Event::Text(e.into_owned()),
            Event::Comment(e) => Event::Comment(e.into_owned()),
            Event::CData(e) => Event::CData(e.into_owned()),
            Event::Decl(e) => Event::Decl(e.into_owned()),
            Event::PI(e) => Event::PI(e.into_owned()),
            Event::DocType(e) => Event::DocType(e.into_owned()),
            Event::GeneralRef(e) => Event::GeneralRef(e.into_owned()),
            Event::Eof => Event::Eof,
        }
    }

    /// Converts the event into a borrowed event.
    #[inline]
    pub fn borrow(&self) -> Event {
        match self {
            Event::Start(e) => Event::Start(e.borrow()),
            Event::End(e) => Event::End(e.borrow()),
            Event::Empty(e) => Event::Empty(e.borrow()),
            Event::Text(e) => Event::Text(e.borrow()),
            Event::Comment(e) => Event::Comment(e.borrow()),
            Event::CData(e) => Event::CData(e.borrow()),
            Event::Decl(e) => Event::Decl(e.borrow()),
            Event::PI(e) => Event::PI(e.borrow()),
            Event::DocType(e) => Event::DocType(e.borrow()),
            Event::GeneralRef(e) => Event::GeneralRef(e.borrow()),
            Event::Eof => Event::Eof,
        }
    }

    /// Well-formedness constraints
    /// ===========================
    ///
    /// [WFC: External Subset]
    /// ----------------------
    /// The external subset, if any, MUST match the production for extSubset.
    ///
    /// [WFC: PE Between Declarations]
    /// ------------------------------
    /// The replacement text of a parameter entity reference in a `DeclSep` MUST
    /// match the production `extSubsetDecl`.
    ///
    /// [WFC: PEs in Internal Subset]
    /// -----------------------------
    /// In the internal DTD subset, parameter-entity references MUST NOT occur
    /// within markup declarations; they may occur where markup declarations
    /// can occur. (This does not apply to references that occur in external
    /// parameter entities or to the external subset.)
    ///
    /// [WFC: Element Type Match]
    /// -------------------------
    /// The Name in an element's end-tag MUST match the element type in the start-tag.
    ///
    /// [WFC: Unique Att Spec]
    /// ----------------------
    /// An attribute name MUST NOT appear more than once in the same start-tag
    /// or empty-element tag.
    ///
    /// [WFC: No External Entity References]
    /// ------------------------------------
    /// Attribute values MUST NOT contain direct or indirect entity references
    /// to external entities.
    ///
    /// [WFC: No < in Attribute Values]
    /// -------------------------------
    /// The [replacement text] of any entity referred to directly or indirectly
    /// in an attribute value MUST NOT contain a `<`.
    ///
    /// [WFC: Legal Character]
    /// ----------------------
    /// Characters referred to using character references MUST match the production for `Char`.
    ///
    /// [WFC: Entity Declared]
    /// ----------------------
    /// In a document without any DTD, a document with only an internal DTD subset
    /// which contains no parameter entity references, or a document with `standalone='yes'`,
    /// for an entity reference that does not occur within the external subset
    /// or a parameter entity, the `Name` given in the entity reference MUST
    /// match that in an entity declaration that does not occur within the
    /// external subset or a parameter entity, except that well-formed documents
    /// need not declare any of the following entities: `amp`, `lt`, `gt`, `apos`,
    /// `quot`. The declaration of a general entity MUST precede any reference
    /// to it which appears in a default value in an attribute-list declaration.
    ///
    /// Note that non-validating processors are not obligated to to read and
    /// process entity declarations occurring in parameter entities or in the
    /// external subset; for such documents, the rule that an entity must be
    /// declared is a well-formedness constraint only if `standalone='yes'`.
    ///
    /// [WFC: Parsed Entity]
    /// --------------------
    /// An entity reference MUST NOT contain the name of an unparsed entity.
    /// Unparsed entities may be referred to only in attribute values declared
    /// to be of type ENTITY or ENTITIES.
    ///
    /// [WFC: No Recursion]
    /// -------------------
    /// A parsed entity MUST NOT contain a recursive reference to itself, either
    /// directly or indirectly.
    ///
    /// [WFC: In DTD]
    /// -------------
    /// Parameter-entity references MUST NOT appear outside the DTD.
    ///
    ///
    ///
    /// Validity constraints
    /// ====================
    ///
    /// [VC: Element Valid]
    /// -------------------
    /// An element is valid if there is a declaration matching `elementdecl`
    /// where the Name matches the element type, and one of the following holds:
    /// 1. The declaration matches `EMPTY` and the element has no content
    ///    (not even entity references, comments, PIs or white space).
    /// 2. The declaration matches children and the sequence of child elements
    ///    belongs to the language generated by the regular expression in the
    ///    content model, with optional white space, comments and PIs (i.e.
    ///    markup matching production [27] Misc) between the start-tag and the
    ///    first child element, between child elements, or between the last child
    ///    element and the end-tag. Note that a CDATA section containing only
    ///    white space or a reference to an entity whose replacement text is
    ///    character references expanding to white space do not match the
    ///    nonterminal `S`, and hence cannot appear in these positions;
    ///    however, a reference to an internal entity with a literal value
    ///    consisting of character references expanding to white space does
    ///    match `S`, since its replacement text is the white space resulting
    ///    from expansion of the character references.
    /// 3. The declaration matches `Mixed`, and the content (after replacing
    ///    any entity references with their replacement text) consists of
    ///    character data (including CDATA sections), comments, PIs and child
    ///    elements whose types match names in the content model.
    /// 4. The declaration matches `ANY`, and the content (after replacing any
    ///    entity references with their replacement text) consists of character
    ///    data, CDATA sections, comments, PIs and child elements whose types
    ///    have been declared.
    ///
    /// [VC: Root Element Type]
    /// -----------------------
    /// The `Name` in the document type declaration MUST match the element type
    /// of the root element.
    ///
    /// [VC: Proper Declaration/PE Nesting]
    /// -----------------------------------
    /// Parameter-entity replacement text MUST be properly nested with markup
    /// declarations. That is to say, if either the first character or the last
    /// character of a markup declaration (`markupdecl` above) is contained in
    /// the replacement text for a parameter-entity reference, both MUST be
    /// contained in the same replacement text.
    ///
    /// [VC: Standalone Document Declaration]
    /// -------------------------------------
    /// The standalone document declaration MUST have the value `no` if any
    /// external markup declarations contain declarations of:
    /// - attributes with default values, if elements to which these attributes
    ///   apply appear in the document without specifications of values for these
    ///   attributes, or
    /// - entities (other than `amp`, `lt`, `gt`, `apos`, `quot`), if references
    ///   to those entities appear in the document, or
    /// - attributes with tokenized types, where the attribute appears in the
    ///   document with a value such that normalization will produce a different
    ///   value from that which would be produced in the absence of the declaration, or
    /// - element types with element content, if white space occurs directly
    ///   within any instance of those types.
    ///
    /// [VC: Attribute Value Type]
    /// --------------------------
    /// The attribute MUST have been declared; the value MUST be of the type
    /// declared for it. (For attribute types, see 3.3 Attribute-List Declarations.)
    ///
    /// [VC: Unique Element Type Declaration]
    /// -------------------------------------
    /// An element type MUST NOT be declared more than once.
    ///
    /// [VC: Proper Group/PE Nesting]
    /// -----------------------------
    /// Parameter-entity replacement text MUST be properly nested with parenthesized
    /// groups. That is to say, if either of the opening or closing parentheses
    /// in a choice, seq, or Mixed construct is contained in the replacement text
    /// for a parameter entity, both MUST be contained in the same replacement text.
    ///
    /// For interoperability, if a parameter-entity reference appears in a `choice`,
    /// `seq`, or `Mixed` construct, its replacement text SHOULD contain at least
    /// one non-blank character, and neither the first nor last non-blank character
    /// of the replacement text SHOULD be a connector (`|` or `,`).
    ///
    /// [VC: No Duplicate Types]
    /// ------------------------
    /// The same name MUST NOT appear more than once in a single mixed-content declaration.
    ///
    /// [VC: ID]
    /// --------
    /// Values of type ID MUST match the Name production. A name MUST NOT appear
    /// more than once in an XML document as a value of this type; i.e., ID values
    /// MUST uniquely identify the elements which bear them.
    ///
    /// [VC: One ID per Element Type]
    /// -----------------------------
    /// An element type MUST NOT have more than one ID attribute specified.
    ///
    /// [VC: ID Attribute Default]
    /// --------------------------
    /// An element type MUST NOT have more than one ID attribute specified.
    ///
    /// [VC: IDREF]
    /// -----------
    /// Values of type IDREF MUST match the Name production, and values of type
    /// IDREFS MUST match Names; each Name MUST match the value of an ID attribute
    /// on some element in the XML document; i.e. IDREF values MUST match the
    /// value of some ID attribute.
    ///
    /// [VC: Entity Name]
    /// -----------------
    /// Values of type ENTITY MUST match the Name production, values of type
    /// ENTITIES MUST match Names; each Name MUST match the name of an unparsed
    /// entity declared in the DTD.
    ///
    /// [VC: Name Token]
    /// ----------------
    /// Values of type NMTOKEN MUST match the Nmtoken production; values of type
    /// NMTOKENS MUST match Nmtokens.
    ///
    /// [VC: Notation Attributes]
    /// -------------------------
    /// Values of this type MUST match one of the notation names included in
    /// the declaration; all notation names in the declaration MUST be declared.
    ///
    /// [VC: One Notation Per Element Type]
    /// -----------------------------------
    /// An element type MUST NOT have more than one NOTATION attribute specified.
    ///
    /// [VC: No Notation on Empty Element]
    /// ----------------------------------
    /// For compatibility, an attribute of type NOTATION MUST NOT be declared
    /// on an element declared EMPTY.
    ///
    /// [VC: No Duplicate Tokens]
    /// -------------------------
    /// The notation names in a single NotationType attribute declaration,
    /// as well as the NmTokens in a single Enumeration attribute declaration,
    /// MUST all be distinct.
    ///
    /// [VC: Enumeration]
    /// -----------------
    /// Values of this type MUST match one of the Nmtoken tokens in the declaration.
    ///
    /// [VC: Required Attribute]
    /// ------------------------
    /// If the default declaration is the keyword #REQUIRED, then the attribute
    /// MUST be specified for all elements of the type in the attribute-list declaration.
    ///
    /// [VC: Attribute Default Value Syntactically Correct]
    /// ---------------------------------------------------
    /// The declared default value MUST meet the syntactic constraints of the
    /// declared attribute type. That is, the default value of an attribute:
    /// - of type IDREF or ENTITY must match the Name production;
    /// - of type IDREFS or ENTITIES must match the Names production;
    /// - of type NMTOKEN must match the Nmtoken production;
    /// - of type NMTOKENS must match the Nmtokens production;
    /// - of an enumerated type (either a NOTATION type or an enumeration) must
    ///   match one of the enumerated values.
    ///
    /// Note that only the syntactic constraints of the type are required here;
    /// other constraints (e.g. that the value be the name of a declared unparsed
    /// entity, for an attribute of type ENTITY) will be reported by a validating
    /// parser only if an element without a specification for this attribute actually occurs.
    ///
    /// [VC: Fixed Attribute Default]
    /// -----------------------------
    /// If an attribute has a default value declared with the #FIXED keyword,
    /// instances of that attribute MUST match the default value.
    ///
    /// [VC: Proper Conditional Section/PE Nesting]
    /// -------------------------------------------
    /// If any of the `<![`, `[`, or `]]>` of a conditional section is contained
    /// in the replacement text for a parameter-entity reference, all of them
    /// MUST be contained in the same replacement text.
    ///
    /// [VC: Entity Declared]
    /// ---------------------
    /// In a document with an external subset or parameter entity references with
    /// `standalone='no'`, the `Name` given in the entity reference MUST match
    /// that in an entity declaration. For interoperability, valid documents
    /// SHOULD declare the entities `amp`, `lt`, `gt`, `apos`, `quot`, in the
    /// form specified in 4.6 Predefined Entities. The declaration of a parameter
    /// entity MUST precede any reference to it. Similarly, the declaration of
    /// a general entity MUST precede any attribute-list declaration containing
    /// a default value with a direct or indirect reference to that general entity.
    ///
    /// [VC: Notation Declared]
    /// -----------------------
    /// The `Name` MUST match the declared name of a notation.
    ///
    /// [VC: Unique Notation Name]
    /// --------------------------
    /// A given `Name` MUST NOT be declared in more than one notation declaration.
    ///
    ///
    ///
    /// [WFC: External Subset]: https://www.w3.org/TR/xml11/#ExtSubset
    /// [WFC: PE Between Declarations]: https://www.w3.org/TR/xml11/#PE-between-Decls
    /// [WFC: PEs in Internal Subset]: https://www.w3.org/TR/xml11/#wfc-PEinInternalSubset
    /// [WFC: Element Type Match]: https://www.w3.org/TR/xml11/#GIMatch
    /// [WFC: Unique Att Spec]: https://www.w3.org/TR/xml11/#uniqattspec
    /// [WFC: No External Entity References]: https://www.w3.org/TR/xml11/#NoExternalRefs
    /// [WFC: No < in Attribute Values]: https://www.w3.org/TR/xml11/#CleanAttrVals
    /// [WFC: Legal Character]: https://www.w3.org/TR/xml11/#wf-Legalchar
    /// [WFC: Entity Declared]: https://www.w3.org/TR/xml11/#wf-entdeclared
    /// [WFC: Parsed Entity]: https://www.w3.org/TR/xml11/#textent
    /// [WFC: No Recursion]: https://www.w3.org/TR/xml11/#norecursion
    /// [WFC: In DTD]: https://www.w3.org/TR/xml11/#indtd
    /// [VC: Element Valid]: https://www.w3.org/TR/xml11/#elementvalid
    /// [VC: Root Element Type]: https://www.w3.org/TR/xml11/#vc-roottype
    /// [VC: Proper Declaration/PE Nesting]: https://www.w3.org/TR/xml11/#vc-PEinMarkupDecl
    /// [VC: Standalone Document Declaration]: https://www.w3.org/TR/xml11/#vc-check-rmd
    /// [VC: Attribute Value Type]: https://www.w3.org/TR/xml11/#ValueType
    /// [VC: Unique Element Type Declaration]: https://www.w3.org/TR/xml11/#EDUnique
    /// [VC: Proper Group/PE Nesting]: https://www.w3.org/TR/xml11/#vc-PEinGroup
    /// [VC: No Duplicate Types]: https://www.w3.org/TR/xml11/#vc-MixedChildrenUnique
    /// [VC: ID]: https://www.w3.org/TR/xml11/#id
    /// [VC: One ID per Element Type]: https://www.w3.org/TR/xml11/#one-id-per-el
    /// [VC: ID Attribute Default]: https://www.w3.org/TR/xml11/#id-default
    /// [VC: IDREF]: https://www.w3.org/TR/xml11/#idref
    /// [VC: Entity Name]: https://www.w3.org/TR/xml11/#entname
    /// [VC: Name Token]: https://www.w3.org/TR/xml11/#nmtok
    /// [VC: Notation Attributes]: https://www.w3.org/TR/xml11/#notatn
    /// [VC: One Notation Per Element Type]: https://www.w3.org/TR/xml11/#OneNotationPer
    /// [VC: No Notation on Empty Element]: https://www.w3.org/TR/xml11/#NoNotationEmpty
    /// [VC: No Duplicate Tokens]: https://www.w3.org/TR/xml11/#NoDuplicateTokens
    /// [VC: Enumeration]: https://www.w3.org/TR/xml11/#enum
    /// [VC: Required Attribute]: https://www.w3.org/TR/xml11/#RequiredAttr
    /// [VC: Attribute Default Value Syntactically Correct]: https://www.w3.org/TR/xml11/#defattrvalid
    /// [VC: Fixed Attribute Default]: https://www.w3.org/TR/xml11/#FixedAttr
    /// [VC: Proper Conditional Section/PE Nesting]: https://www.w3.org/TR/xml11/#condsec-nesting
    /// [VC: Entity Declared]: https://www.w3.org/TR/xml11/#vc-entdeclared
    /// [VC: Notation Declared]: https://www.w3.org/TR/xml11/#not-declared
    /// [VC: Unique Notation Name]: https://www.w3.org/TR/xml11/#UniqueNotationName
    /// [replacement text]: https://www.w3.org/TR/xml11/#dt-repltext
    fn validate(&self) {
        todo!()
    }
}

impl<'a> Deref for Event<'a> {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        match *self {
            Event::Start(ref e) | Event::Empty(ref e) => e,
            Event::End(ref e) => e,
            Event::Text(ref e) => e,
            Event::Decl(ref e) => e,
            Event::PI(ref e) => e,
            Event::CData(ref e) => e,
            Event::Comment(ref e) => e,
            Event::DocType(ref e) => e,
            Event::GeneralRef(ref e) => e,
            Event::Eof => &[],
        }
    }
}

impl<'a> AsRef<Event<'a>> for Event<'a> {
    fn as_ref(&self) -> &Event<'a> {
        self
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////

#[inline]
fn str_cow_to_bytes<'a, C: Into<Cow<'a, str>>>(content: C) -> Cow<'a, [u8]> {
    match content.into() {
        Cow::Borrowed(s) => Cow::Borrowed(s.as_bytes()),
        Cow::Owned(s) => Cow::Owned(s.into_bytes()),
    }
}

fn trim_cow<'a, F>(value: Cow<'a, [u8]>, trim: F) -> Cow<'a, [u8]>
where
    F: FnOnce(&[u8]) -> &[u8],
{
    match value {
        Cow::Borrowed(bytes) => Cow::Borrowed(trim(bytes)),
        Cow::Owned(mut bytes) => {
            let trimmed = trim(&bytes);
            if trimmed.len() != bytes.len() {
                bytes = trimmed.to_vec();
            }
            Cow::Owned(bytes)
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn bytestart_create() {
        let b = BytesStart::new("test");
        assert_eq!(b.len(), 4);
        assert_eq!(b.name(), QName(b"test"));
    }

    #[test]
    fn bytestart_set_name() {
        let mut b = BytesStart::new("test");
        assert_eq!(b.len(), 4);
        assert_eq!(b.name(), QName(b"test"));
        assert_eq!(b.attributes_raw(), b"");
        b.push_attribute(("x", "a"));
        assert_eq!(b.len(), 10);
        assert_eq!(b.attributes_raw(), b" x=\"a\"");
        b.set_name(b"g");
        assert_eq!(b.len(), 7);
        assert_eq!(b.name(), QName(b"g"));
    }

    #[test]
    fn bytestart_clear_attributes() {
        let mut b = BytesStart::new("test");
        b.push_attribute(("x", "y\"z"));
        b.push_attribute(("x", "y\"z"));
        b.clear_attributes();
        assert!(b.attributes().next().is_none());
        assert_eq!(b.len(), 4);
        assert_eq!(b.name(), QName(b"test"));
    }
}
