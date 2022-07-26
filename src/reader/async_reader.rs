//! This is an implementation of [`Reader`] for reading from a [`AsyncBufRead`]
//! as underlying byte stream. This reader fully implements async/await so reading
//! can use non-blocking I/O.

use async_recursion::async_recursion;
use tokio::io::{self, AsyncBufRead, AsyncBufReadExt};

#[cfg(feature = "encoding")]
use crate::encoding::detect_encoding;
use crate::events::{BytesText, Event};
use crate::reader::{is_whitespace, BangType, EncodingRef, ReadElementState, Reader, TagState};
use crate::{Error, Result};

/// A struct for handling reading functions based on reading from an [`AsyncBufRead`].
#[derive(Debug, Clone)]
pub struct AsyncReader<R> {
    reader: R,
}

impl<R: AsyncBufRead + Unpin> AsyncReader<R> {
    impl_buffered_source!('b, reader, async, await);
}

impl<R: AsyncBufRead + Unpin> Reader<AsyncReader<R>> {
    /// Creates a `Reader` that reads from a given reader.
    pub fn from_async_reader(reader: R) -> Self {
        Self::from_reader(AsyncReader { reader })
    }

    /// Read an event that borrows from the input rather than a buffer.
    #[async_recursion(?Send)]
    pub async fn read_event_into_async<'b>(&mut self, buf: &'b mut Vec<u8>) -> Result<Event<'b>> {
        let event = match self.tag_state {
            TagState::Init => self.read_until_open_async(buf, true).await,
            TagState::Closed => self.read_until_open_async(buf, false).await,
            TagState::Opened => self.read_until_close_async(buf).await,
            TagState::Empty => self.close_expanded_empty(),
            TagState::Exit => return Ok(Event::Eof),
        };
        match event {
            Err(_) | Ok(Event::Eof) => self.tag_state = TagState::Exit,
            _ => {}
        }
        event
    }

    /// Read until '<' is found and moves reader to an `Opened` state.
    ///
    /// Return a `StartText` event if `first` is `true` and a `Text` event otherwise
    async fn read_until_open_async<'b>(
        &mut self,
        buf: &'b mut Vec<u8>,
        first: bool,
    ) -> Result<Event<'b>> {
        self.tag_state = TagState::Opened;

        if self.trim_text_start {
            self.reader.skip_whitespace(&mut self.buf_position).await?;
        }

        // If we already at the `<` symbol, do not try to return an empty Text event
        if self.reader.skip_one(b'<', &mut self.buf_position).await? {
            return self.read_event_into_async(buf).await;
        }

        match self
            .reader
            .read_bytes_until(b'<', buf, &mut self.buf_position)
            .await
        {
            Ok(Some(bytes)) => {
                #[cfg(feature = "encoding")]
                if first && self.encoding.can_be_refined() {
                    if let Some(encoding) = detect_encoding(bytes) {
                        self.encoding = EncodingRef::BomDetected(encoding);
                    }
                }

                let content = if self.trim_text_end {
                    // Skip the ending '<
                    let len = bytes
                        .iter()
                        .rposition(|&b| !is_whitespace(b))
                        .map_or_else(|| bytes.len(), |p| p + 1);
                    &bytes[..len]
                } else {
                    bytes
                };

                Ok(if first {
                    Event::StartText(BytesText::wrap(content, self.decoder()).into())
                } else {
                    Event::Text(BytesText::wrap(content, self.decoder()))
                })
            }
            Ok(None) => Ok(Event::Eof),
            Err(e) => Err(e),
        }
    }

    /// Private function to read until `>` is found. This function expects that
    /// it was called just after encounter a `<` symbol.
    async fn read_until_close_async<'b>(&mut self, buf: &'b mut Vec<u8>) -> Result<Event<'b>> {
        self.tag_state = TagState::Closed;

        match self.reader.peek_one().await {
            // `<!` - comment, CDATA or DOCTYPE declaration
            Ok(Some(b'!')) => match self
                .reader
                .read_bang_element(buf, &mut self.buf_position)
                .await
            {
                Ok(None) => Ok(Event::Eof),
                Ok(Some((bang_type, bytes))) => self.read_bang(bang_type, bytes),
                Err(e) => Err(e),
            },
            // `</` - closing tag
            Ok(Some(b'/')) => match self
                .reader
                .read_bytes_until(b'>', buf, &mut self.buf_position)
                .await
            {
                Ok(None) => Ok(Event::Eof),
                Ok(Some(bytes)) => self.read_end(bytes),
                Err(e) => Err(e),
            },
            // `<?` - processing instruction
            Ok(Some(b'?')) => match self
                .reader
                .read_bytes_until(b'>', buf, &mut self.buf_position)
                .await
            {
                Ok(None) => Ok(Event::Eof),
                Ok(Some(bytes)) => self.read_question_mark(bytes),
                Err(e) => Err(e),
            },
            // `<...` - opening or self-closed tag
            Ok(Some(_)) => match self.reader.read_element(buf, &mut self.buf_position).await {
                Ok(None) => Ok(Event::Eof),
                Ok(Some(bytes)) => self.read_start(bytes),
                Err(e) => Err(e),
            },
            Ok(None) => Ok(Event::Eof),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod test {
    use super::AsyncReader;
    use crate::reader::test::check;

    fn wrap<R>(input: R) -> AsyncReader<R> {
        AsyncReader { reader: input }
    }

    check!(
        #[tokio::test]
        wrap,
        &mut Vec::new(),
        async, await
    );
}
