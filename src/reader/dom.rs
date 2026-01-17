//! Exports an [`Element`] struct which represents a DOM tree and a [`DomBuilder`]
//! used to create new DOMs from events.

use crate::encoding::EncodingError;
use crate::errors::Error;
use crate::events::{BytesCData, BytesEnd, BytesStart, BytesText, Event};
use crate::reader::Reader;
use std::borrow::Cow;
use std::collections::VecDeque;
use std::fmt;
use std::io::BufRead;

/// Skips comments, processing instructions and spaces ("Misc" production from XML grammar)
fn skip_misc<'i>(reader: &mut Reader<&'i [u8]>, mut event: Event<'i>) -> Result<Event<'i>, Error> {
    loop {
        match event {
            Event::Comment(_) | Event::PI(_) => {
                event = reader.read_event()?;
                continue;
            }
            Event::Text(e) if e.is_xml_spaces() => {
                event = reader.read_event()?;
                continue;
            }
            _ => {}
        }
        break Ok(event);
    }
}

fn skip_misc_into<'b, R>(
    reader: &mut Reader<R>,
    buf: &'b mut Vec<u8>,
    mut event: Event<'b>,
) -> Result<Event<'b>, Error>
where
    R: BufRead,
{
    loop {
        match event {
            Event::Comment(_) | Event::PI(_) => {
                buf.clear();
                event = reader.read_event_into(buf)?.into_owned();
                continue;
            }
            Event::Text(e) if e.is_xml_spaces() => {
                buf.clear();
                event = reader.read_event_into(buf)?.into_owned();
                continue;
            }
            _ => {}
        }
        break Ok(event);
    }
}

/// A struct representing a DOM Element.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Element<'i> {
    /// The tag name and the attributes of a node
    pub tag: BytesStart<'i>,
    /// Nodes inside this element
    pub children: VecDeque<Node<'i>>,
}
impl<'i> Element<'i> {
    /// Parses specified XML string returning result as a single DOM Element.
    /// Only one root element is allowed.
    ///
    /// # Examples
    ///
    /// ```
    /// # use pretty_assertions::assert_eq;
    /// # use quick_xml::events::BytesStart;
    /// # use quick_xml::reader::dom::{Element, Node};
    /// #
    /// let element = Element::from_str("<tag>text</tag>").unwrap();
    /// assert_eq!(element.tag, BytesStart::new("tag"));
    /// assert_eq!(element.children, [Node::Text("text".into())]);
    /// ```
    ///
    /// Only one root element is allowed:
    /// ```
    /// # use quick_xml::reader::dom::{DomError, Element};
    /// #
    /// let error = Element::from_str("<one/><two/>").unwrap_err();
    /// assert!(matches!(error, DomError::MultipleRoots));
    /// ```
    ///
    /// Text instead of element produces error:
    /// ```
    /// # use quick_xml::reader::dom::{DomError, Element};
    /// #
    /// let error = Element::from_str("text").unwrap_err();
    /// assert!(matches!(error, DomError::MissingRoot));
    ///
    /// let error = Element::from_str("").unwrap_err();
    /// assert!(matches!(error, DomError::MissingRoot));
    /// ```
    pub fn from_str(xml: &'i str) -> Result<Self, DomError> {
        let mut reader = Reader::from_str(xml);
        let mut builder = DomBuilder::default();
        let mut root = None;

        // Read optional XML declaration ("XmlDecl" production from XML grammar)
        let event = match reader.read_event()? {
            Event::Decl(_) => reader.read_event()?,
            e => e,
        };
        // Skip comments, processing instructions and spaces
        let event = skip_misc(&mut reader, event)?;
        // Skip DTD and following comments, processing instructions and spaces
        let mut event = match event {
            Event::DocType(_) => {
                let event = reader.read_event()?;
                skip_misc(&mut reader, event)?
            }
            e => e,
        };
        loop {
            match builder.feed(event)? {
                FeedResult::NeedData => {
                    event = reader.read_event()?;
                    continue;
                }
                FeedResult::NoData | FeedResult::Space(_, _) => match root {
                    Some(element) => return Ok(element),
                    None => return Err(DomError::MissingRoot),
                },
                FeedResult::Element(fragment) => match root {
                    Some(_) => return Err(DomError::MultipleRoots),
                    None => root = Some(fragment),
                },
                FeedResult::Text(_, _) => return Err(DomError::MissingRoot),
            }
            event = reader.read_event()?;
        }
    }

    /// Parses specified XML string returning result as a single DOM Element.
    /// Only one root element is allowed.
    ///
    /// # Examples
    ///
    /// ```
    /// # use pretty_assertions::assert_eq;
    /// # use quick_xml::events::BytesStart;
    /// # use quick_xml::reader::dom::{Element, Node};
    /// #
    /// let element = Element::from_reader("<tag>text</tag>".as_bytes()).unwrap();
    /// assert_eq!(element.tag, BytesStart::new("tag"));
    /// assert_eq!(element.children, [Node::Text("text".into())]);
    /// ```
    ///
    /// Only one root element is allowed:
    /// ```
    /// # use quick_xml::reader::dom::{DomError, Element};
    /// #
    /// let error = Element::from_reader("<one/><two/>".as_bytes()).unwrap_err();
    /// assert!(matches!(error, DomError::MultipleRoots));
    /// ```
    ///
    /// Text instead of element produces error:
    /// ```
    /// # use quick_xml::reader::dom::{DomError, Element};
    /// #
    /// let error = Element::from_reader("text".as_bytes()).unwrap_err();
    /// assert!(matches!(error, DomError::MissingRoot));
    ///
    /// let error = Element::from_reader("".as_bytes()).unwrap_err();
    /// assert!(matches!(error, DomError::MissingRoot));
    /// ```
    pub fn from_reader<R: BufRead>(reader: R) -> Result<Element<'static>, DomError> {
        let mut reader = Reader::from_reader(reader);
        let mut builder = DomBuilder::default();
        let mut root = None::<Element<'static>>;

        let mut buf = Vec::new();

        // Read optional XML declaration ("XmlDecl" production from XML grammar)
        let event = match reader.read_event_into(&mut buf)?.into_owned() {
            Event::Decl(_) => reader.read_event_into(&mut buf)?.into_owned(),
            e => e,
        };
        // Skip comments, processing instructions and spaces
        let event = skip_misc_into(&mut reader, &mut buf, event)?.into_owned();
        // Skip DTD and following comments, processing instructions and spaces
        let mut event = match event.into_owned() {
            Event::DocType(_) => {
                let event = reader.read_event_into(&mut buf)?.into_owned();
                skip_misc_into(&mut reader, &mut buf, event)?.into_owned()
            }
            e => e,
        };

        loop {
            buf.clear();
            match builder.feed(event)? {
                FeedResult::NeedData => {
                    event = reader.read_event_into(&mut buf)?.into_owned();
                    continue;
                }
                FeedResult::NoData | FeedResult::Space(_, _) => match root {
                    Some(element) => return Ok(element),
                    None => return Err(DomError::MissingRoot),
                },
                FeedResult::Element(fragment) => match root {
                    Some(_) => return Err(DomError::MultipleRoots),
                    None => root = Some(fragment),
                },
                FeedResult::Text(_, _) => return Err(DomError::MissingRoot),
            }
            event = reader.read_event_into(&mut buf)?.into_owned();
        }
    }

    /// Ensures that all data is owned to extend the object's lifetime if necessary.
    pub fn into_owned(self) -> Element<'static> {
        Element {
            tag: self.tag.into_owned(),
            children: self.children.into_iter().map(Node::into_owned).collect(),
        }
    }
}

impl<'i> From<BytesStart<'i>> for Element<'i> {
    #[inline]
    fn from(tag: BytesStart<'i>) -> Self {
        Self {
            tag,
            children: VecDeque::new(),
        }
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////

/// A node in an element tree.
#[derive(Clone, Eq, PartialEq)]
pub enum Node<'i> {
    /// An `Element`.
    Element(Element<'i>),
    /// A text node, borrowing from input if possible. Note, that leading/trailing
    /// spaces won't be trimmed.
    Text(Cow<'i, str>),
    /// A text node which formed exclusively by [whitespace characters] literally
    /// presented in the document. Note, that according to the XML specification,
    /// parser operates over normalized text, so this string may contain only
    /// ` `, `\n` and `\t` characters, but original text may contain additional
    /// characters.
    ///
    /// [whitespace characters]: https://www.w3.org/TR/xml11/#NT-S
    Space(Cow<'i, str>),
}
impl<'i> Node<'i> {
    /// Parses specified XML string returning result as a single DOM Element.
    /// Only one root element is allowed.
    ///
    /// # Examples
    ///
    /// ```
    /// # use pretty_assertions::assert_eq;
    /// # use std::borrow::Cow;
    /// # use quick_xml::reader::dom::{DomError, Node};
    /// #
    /// let node = Node::from_str("<tag>text</tag>").unwrap();
    /// assert!(matches!(node, Node::Element(_)));
    ///
    /// let node = Node::from_str("text").unwrap();
    /// assert!(matches!(node, Node::Text(Cow::Borrowed("text"))));
    ///
    /// let node = Node::from_str("").unwrap();
    /// assert!(matches!(node, Node::Text(Cow::Borrowed(""))));
    /// ```
    ///
    /// Only one root element is allowed:
    /// ```
    /// # use quick_xml::reader::dom::{DomError, Node};
    /// #
    /// let error = Node::from_str("<one/><two/>").unwrap_err();
    /// assert!(matches!(error, DomError::MultipleRoots));
    ///
    /// let error = Node::from_str("text<element/>").unwrap_err();
    /// assert!(matches!(error, DomError::MultipleRoots));
    ///
    /// let error = Node::from_str("<element/>text").unwrap_err();
    /// assert!(matches!(error, DomError::MultipleRoots));
    /// ```
    pub fn from_str(xml: &'i str) -> Result<Self, DomError> {
        let mut reader = Reader::from_str(xml);
        let mut builder = DomBuilder::default();
        let mut root = None;

        loop {
            let event = reader.read_event()?;
            match builder.feed(event)? {
                FeedResult::NeedData => continue,
                FeedResult::NoData | FeedResult::Space(_, _) => {
                    return match root {
                        Some(element) => Ok(Node::Element(element)),
                        None => Ok(Node::Text(Cow::Borrowed(""))),
                    };
                }
                FeedResult::Element(fragment) => match root {
                    Some(_) => return Err(DomError::MultipleRoots),
                    None => root = Some(fragment),
                },
                FeedResult::Text(text, Event::Eof) => {
                    return match root {
                        Some(_) => Err(DomError::MultipleRoots),
                        None => Ok(Node::Text(text)),
                    };
                }
                FeedResult::Text(_, _) => return Err(DomError::MultipleRoots),
            }
        }
    }

    /// Parses specified XML string returning result as a single DOM Element.
    /// Only one root element is allowed.
    ///
    /// # Examples
    ///
    /// ```
    /// # use pretty_assertions::assert_eq;
    /// # use std::borrow::Cow;
    /// # use quick_xml::reader::dom::{DomError, Node};
    /// #
    /// let node = Node::from_reader("<tag>text</tag>".as_bytes()).unwrap();
    /// assert!(matches!(node, Node::Element(_)));
    ///
    /// let node = Node::from_reader("text".as_bytes()).unwrap();
    /// let text = String::from("text");
    /// assert!(matches!(node, Node::Text(Cow::Owned(text))));
    ///
    /// let node = Node::from_reader("".as_bytes()).unwrap();
    /// assert!(matches!(node, Node::Text(Cow::Borrowed(""))));
    /// ```
    ///
    /// Only one root element is allowed:
    /// ```
    /// # use quick_xml::reader::dom::{DomError, Node};
    /// #
    /// let error = Node::from_reader("<one/><two/>".as_bytes()).unwrap_err();
    /// assert!(matches!(error, DomError::MultipleRoots));
    ///
    /// let error = Node::from_reader("text<element/>".as_bytes()).unwrap_err();
    /// assert!(matches!(error, DomError::MultipleRoots));
    ///
    /// let error = Node::from_reader("<element/>text".as_bytes()).unwrap_err();
    /// assert!(matches!(error, DomError::MultipleRoots));
    /// ```
    pub fn from_reader<R: BufRead>(reader: R) -> Result<Node<'static>, DomError> {
        let mut reader = Reader::from_reader(reader);
        let mut builder = DomBuilder::default();
        let mut root = None::<Element<'static>>;

        let mut buf = Vec::new();
        loop {
            let event = reader.read_event_into(&mut buf)?.into_owned();
            buf.clear();
            match builder.feed(event)? {
                FeedResult::NeedData => continue,
                FeedResult::NoData | FeedResult::Space(_, _) => {
                    return match root {
                        Some(element) => Ok(Node::Element(element)),
                        None => Ok(Node::Text(Cow::Borrowed(""))),
                    };
                }
                FeedResult::Element(fragment) => match root {
                    Some(_) => return Err(DomError::MultipleRoots),
                    None => root = Some(fragment),
                },
                FeedResult::Text(text, Event::Eof) => {
                    return match root {
                        Some(_) => Err(DomError::MultipleRoots),
                        None => Ok(Node::Text(text)),
                    };
                }
                FeedResult::Text(_, _) => return Err(DomError::MultipleRoots),
            }
        }
    }

    /// Ensures that all data is owned to extend the object's lifetime if necessary.
    pub fn into_owned(self) -> Node<'static> {
        match self {
            Self::Element(e) => Node::Element(e.into_owned()),
            Self::Text(e) => Node::Text(e.into_owned().into()),
            Self::Space(e) => Node::Space(e.into_owned().into()),
        }
    }
}

impl<'i> fmt::Debug for Node<'i> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // to avoid render as Element(Element { ... })
            Self::Element(e) => e.fmt(f),
            Self::Text(text) => f.debug_tuple("Text").field(text).finish(),
            Self::Space(text) => f.debug_tuple("Space").field(text).finish(),
        }
    }
}

impl<'i, E> From<E> for Node<'i>
where
    E: Into<Element<'i>>,
{
    fn from(element: E) -> Self {
        Self::Element(element.into())
    }
}

impl<'i> From<Cow<'i, str>> for Node<'i> {
    #[inline]
    fn from(text: Cow<'i, str>) -> Self {
        Self::Text(text)
    }
}

impl<'i> From<String> for Node<'i> {
    #[inline]
    fn from(text: String) -> Self {
        Self::Text(Cow::Owned(text))
    }
}

impl<'i> From<&'i str> for Node<'i> {
    #[inline]
    fn from(text: &'i str) -> Self {
        Self::Text(Cow::Borrowed(text))
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////

/// An error raised during parsing DOM.
#[derive(Clone, Debug)]
pub enum DomError {
    /// Low-level parse error, which includes format violations, mismatched tags,
    /// and encoding issues
    Parse(Error),
    /// The XML Document contains multiple root elements. According to the XML
    /// specification, the document should contain exactly one top-level element
    MultipleRoots,
    /// The XML Document does not contain root element. According to the XML
    /// specification, the document should contain exactly one top-level element
    MissingRoot,
}

impl fmt::Display for DomError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "parse error: {}", e),
            Self::MultipleRoots => write!(f, "multiple root elements"),
            Self::MissingRoot => write!(f, "missing root element"),
        }
    }
}

impl std::error::Error for DomError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Parse(e) => Some(e),
            _ => None,
        }
    }
}

impl From<EncodingError> for DomError {
    #[inline]
    fn from(err: EncodingError) -> Self {
        Self::Parse(Error::Encoding(err))
    }
}

impl From<Error> for DomError {
    #[inline]
    fn from(err: Error) -> Self {
        Self::Parse(err)
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////

/// Temporary storage for unprocessed event
enum Unprocessed<'i> {
    Empty(BytesStart<'i>),
    Start(BytesStart<'i>),
    End(BytesEnd<'i>),
    Text(BytesText<'i>),
    CData(BytesCData<'i>),
}
impl<'i> Unprocessed<'i> {
    #[inline]
    fn into_event(self) -> Event<'i> {
        match self {
            Unprocessed::Empty(e) => Event::Empty(e),
            Unprocessed::Start(e) => Event::Start(e),
            Unprocessed::End(e) => Event::End(e),
            Unprocessed::Text(e) => Event::Text(e),
            Unprocessed::CData(e) => Event::CData(e),
        }
    }
}

/// Result of the [`DomBuilder::feed`] method.
#[derive(Debug)]
pub enum FeedResult<'i> {
    /// Event was consumed, but the XML tree is not complete yet. A new event is
    /// required to make a decision. Call [`DomBuilder::feed`] again.
    NeedData,
    /// An [`Event::Eof`] was supplied, but there is not complete XML tree.
    /// Usually this means premature EOF and corrupted XML structure.
    NoData,
    /// The [`Event::End`] was supplied that finishes DOM element node. The produced
    /// element is returned.
    Element(Element<'i>),
    /// A text node, borrowing from input if possible and an event after last text event.
    Text(Cow<'i, str>, Event<'i>),
    /// A spaces, borrowing from input if possible and an event after last text event.
    Space(Cow<'i, str>, Event<'i>),
}

/// Creates a DOM Element by reading one node from a reader.
///
/// # Example
///
/// ```
/// # use pretty_assertions::assert_eq;
/// use quick_xml::events::{BytesStart, BytesText, Event};
/// use quick_xml::reader::Reader;
/// use quick_xml::reader::dom::{DomBuilder, Element, FeedResult, Node};
///
/// let mut reader = Reader::from_str("\
///     <root>\
///         <field>data</field>\
///         text <![CDATA[merged with CDATA]]> &lt;:)\
///     </root>\
/// ");
///
/// let start = BytesStart::new("root");
/// let end   = start.to_end().into_owned();
///
/// // Read `<root>`
/// assert_eq!(reader.read_event().unwrap(), Event::Start(start));
///
/// let mut builder = DomBuilder::default();
/// let element = loop {
///     let event = reader.read_event().unwrap();
///     match builder.feed(event).unwrap() {
///         FeedResult::NeedData => continue,
///         FeedResult::NoData => panic!("premature EOF"),
///         FeedResult::Element(element) => break element,
///         FeedResult::Text(text, _) => panic!("unexpected text {:?}", text),
///     }
/// };
///
/// // Read `text`
/// let (text, next) = loop {
///     let event = reader.read_event().unwrap();
///     match builder.feed(event).unwrap() {
///         FeedResult::NeedData => continue,
///         FeedResult::NoData => panic!("premature EOF"),
///         FeedResult::Element(e) => panic!("unexpected element {:?}", e),
///         FeedResult::Text(text, next) => break (text, next),
///     }
/// };
/// assert_eq!(text, "text merged with CDATA <:)");
///
/// // Next event `</root>` already read after text nodes
/// assert_eq!(next, Event::End(end));
///
/// // At the end we should get an Eof event, because we ate the whole XML
/// assert_eq!(reader.read_event().unwrap(), Event::Eof);
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct DomBuilder<'i> {
    parents: Vec<Element<'i>>,
    /// Merged consequent text and CDATA events. This events can be splitted with service nodes
    /// (PI and comments)
    text: Option<Cow<'i, str>>,
    spaces_only: bool,
}
impl<'i> DomBuilder<'i> {
    /// Creates a new DOM builder
    pub const fn new() -> Self {
        Self {
            parents: Vec::new(),
            text: None,
            spaces_only: true,
        }
    }
    /// Pushes new event to the builder
    pub fn feed(&mut self, event: Event<'i>) -> Result<FeedResult<'i>, EncodingError> {
        let (unprocessed, create_text_node) = match event {
            Event::Comment(_) | Event::PI(_) => return Ok(FeedResult::NeedData),

            Event::Start(e) => (Unprocessed::Start(e), true),
            Event::End(e) => (Unprocessed::End(e), true),
            Event::Empty(e) => (Unprocessed::Empty(e), true),
            Event::Text(e) if self.spaces_only => {
                self.spaces_only = e.is_xml_spaces();
                (Unprocessed::Text(e), false)
            }
            Event::Text(e) => (Unprocessed::Text(e), false),
            Event::CData(e) => {
                self.spaces_only = false;
                (Unprocessed::CData(e), false)
            }
            Event::GeneralRef(e) => todo!("{:?}", e),
            Event::Decl(e) => todo!("unexpected {:?}", e),
            Event::DocType(e) => todo!("unexpected {:?}", e),

            Event::Eof => {
                let spaces_only = self.spaces_only;
                self.spaces_only = true;
                return match self.text.take() {
                    Some(text) if spaces_only => Ok(FeedResult::Space(text, Event::Eof)),
                    Some(text) => Ok(FeedResult::Text(text, Event::Eof)),
                    None => Ok(FeedResult::NoData),
                };
            }
        };
        // We read tag, so we need to convert all text nodes that we collect before that
        // into a Text node
        if create_text_node {
            let spaces_only = self.spaces_only;
            self.spaces_only = true;
            if let Some(text) = self.text.take() {
                match self.parents.last_mut() {
                    Some(parent) if spaces_only => parent.children.push_back(Node::Space(text)),
                    Some(parent) => parent.children.push_back(Node::Text(text)),

                    None if spaces_only => {
                        return Ok(FeedResult::Space(text, unprocessed.into_event()))
                    }
                    None => return Ok(FeedResult::Text(text, unprocessed.into_event())),
                }
            }
        }
        // Process events after creating pending text node
        match unprocessed {
            Unprocessed::Start(e) => self.parents.push(Element::from(e)),
            // Matching to start event already checked in a parser
            Unprocessed::End(_) => match self.parents.pop() {
                Some(element) => match self.parents.last_mut() {
                    Some(parent) => parent.children.push_back(Node::Element(element)),
                    // Reader is guarantee that nesting is correct, so when parents become empty
                    // we have finished reading the element tree
                    None => {
                        // assume that only spaces would be after the element
                        self.spaces_only = true;
                        return Ok(FeedResult::Element(element));
                    }
                },
                // SAFETY: Reader is guarantee that nesting is correct, and we put to `parents`
                // each time when we read `Event::Start`. That means that when we read `Event::End`,
                // `parents` contains at least one element
                None => {
                    unreachable!("received Event::End which does not match to any Event::Start")
                }
            },
            Unprocessed::Empty(e) => {
                let element = Element::from(e);
                match self.parents.last_mut() {
                    Some(parent) => parent.children.push_back(Node::Element(element)),
                    None => {
                        // assume that only spaces would be after the element
                        self.spaces_only = true;
                        return Ok(FeedResult::Element(element));
                    }
                }
            }
            Unprocessed::Text(e) => self.append_text(e.xml_content()?),
            Unprocessed::CData(e) => self.append_text(e.xml_content()?),
        }
        Ok(FeedResult::NeedData)
    }

    /// Append text to storage or store it to storage
    fn append_text(&mut self, text: Cow<'i, str>) {
        self.text = match self.text.take() {
            None => Some(text),
            Some(prefix) => {
                let mut s = prefix.into_owned();
                s += &text;
                Some(Cow::Owned(s))
            }
        };
    }
}

impl<'i> Default for DomBuilder<'i> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn empty() {
        let element = Element::from_str("<empty/>").unwrap();
        assert_eq!(
            element,
            Element {
                tag: BytesStart::new("empty"),
                children: VecDeque::new(),
            }
        );
    }

    #[test]
    fn simple() {
        let element = Element::from_str("<tag></tag>").unwrap();
        assert_eq!(
            element,
            Element {
                tag: BytesStart::new("tag"),
                children: VecDeque::new(),
            }
        );
    }

    #[test]
    fn text() {
        let element = Element::from_str("<tag>\t\r\n\n\r text\t\r\n\n\r </tag>").unwrap();
        assert_eq!(
            element,
            Element {
                tag: BytesStart::new("tag"),
                children: VecDeque::from([Node::Text("\t\n\n\n text\t\n\n\n ".into())]),
            }
        );
    }

    #[test]
    fn cdata() {
        let element = Element::from_str("<tag><![CDATA[\t\r\n\n\r ]]></tag>").unwrap();
        assert_eq!(
            element,
            Element {
                tag: BytesStart::new("tag"),
                children: VecDeque::from([Node::Text("\t\n\n\n ".into())]),
            }
        );
    }

    mod spaces {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn between_tags() {
            let element = Element::from_str("<tag>\t\r\n\n\r </tag>").unwrap();
            assert_eq!(
                element,
                Element {
                    tag: BytesStart::new("tag"),
                    children: VecDeque::from([Node::Space("\t\n\n\n ".into())]),
                }
            );
        }

        #[test]
        fn indent_empty() {
            let element = Element::from_str("<tag>\t\r\n\n\r <empty/>\t\r\n\n\r </tag>").unwrap();
            assert_eq!(
                element,
                Element {
                    tag: BytesStart::new("tag"),
                    children: VecDeque::from([
                        Node::Space("\t\n\n\n ".into()),
                        Node::Element(Element {
                            tag: BytesStart::new("empty"),
                            children: VecDeque::new(),
                        }),
                        Node::Space("\t\n\n\n ".into()),
                    ]),
                }
            );
        }

        #[test]
        fn indent_tags() {
            let element =
                Element::from_str("<tag>\t\r\n\n\r <tag>content</tag>\t\r\n\n\r </tag>").unwrap();
            assert_eq!(
                element,
                Element {
                    tag: BytesStart::new("tag"),
                    children: VecDeque::from([
                        Node::Space("\t\n\n\n ".into()),
                        Node::Element(Element {
                            tag: BytesStart::new("tag"),
                            children: VecDeque::from([Node::Text("content".into())]),
                        }),
                        Node::Space("\t\n\n\n ".into()),
                    ]),
                }
            );
        }
    }

    /// Checks, that spaces, processing instructions and comments are skipped and
    /// do not considered as roots. They corresponds to the "Misc" production in
    /// XML grammar.
    mod misc {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn empty() {
            let element = Element::from_str(
                "<?xml version='1.0'?><?pi?><!--comment-->\n\r\t <empty/>\n\r\t <?pi?><!--comment-->",
            )
            .unwrap();
            assert_eq!(
                element,
                Element {
                    tag: BytesStart::new("empty"),
                    children: VecDeque::new(),
                }
            );
        }

        #[test]
        fn simple() {
            let element = Element::from_str(
                "<?xml version='1.0'?><?pi?><!--comment-->\n\r\t <root></root>\n\r\t <?pi?><!--comment-->",
            )
            .unwrap();
            assert_eq!(
                element,
                Element {
                    tag: BytesStart::new("root"),
                    children: VecDeque::new(),
                }
            );
        }
    }

    #[test]
    fn nested() {
        let element = Element::from_str(
            "<tag><tag>HTML <i>text</i> <![CDATA[is awesome! <3]]> &lt;3</tag></tag>",
        )
        .unwrap();
        assert_eq!(
            element,
            Element {
                tag: BytesStart::new("tag"),
                children: VecDeque::from([Node::Element(Element {
                    tag: BytesStart::new("tag"),
                    children: VecDeque::from([
                        Node::Text("HTML ".into()),
                        Node::Element(Element {
                            tag: BytesStart::new("i"),
                            children: VecDeque::from([Node::Text("text".into())]),
                        }),
                        Node::Text(" is awesome! <3 <3".into()),
                    ]),
                })]),
            }
        );
    }

    #[test]
    fn multiple_roots() {
        let error = Element::from_str("<one/><two/>").unwrap_err();
        assert!(matches!(error, DomError::MultipleRoots));

        let error = Element::from_str("<one/><two></two>").unwrap_err();
        assert!(matches!(error, DomError::MultipleRoots));

        let error = Element::from_str("<one></one><two/>").unwrap_err();
        assert!(matches!(error, DomError::MultipleRoots));

        let error = Element::from_str("<one></one><two></two>").unwrap_err();
        assert!(matches!(error, DomError::MultipleRoots));
    }

    #[test]
    fn missing_root() {
        let error = Element::from_str("text").unwrap_err();
        assert!(matches!(error, DomError::MissingRoot));
    }

    #[test]
    fn missing_open_tag() {
        let error = Element::from_str("</close>").unwrap_err();
        assert!(matches!(error, DomError::Parse(Error::IllFormed(_))));
    }
}
