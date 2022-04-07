//! Utility functions for integration tests
// Not all tests use all helpers
#![allow(dead_code)]

use quick_xml::de::Deserializer;
use quick_xml::errors::Error;
use quick_xml::events::Event;
#[cfg(feature = "span")]
use quick_xml::events::Spanned;
use quick_xml::name::ResolveResult;
#[cfg(feature = "span")]
use quick_xml::reader::Span;
use quick_xml::reader::{NsReader, Reader};
use quick_xml::DeError;
use serde::Deserialize;
use std::io::BufRead;
#[cfg(feature = "async-tokio")]
use tokio::io::AsyncBufRead;

/// Deserialize an instance of type T from a string of XML text.
/// If deserialization was succeeded checks that all XML events was consumed
pub fn from_str<'de, T>(source: &'de str) -> Result<T, DeError>
where
    T: Deserialize<'de>,
{
    // Log XML that we try to deserialize to see it in the failed tests output
    dbg!(source);
    let mut de = Deserializer::from_str(source);
    let result = T::deserialize(&mut de);

    // If type was deserialized, the whole XML document should be consumed
    if let Ok(_) = result {
        assert!(de.is_empty(), "the whole XML document should be consumed");
    }

    result
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
