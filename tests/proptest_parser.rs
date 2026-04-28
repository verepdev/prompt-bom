use prompt_bom::services::transcript::parse_from_reader;
use proptest::prelude::*;
use std::io::Cursor;

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, .. ProptestConfig::default() })]

    /// Arbitrary UTF-8 input must never panic the parser. It may produce zero
    /// records, but it must always return — strict-mode parsing of malformed
    /// transcripts is a non-goal for M1.
    #[test]
    fn parser_does_not_panic_on_arbitrary_strings(s in "[\\PC]{0,500}") {
        let cursor = Cursor::new(s);
        let _ = parse_from_reader(cursor);
    }

    /// Newline-spammed input — exercises the "skip blank lines" path under
    /// adversarial whitespace patterns.
    #[test]
    fn parser_handles_arbitrary_newline_runs(s in "(\n|\r| |\\PC){0,500}") {
        let cursor = Cursor::new(s);
        let _ = parse_from_reader(cursor);
    }

    /// JSON-ish input: a list of either valid small JSON objects or random
    /// noise per line. Closer to the malformed-but-mostly-valid transcripts
    /// we see in the wild.
    #[test]
    fn parser_handles_mixed_valid_and_garbage_lines(
        lines in proptest::collection::vec(jsonish_line(), 0..40)
    ) {
        let input = lines.join("\n");
        let cursor = Cursor::new(input);
        let _ = parse_from_reader(cursor);
    }
}

fn jsonish_line() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(String::new()),
        Just(r#"{"type":"user","sessionId":"s","uuid":"u"}"#.to_string()),
        Just(
            r#"{"type":"assistant","sessionId":"s","uuid":"u","message":{"content":[]}}"#
                .to_string()
        ),
        "[\\PC]{0,80}".prop_map(|s| s.replace('\n', " ")),
    ]
}
