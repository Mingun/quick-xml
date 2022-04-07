//! Utility functions for integration tests
// Not all tests use all helpers
#![allow(dead_code)]

use quick_xml::errors::Error;
use quick_xml::events::Event;
#[cfg(feature = "span")]
use quick_xml::events::Spanned;
use quick_xml::name::ResolveResult;
#[cfg(feature = "span")]
use quick_xml::reader::Span;
use quick_xml::reader::{NsReader, Reader};
use std::io::BufRead;
#[cfg(feature = "async-tokio")]
use tokio::io::AsyncBufRead;

/// Tests for https://github.com/tafia/quick-xml/issues/469
/// Exported to reuse in `async-tokio` tests.
#[macro_export]
macro_rules! small_buffers_tests {
    (
        #[$test:meta]
        $read_event:ident: $BufReader:ty
        $(, $async:ident, $await:ident)?
    ) => {
        mod small_buffers {
            use quick_xml::events::{BytesCData, BytesComment, BytesDecl, BytesPI, BytesStart, Event};
            #[cfg(feature = "span")]
            use quick_xml::events::Spanned;
            use quick_xml::reader::Reader;
            use pretty_assertions::assert_eq;

            #[$test]
            $($async)? fn decl() {
                let xml = "<?xml ?>";
                //         ^^^^^^^ data that fit into buffer
                let size = xml.match_indices("?>").next().unwrap().0 + 1;
                let br = <$BufReader>::with_capacity(size, xml.as_bytes());
                let mut reader = Reader::from_reader(br);
                let mut buf = Vec::new();

                let expected = BytesDecl::from_start(BytesStart::from_content("xml ", 3));

                #[cfg(feature = "span")]
                let expected = expected.with_span(0..8);

                assert_eq!(
                    reader.$read_event(&mut buf) $(.$await)? .unwrap(),
                    Event::Decl(expected)
                );
                assert_eq!(
                    reader.$read_event(&mut buf) $(.$await)? .unwrap(),
                    Event::Eof
                );
            }

            #[$test]
            $($async)? fn pi() {
                let xml = "<?pi?>";
                //         ^^^^^ data that fit into buffer
                let size = xml.match_indices("?>").next().unwrap().0 + 1;
                let br = <$BufReader>::with_capacity(size, xml.as_bytes());
                let mut reader = Reader::from_reader(br);
                let mut buf = Vec::new();

                let expected = BytesPI::new("pi");

                #[cfg(feature = "span")]
                let expected = expected.with_span(0..6);

                assert_eq!(
                    reader.$read_event(&mut buf) $(.$await)? .unwrap(),
                    Event::PI(expected)
                );
                assert_eq!(
                    reader.$read_event(&mut buf) $(.$await)? .unwrap(),
                    Event::Eof
                );
            }

            #[$test]
            $($async)? fn empty() {
                let xml = "<empty/>";
                //         ^^^^^^^ data that fit into buffer
                let size = xml.match_indices("/>").next().unwrap().0 + 1;
                let br = <$BufReader>::with_capacity(size, xml.as_bytes());
                let mut reader = Reader::from_reader(br);
                let mut buf = Vec::new();

                let expected = BytesStart::new("empty");

                #[cfg(feature = "span")]
                let expected = expected.with_span(0..8);

                assert_eq!(
                    reader.$read_event(&mut buf) $(.$await)? .unwrap(),
                    Event::Empty(expected)
                );
                assert_eq!(
                    reader.$read_event(&mut buf) $(.$await)? .unwrap(),
                    Event::Eof
                );
            }

            #[$test]
            $($async)? fn cdata1() {
                let xml = "<![CDATA[cdata]]>";
                //         ^^^^^^^^^^^^^^^ data that fit into buffer
                let size = xml.match_indices("]]>").next().unwrap().0 + 1;
                let br = <$BufReader>::with_capacity(size, xml.as_bytes());
                let mut reader = Reader::from_reader(br);
                let mut buf = Vec::new();

                let expected = BytesCData::new("cdata");

                #[cfg(feature = "span")]
                let expected = expected.with_span(0..17);

                assert_eq!(
                    reader.$read_event(&mut buf) $(.$await)? .unwrap(),
                    Event::CData(expected)
                );
                assert_eq!(
                    reader.$read_event(&mut buf) $(.$await)? .unwrap(),
                    Event::Eof
                );
            }

            #[$test]
            $($async)? fn cdata2() {
                let xml = "<![CDATA[cdata]]>";
                //         ^^^^^^^^^^^^^^^^ data that fit into buffer
                let size = xml.match_indices("]]>").next().unwrap().0 + 2;
                let br = <$BufReader>::with_capacity(size, xml.as_bytes());
                let mut reader = Reader::from_reader(br);
                let mut buf = Vec::new();

                let expected = BytesCData::new("cdata");

                #[cfg(feature = "span")]
                let expected = expected.with_span(0..17);

                assert_eq!(
                    reader.$read_event(&mut buf) $(.$await)? .unwrap(),
                    Event::CData(expected)
                );
                assert_eq!(
                    reader.$read_event(&mut buf) $(.$await)? .unwrap(),
                    Event::Eof
                );
            }

            #[$test]
            $($async)? fn comment1() {
                let xml = "<!--comment-->";
                //         ^^^^^^^^^^^^ data that fit into buffer
                let size = xml.match_indices("-->").next().unwrap().0 + 1;
                let br = <$BufReader>::with_capacity(size, xml.as_bytes());
                let mut reader = Reader::from_reader(br);
                let mut buf = Vec::new();

                let expected = BytesComment::new("comment").unwrap();

                #[cfg(feature = "span")]
                let expected = expected.with_span(0..14);

                assert_eq!(
                    reader.$read_event(&mut buf) $(.$await)? .unwrap(),
                    Event::Comment(expected)
                );
                assert_eq!(
                    reader.$read_event(&mut buf) $(.$await)? .unwrap(),
                    Event::Eof
                );
            }

            #[$test]
            $($async)? fn comment2() {
                let xml = "<!--comment-->";
                //         ^^^^^^^^^^^^^ data that fit into buffer
                let size = xml.match_indices("-->").next().unwrap().0 + 2;
                let br = <$BufReader>::with_capacity(size, xml.as_bytes());
                let mut reader = Reader::from_reader(br);
                let mut buf = Vec::new();

                let expected = BytesComment::new("comment").unwrap();

                #[cfg(feature = "span")]
                let expected = expected.with_span(0..14);

                assert_eq!(
                    reader.$read_event(&mut buf) $(.$await)? .unwrap(),
                    Event::Comment(expected)
                );
                assert_eq!(
                    reader.$read_event(&mut buf) $(.$await)? .unwrap(),
                    Event::Eof
                );
            }
        }
    };
}

/// Tests for reader-errors and reader-dtd.
#[macro_export]
macro_rules! event_ok {
    ($test:ident($xml:literal) => $pos:literal : $event:expr) => {
        mod $test {
            use super::*;

            mod reader {
                use super::*;
                use pretty_assertions::assert_eq;

                #[test]
                fn borrowed() {
                    let mut reader = Reader::from_str($xml);
                    reader.config_mut().enable_all_checks(true);
                    assert_eq!(
                        (
                            helpers::read_event(&mut reader).unwrap(),
                            reader.buffer_position()
                        ),
                        ($event, $pos)
                    );
                }

                #[test]
                fn buffered() {
                    let mut buf = Vec::new();
                    let mut reader = Reader::from_str($xml);
                    reader.config_mut().enable_all_checks(true);
                    assert_eq!(
                        (
                            helpers::read_event_into(&mut reader, &mut buf).unwrap(),
                            reader.buffer_position()
                        ),
                        ($event, $pos)
                    );
                }

                #[cfg(feature = "async-tokio")]
                #[tokio::test]
                async fn async_tokio() {
                    let mut buf = Vec::new();
                    let mut reader = Reader::from_str($xml);
                    reader.config_mut().enable_all_checks(true);
                    assert_eq!(
                        (
                            helpers::read_event_into_async(&mut reader, &mut buf)
                                .await
                                .unwrap(),
                            reader.buffer_position()
                        ),
                        ($event, $pos)
                    );
                }
            }

            mod ns_reader {
                use super::*;
                use pretty_assertions::assert_eq;

                #[test]
                fn borrowed() {
                    let mut reader = NsReader::from_str($xml);
                    reader.config_mut().enable_all_checks(true);
                    assert_eq!(
                        (
                            helpers::read_resolved_event(&mut reader).unwrap().1,
                            reader.buffer_position()
                        ),
                        ($event, $pos)
                    );
                }

                #[test]
                fn buffered() {
                    let mut buf = Vec::new();
                    let mut reader = NsReader::from_str($xml);
                    reader.config_mut().enable_all_checks(true);
                    assert_eq!(
                        (
                            helpers::read_resolved_event_into(&mut reader, &mut buf)
                                .unwrap()
                                .1,
                            reader.buffer_position()
                        ),
                        ($event, $pos)
                    );
                }

                #[cfg(feature = "async-tokio")]
                #[tokio::test]
                async fn async_tokio() {
                    let mut buf = Vec::new();
                    let mut reader = NsReader::from_str($xml);
                    reader.config_mut().enable_all_checks(true);
                    assert_eq!(
                        (
                            helpers::read_resolved_event_into_async(&mut reader, &mut buf)
                                .await
                                .unwrap()
                                .1,
                            reader.buffer_position()
                        ),
                        ($event, $pos)
                    );
                }
            }
        }
    };
}

/// Tests for reader-errors and reader-dtd.
#[macro_export]
macro_rules! syntax_err {
    ($test:ident($xml:literal) => $pos:expr, $cause:expr) => {
        mod $test {
            use super::*;

            mod reader {
                use super::*;
                use pretty_assertions::assert_eq;

                #[test]
                fn borrowed() {
                    let mut reader = Reader::from_str($xml);
                    assert_eq!(
                        helpers::read_event(&mut reader)
                            .expect("parser should return `Event::Text`"),
                        Event::Text(BytesText::new("."))
                    );
                    match reader.read_event() {
                        Err(Error::Syntax(cause)) => assert_eq!(
                            (cause, reader.error_position(), reader.buffer_position()),
                            ($cause, 1, $pos),
                        ),
                        x => panic!("Expected `Err(Syntax(_))`, but got {:?}", x),
                    }
                    assert_eq!(
                        reader
                            .read_event()
                            .expect("parser should return `Event::Eof` after error"),
                        Event::Eof
                    );
                }

                #[test]
                fn buffered() {
                    let mut buf = Vec::new();
                    let mut reader = Reader::from_str($xml);
                    assert_eq!(
                        helpers::read_event_into(&mut reader, &mut buf)
                            .expect("parser should return `Event::Text`"),
                        Event::Text(BytesText::new("."))
                    );
                    match reader.read_event_into(&mut buf) {
                        Err(Error::Syntax(cause)) => assert_eq!(
                            (cause, reader.error_position(), reader.buffer_position()),
                            ($cause, 1, $pos),
                        ),
                        x => panic!("Expected `Err(Syntax(_))`, but got {:?}", x),
                    }
                    assert_eq!(
                        reader
                            .read_event_into(&mut buf)
                            .expect("parser should return `Event::Eof` after error"),
                        Event::Eof
                    );
                }

                #[cfg(feature = "async-tokio")]
                #[tokio::test]
                async fn async_tokio() {
                    let mut buf = Vec::new();
                    let mut reader = Reader::from_str($xml);
                    assert_eq!(
                        helpers::read_event_into_async(&mut reader, &mut buf)
                            .await
                            .expect("parser should return `Event::Text`"),
                        Event::Text(BytesText::new("."))
                    );
                    match reader.read_event_into_async(&mut buf).await {
                        Err(Error::Syntax(cause)) => assert_eq!(
                            (cause, reader.error_position(), reader.buffer_position()),
                            ($cause, 1, $pos),
                        ),
                        x => panic!("Expected `Err(Syntax(_))`, but got {:?}", x),
                    }
                    assert_eq!(
                        reader
                            .read_event_into_async(&mut buf)
                            .await
                            .expect("parser should return `Event::Eof` after error"),
                        Event::Eof
                    );
                }
            }

            mod ns_reader {
                use super::*;
                use pretty_assertions::assert_eq;

                #[test]
                fn borrowed() {
                    let mut reader = NsReader::from_str($xml);
                    assert_eq!(
                        helpers::read_resolved_event(&mut reader)
                            .expect("parser should return `Event::Text`")
                            .1,
                        Event::Text(BytesText::new("."))
                    );
                    match reader.read_resolved_event() {
                        Err(Error::Syntax(cause)) => assert_eq!(
                            (cause, reader.error_position(), reader.buffer_position()),
                            ($cause, 1, $pos),
                        ),
                        x => panic!("Expected `Err(Syntax(_))`, but got {:?}", x),
                    }
                    assert_eq!(
                        reader
                            .read_resolved_event()
                            .expect("parser should return `Event::Eof` after error")
                            .1,
                        Event::Eof
                    );
                }

                #[test]
                fn buffered() {
                    let mut buf = Vec::new();
                    let mut reader = NsReader::from_str($xml);
                    assert_eq!(
                        helpers::read_resolved_event_into(&mut reader, &mut buf)
                            .expect("parser should return `Event::Text`")
                            .1,
                        Event::Text(BytesText::new("."))
                    );
                    match reader.read_resolved_event_into(&mut buf) {
                        Err(Error::Syntax(cause)) => assert_eq!(
                            (cause, reader.error_position(), reader.buffer_position()),
                            ($cause, 1, $pos),
                        ),
                        x => panic!("Expected `Err(Syntax(_))`, but got {:?}", x),
                    }
                    assert_eq!(
                        reader
                            .read_resolved_event_into(&mut buf)
                            .expect("parser should return `Event::Eof` after error")
                            .1,
                        Event::Eof
                    );
                }

                #[cfg(feature = "async-tokio")]
                #[tokio::test]
                async fn async_tokio() {
                    let mut buf = Vec::new();
                    let mut reader = NsReader::from_str($xml);
                    assert_eq!(
                        helpers::read_resolved_event_into_async(&mut reader, &mut buf)
                            .await
                            .expect("parser should return `Event::Text`")
                            .1,
                        Event::Text(BytesText::new("."))
                    );
                    match reader.read_resolved_event_into_async(&mut buf).await {
                        Err(Error::Syntax(cause)) => assert_eq!(
                            (cause, reader.error_position(), reader.buffer_position()),
                            ($cause, 1, $pos),
                        ),
                        x => panic!("Expected `Err(Syntax(_))`, but got {:?}", x),
                    }
                    assert_eq!(
                        reader
                            .read_resolved_event_into_async(&mut buf)
                            .await
                            .expect("parser should return `Event::Eof` after error")
                            .1,
                        Event::Eof
                    );
                }
            }
        }
    };
    ($test:ident($xml:literal) => $cause:expr) => {
        syntax_err!($test($xml) => $xml.len() as u64, $cause);
    };
}

pub fn read_event<'i>(reader: &mut Reader<&'i [u8]>) -> Result<Event<'i>, Error> {
    let event = reader.read_event()?;

    // We do not test correctness of spans here so just clear them
    #[cfg(feature = "span")]
    let event = event.with_span(Span::default());

    Ok(event)
}
pub fn read_event_into<'b, R: BufRead>(
    reader: &mut Reader<R>,
    buf: &'b mut Vec<u8>,
) -> Result<Event<'b>, Error> {
    let event = reader.read_event_into(buf)?;

    // We do not test correctness of spans here so just clear them
    #[cfg(feature = "span")]
    let event = event.with_span(Span::default());

    Ok(event)
}
#[cfg(feature = "async-tokio")]
pub async fn read_event_into_async<'b, R: AsyncBufRead + Unpin>(
    reader: &mut Reader<R>,
    buf: &'b mut Vec<u8>,
) -> Result<Event<'b>, Error> {
    let event = reader.read_event_into_async(buf).await?;

    // We do not test correctness of spans here so just clear them
    #[cfg(feature = "span")]
    let event = event.with_span(Span::default());

    Ok(event)
}

pub fn read_resolved_event<'ns, 'i>(
    reader: &'ns mut NsReader<&'i [u8]>,
) -> Result<(ResolveResult<'ns>, Event<'i>), Error> {
    let (rr, event) = reader.read_resolved_event()?;

    // We do not test correctness of spans here so just clear them
    #[cfg(feature = "span")]
    let event = event.with_span(Span::default());

    Ok((rr, event))
}
pub fn read_resolved_event_into<'ns, 'b, R: BufRead>(
    reader: &'ns mut NsReader<R>,
    buf: &'b mut Vec<u8>,
) -> Result<(ResolveResult<'ns>, Event<'b>), Error> {
    let (rr, event) = reader.read_resolved_event_into(buf)?;

    // We do not test correctness of spans here so just clear them
    #[cfg(feature = "span")]
    let event = event.with_span(Span::default());

    Ok((rr, event))
}
#[cfg(feature = "async-tokio")]
pub async fn read_resolved_event_into_async<'ns, 'b, R: AsyncBufRead + Unpin>(
    reader: &'ns mut NsReader<R>,
    buf: &'b mut Vec<u8>,
) -> Result<(ResolveResult<'ns>, Event<'b>), Error> {
    let (rr, event) = reader.read_resolved_event_into_async(buf).await?;

    // We do not test correctness of spans here so just clear them
    #[cfg(feature = "span")]
    let event = event.with_span(Span::default());

    Ok((rr, event))
}
