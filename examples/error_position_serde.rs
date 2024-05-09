// example extraction of error position from Deserializer using into_inner()
//
// note: to use serde, the feature needs to be enabled
// run example with:
//    cargo run --example error_position_serde --features="serialize"

use quick_xml::Reader;
use serde::Deserialize;
use std::error::Error;
use std::fs::read_to_string;

fn main() -> Result<(), Box<dyn Error>> {
    let contents = read_to_string("tests/documents/document.xml")?;

    // intentionally break XML: split document just before the end tag
    let contents = contents.split("<w:sectPr>").next().unwrap();

    // split into lines
    let contents = contents.replace('<', "\n<");

    for (num, line) in contents.lines().enumerate() {
        println!("{:02}: {}", num + 1, line);
    }

    // returns an error: Ln 244, Col 7: ill-formed document
    let doc: Document = from_str_ln_col(&contents)?;

    println!("doc: {doc:#?}");

    Ok(())
}

pub fn from_str_ln_col<'de, T>(source: &'de str) -> Result<T, Box<dyn Error>>
where
    T: Deserialize<'de>,
{
    use quick_xml::de::Deserializer;

    let mut de = Deserializer::from_str(source);
    let result = T::deserialize(&mut de);

    match result {
        Err(err) => {
            let buf_pos = de.get_ref().buffer_position();
            let msg = if let Some(consumed) = source.as_bytes().get(..buf_pos) {
                pos2line(consumed)
            } else {
                format!("position: {}", buf_pos)
            };
            Err(Box::<dyn Error>::from(format!("{msg}: {err}")))
        }
        Ok(object) => Ok(object),
    }
}

pub fn pos2line(buf: &[u8]) -> String {
    let line_num = buf.iter().filter(|&c| *c == b'\n').count() + 1;
    let line: &[u8] = buf.rsplit(|e| *e == b'\n').next().unwrap_or(buf);
    let col_num = line.len();
    format!("Ln {}, Col {}", line_num, col_num)
}

#[derive(Debug, Default, PartialEq, Deserialize)]
#[serde(rename = "document")]
pub struct Document {
    #[serde(rename = "body")]
    pub body: Body,
}

#[derive(Debug, Default, PartialEq, Deserialize)]
pub struct Body {
    #[serde(rename = "$value")]
    pub content: Vec<BodyContent>,
}

#[derive(Debug, PartialEq, Deserialize)]
pub enum BodyContent {
    #[serde(rename = "p")]
    Paragraph(Paragraph),

    #[serde(rename = "tbl")]
    Table(Table),

    #[serde(rename = "sectPr")]
    Section(Section),
}

#[derive(Debug, PartialEq, Deserialize)]
pub struct Table {}

#[derive(Debug, PartialEq, Deserialize)]
pub struct Section {}

#[derive(Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
#[serde(rename = "p")]
pub struct Paragraph {
    #[serde(
        rename = "r",
        alias = "hyperlink",
        alias = "bookmarkStart",
        alias = "bookmarkEnd"
    )]
    pub content: Option<Vec<ParagraphContent>>,
}

#[derive(Debug, Deserialize, PartialEq)]
pub enum ParagraphContent {
    #[serde(rename = "r")]
    Run(Run),
}
#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(rename = "r")]
pub struct Run {
    #[serde(rename = "$value")]
    pub content: Vec<RunContent>,
}

#[derive(Debug, Default, Deserialize, PartialEq)]
pub struct CharacterProperty {}

#[derive(Debug, Deserialize, PartialEq)]
pub enum RunContent {
    #[serde(rename = "rPr")]
    Property(CharacterProperty),

    #[serde(rename = "t")]
    Text(Text),
}

#[derive(Debug, Default, Deserialize, PartialEq)]
pub struct Text {
    #[serde(rename = "$text")]
    pub text: String,
}
