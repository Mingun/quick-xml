Run fuzzing with `-O` to avoid false positives at `debug_assert!`, e.g.:

```bash
cargo fuzz run -O -j4 fuzz_target_1
```

See also: https://github.com/rust-fuzz/cargo-fuzz

## Targets

- `fuzz_target_1` — drives `Reader::from_reader` over `Cursor<&[u8]>` and
  `Reader::from_str`. Broad coverage of event-decoding and writer
  round-tripping. Fast per-execution.
- `structured_roundtrip` — uses `arbitrary` to build a writer program,
  emits XML, then re-reads it. Targets writer/reader symmetry.
- `fuzz_chunked_reader` — drives `Reader::from_reader` over a `BufReader`
  with a small fuzz-controlled capacity. Exercises parser states that
  span chunk boundaries (where `BufRead::fill_buf` returns a partial
  window) — a regime not reached by `Cursor`-backed harnesses. Issues
  #950 and #957 are examples of bugs in this regime.

The targets seed off the same input shape (`&[u8]`) so corpora can be
shared between them.
