use criterion::{self, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use pretty_assertions::assert_eq;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use std::hint::black_box;

// https://mirror.accum.se/mirror/wikimedia.org/dumps/angwiktionary/20260901/
static BIG_2M: &str = include_str!("../../tests/documents/angwiktionary-20260901-stub-articles.xml");
static BIG_5M: &str = include_str!("../../tests/documents/angwiktionary-20260901-pages-meta-current.xml");
static BIG_100M: &str = include_str!("../../tests/documents/angwiktionary-20260901-pages-meta-history.xml");

// Test name, XML content, count of tags
static BIG_FILES: [(&str, &str, usize); 3] = [
    ("2M", BIG_2M, 57939),
    ("5M", BIG_5M, 68091),
    ("100M", BIG_100M, 673012),
];
fn big_files(c: &mut Criterion) {
    let mut group = c.benchmark_group("big files");
    for (filename, data, total_tags) in BIG_FILES.iter() {
        let total_tags = *total_tags;

        group.throughput(Throughput::Bytes(data.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("quick_xml:borrowed", filename),
            *data,
            |b, input| {
                b.iter(|| {
                    let mut reader = Reader::from_str(input);
                    reader.config_mut().check_end_names = false;
                    let mut count = black_box(0);
                    loop {
                        match reader.read_event() {
                            Ok(Event::Start(_)) | Ok(Event::Empty(_)) => count += 1,
                            Ok(Event::Eof) => break,
                            _ => (),
                        }
                    }
                    assert_eq!(count, total_tags, "Overall tag count in {}", filename);
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("quick_xml:buffered", filename),
            *data,
            |b, input| {
                b.iter(|| {
                    let mut reader = Reader::from_reader(input.as_bytes());
                    reader.config_mut().check_end_names = false;
                    let mut count = black_box(0);
                    let mut buf = Vec::new();
                    loop {
                        match reader.read_event_into(&mut buf) {
                            Ok(Event::Start(_)) | Ok(Event::Empty(_)) => count += 1,
                            Ok(Event::Eof) => break,
                            _ => (),
                        }
                        buf.clear();
                    }
                    assert_eq!(count, total_tags, "Overall tag count in {}", filename);
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("maybe_xml:0.10", filename),
            *data,
            |b, input| {
                use maybe_xml_0_10::token::Ty;
                use maybe_xml_0_10::Reader;

                b.iter(|| {
                    let reader = Reader::from_str(input);

                    let mut count = black_box(0);
                    for token in reader.into_iter() {
                        match token.ty() {
                            Ty::StartTag(_) | Ty::EmptyElementTag(_) => count += 1,
                            _ => (),
                        }
                    }
                    assert_eq!(count, total_tags, "Overall tag count in {}", filename);
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("maybe_xml:0.11", filename),
            *data,
            |b, input| {
                use maybe_xml::token::Ty;
                use maybe_xml::Reader;

                b.iter(|| {
                    let reader = Reader::from_str(input);

                    let mut count = black_box(0);
                    for token in reader.into_iter() {
                        match token.ty() {
                            Ty::StartTag(_) | Ty::EmptyElementTag(_) => count += 1,
                            _ => (),
                        }
                    }
                    assert_eq!(count, total_tags, "Overall tag count in {}", filename);
                })
            },
        );
    }

    group.finish();
}

criterion_group!(benches, big_files);
criterion_main!(benches);
