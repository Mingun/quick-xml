// This example demonstrates how to read a UTF-16 encoded XML file using
// DecodingReader, which auto-detects the encoding from the BOM or XML
// declaration and transcodes to UTF-8 for the parser.

fn main() -> Result<(), quick_xml::Error> {
    use std::fs::File;
    use std::io::BufReader;

    use quick_xml::encoding::DecodingReader;
    use quick_xml::events::Event;
    use quick_xml::reader::Reader;
    use quick_xml::XmlVersion;

    let file = File::open("tests/documents/encoding/utf16le-bom.xml")?;
    let transcoder = DecodingReader::new(BufReader::new(file));
    let mut reader = Reader::from_reader(transcoder);
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut version = XmlVersion::Implicit1_0;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Decl(e)) => {
                version = e.xml_version()?;
                println!("decl: version={:?}", version);
                if let Some(encoding) = e.encoder() {
                    reader.get_mut().set_encoding(encoding);
                }
            }
            Ok(Event::Start(ref e)) => {
                let name = e.name();
                let name = std::str::from_utf8(name.as_ref()).unwrap();
                println!("start: <{}>", name);

                for attr in e.attributes().flatten() {
                    let key = std::str::from_utf8(attr.key.as_ref()).unwrap();
                    let val = attr.normalized_value(version)?;
                    println!("  {}={:?}", key, val);
                }
            }
            Ok(Event::Text(e)) => {
                let text = e.xml_content(version)?;
                print!("text:   {}", text);
            }
            Ok(Event::End(ref e)) => {
                let name = e.name();
                let name = std::str::from_utf8(name.as_ref()).unwrap();
                println!("end:   </{}>", name);
            }
            Ok(Event::Eof) => break,
            Err(e) => panic!("Error at position {}: {:?}", reader.error_position(), e),
            _ => (),
        }
    }

    Ok(())
}
