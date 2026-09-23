//! JSON with comments, which is what agent config files actually are.
//!
//! `.mcp.json`, `.claude/settings.json` and `.pi/mcp.json` are hand-edited far more often
//! than they are generated, and the agents that read them tolerate `//` comments and
//! trailing commas - Pi ships its own `stripJsonComments` for exactly this. A survey that
//! refuses to parse one is a survey that cannot read a repo somebody has explained their
//! own config in.
//!
//! Comments are blanked rather than deleted, so every byte after them keeps its offset
//! and a genuine syntax error still reports the line and column it is really on.

use std::borrow::Cow;

pub struct Stripped<'a> {
    pub text: Cow<'a, str>,
    /// Whether a comment was removed, as opposed to only a trailing comma. Callers about
    /// to rewrite the file need the difference: a dropped trailing comma costs nothing,
    /// a dropped comment deletes something a person wrote.
    pub had_comments: bool,
}

/// Blank out `//` and `/* */` comments, and any comma left dangling before `}` or `]`.
///
/// Byte-oriented on purpose: every delimiter that matters is ASCII, and UTF-8
/// continuation bytes can never collide with one, so text inside a string is copied
/// through untouched however it is encoded.
pub fn strip(text: &str) -> Stripped<'_> {
    let source = text.as_bytes();
    let mut out: Option<Vec<u8>> = None;
    let mut had_comments = false;
    // The last byte that was not whitespace and not blanked, which is the only way to
    // know whether the comma before a `}` is trailing.
    let mut last_significant: Option<usize> = None;
    let mut i = 0;

    while i < source.len() {
        match source[i] {
            b'"' => {
                last_significant = Some(i);
                i += 1;
                while i < source.len() {
                    match source[i] {
                        // Skip the escaped byte whatever it is, so `\"` does not end the
                        // string and `\\` does not escape the quote after it.
                        b'\\' => i += 2,
                        b'"' => {
                            last_significant = Some(i);
                            i += 1;
                            break;
                        }
                        _ => i += 1,
                    }
                }
            }
            b'/' if source.get(i + 1) == Some(&b'/') => {
                let start = i;
                while i < source.len() && source[i] != b'\n' {
                    i += 1;
                }
                blank(&mut out, source, start..i);
                had_comments = true;
            }
            b'/' if source.get(i + 1) == Some(&b'*') => {
                let start = i;
                i += 2;
                while i + 1 < source.len() && !(source[i] == b'*' && source[i + 1] == b'/') {
                    i += 1;
                }
                // An unterminated block comment runs to the end, which leaves the parser
                // to report the truncation rather than this hiding it.
                i = (i + 2).min(source.len());
                blank(&mut out, source, start..i);
                had_comments = true;
            }
            b'}' | b']' => {
                if let Some(previous) = last_significant {
                    if out.as_deref().unwrap_or(source)[previous] == b',' {
                        blank(&mut out, source, previous..previous + 1);
                    }
                }
                last_significant = Some(i);
                i += 1;
            }
            byte if byte.is_ascii_whitespace() => i += 1,
            _ => {
                last_significant = Some(i);
                i += 1;
            }
        }
    }

    match out {
        // Only ASCII bytes are ever overwritten, and only with ASCII, so what went in as
        // UTF-8 comes out as UTF-8.
        Some(bytes) => Stripped {
            text: Cow::Owned(String::from_utf8(bytes).expect("blanking ascii keeps utf-8")),
            had_comments,
        },
        None => Stripped {
            text: Cow::Borrowed(text),
            had_comments,
        },
    }
}

/// Overwrite a range with spaces, keeping newlines so line numbers survive.
fn blank(out: &mut Option<Vec<u8>>, source: &[u8], range: std::ops::Range<usize>) {
    let buffer = out.get_or_insert_with(|| source.to_vec());
    for byte in &mut buffer[range] {
        if *byte != b'\n' {
            *byte = b' ';
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> serde_json::Value {
        serde_json::from_str(&strip(text).text).expect("stripped json parses")
    }

    #[test]
    fn a_line_comment_is_removed() {
        let value = parse("{\n  // the server pi needs and claude does not\n  \"a\": 1\n}");
        assert_eq!(value["a"], 1);
        assert!(strip("// x\n{}").had_comments);
    }

    #[test]
    fn a_block_comment_is_removed_however_many_lines_it_spans() {
        let value = parse("{\n  /* one\n     two\n     three */\n  \"a\": 1\n}");
        assert_eq!(value["a"], 1);
    }

    #[test]
    fn a_trailing_comma_is_removed_from_objects_and_arrays() {
        let value = parse("{\"a\": [1, 2,], \"b\": 2,}");
        assert_eq!(value["a"], serde_json::json!([1, 2]));
        assert_eq!(value["b"], 2);
    }

    #[test]
    fn a_trailing_comma_alone_is_not_reported_as_a_comment() {
        let stripped = strip("{\"a\": 1,}");
        assert!(
            !stripped.had_comments,
            "a dropped comma deletes nothing anybody wrote"
        );
        assert_ne!(stripped.text, "{\"a\": 1,}", "but it is still removed");
    }

    #[test]
    fn something_that_looks_like_a_comment_inside_a_string_is_left_alone() {
        // The case that makes a naive regex wrong: a url, and a comma that is only text.
        let value = parse(r#"{"url": "https://example.com/x", "note": "a, b // not a comment"}"#);
        assert_eq!(value["url"], "https://example.com/x");
        assert_eq!(value["note"], "a, b // not a comment");
        assert!(!strip(r#"{"url": "https://example.com"}"#).had_comments);
    }

    #[test]
    fn an_escaped_quote_does_not_end_the_string() {
        let value = parse(r#"{"a": "he said \"// hi\"", "b": 1}"#);
        assert_eq!(value["a"], r#"he said "// hi""#);
        assert_eq!(value["b"], 1);
    }

    #[test]
    fn a_file_with_nothing_to_strip_is_borrowed_unchanged() {
        let stripped = strip(r#"{"a": 1}"#);
        assert!(matches!(stripped.text, Cow::Borrowed(_)));
        assert!(!stripped.had_comments);
    }

    #[test]
    fn stripping_keeps_every_later_byte_where_it_was() {
        // So that a real syntax error is still reported on the line it is on, rather than
        // on whatever line it slid up to.
        let text = "{\n  // a comment\n  \"a\": oops\n}";
        let stripped = strip(text);
        assert_eq!(stripped.text.len(), text.len());
        let error = serde_json::from_str::<serde_json::Value>(&stripped.text).unwrap_err();
        assert_eq!(error.line(), 3, "the bad value is on line 3");
    }

    #[test]
    fn multibyte_text_survives() {
        let value = parse("{\n  // comment\n  \"a\": \"café ✓ 日本\"\n}");
        assert_eq!(value["a"], "café ✓ 日本");
    }

    #[test]
    fn an_unterminated_block_comment_does_not_panic() {
        let stripped = strip("{\"a\": 1} /* never closed");
        assert!(stripped.had_comments);
        assert!(serde_json::from_str::<serde_json::Value>(&stripped.text).is_ok());
    }
}
