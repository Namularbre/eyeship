//! Parsing of `.env` files.

/// Parses `.env` content into `(key, value)` pairs, in file order.
///
/// Supports `KEY=VALUE`, `#` comment lines, blank lines, an optional
/// `export ` prefix, single and double quoted values, and inline ` #`
/// comments after unquoted values. Malformed lines are skipped.
pub fn parse_dotenv(content: &str) -> Vec<(String, String)> {
    content.lines().filter_map(parse_line).collect()
}

fn parse_line(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let line = line.strip_prefix("export ").map_or(line, str::trim_start);
    let (key, raw) = line.split_once('=')?;
    let key = key.trim();
    let valid = !key.is_empty() && !key.contains(char::is_whitespace);
    valid.then(|| (key.to_owned(), parse_value(raw.trim())))
}

fn parse_value(raw: &str) -> String {
    if let Some(rest) = raw.strip_prefix('\'') {
        rest.split_once('\'')
            .map_or(rest, |(value, _)| value)
            .to_owned()
    } else if let Some(rest) = raw.strip_prefix('"') {
        double_quoted(rest)
    } else {
        strip_inline_comment(raw).to_owned()
    }
}

/// Reads a double-quoted value up to its closing quote, resolving escapes.
fn double_quoted(rest: &str) -> String {
    rest.chars()
        .scan(false, |escaped, c| match (*escaped, c) {
            (true, _) => {
                *escaped = false;
                Some(Some(unescape(c)))
            }
            (false, '\\') => {
                *escaped = true;
                Some(None)
            }
            (false, '"') => None,
            (false, _) => Some(Some(c)),
        })
        .flatten()
        .collect()
}

fn unescape(c: char) -> char {
    match c {
        'n' => '\n',
        't' => '\t',
        'r' => '\r',
        other => other,
    }
}

/// Cuts an unquoted value at a `#` preceded by whitespace.
fn strip_inline_comment(raw: &str) -> &str {
    raw.char_indices()
        .zip(raw.chars().skip(1))
        .find(|((_, c), next)| c.is_whitespace() && *next == '#')
        .map_or(raw, |((i, _), _)| raw[..i].trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(k: &str, v: &str) -> (String, String) {
        (k.to_owned(), v.to_owned())
    }

    #[test]
    fn empty_content() {
        assert_eq!(parse_dotenv(""), vec![]);
    }

    #[test]
    fn simple_pairs_in_order() {
        assert_eq!(
            parse_dotenv("B=2\nA=1\n"),
            vec![pair("B", "2"), pair("A", "1")]
        );
    }

    #[test]
    fn comments_and_blank_lines_are_skipped() {
        let content = "# a comment\n\n   \n  # indented comment\nA=1\r\n";
        assert_eq!(parse_dotenv(content), vec![pair("A", "1")]);
    }

    #[test]
    fn export_prefix_is_ignored() {
        assert_eq!(parse_dotenv("export TAG=1.2"), vec![pair("TAG", "1.2")]);
    }

    #[test]
    fn whitespace_around_key_and_value_is_trimmed() {
        assert_eq!(parse_dotenv("  KEY = value  "), vec![pair("KEY", "value")]);
    }

    #[test]
    fn empty_value() {
        assert_eq!(parse_dotenv("EMPTY="), vec![pair("EMPTY", "")]);
    }

    #[test]
    fn value_may_contain_equals() {
        assert_eq!(parse_dotenv("URL=a=b=c"), vec![pair("URL", "a=b=c")]);
    }

    #[test]
    fn inline_comment_on_unquoted_value() {
        assert_eq!(
            parse_dotenv("PORT=8080 # http\nHASH=abc#def"),
            vec![pair("PORT", "8080"), pair("HASH", "abc#def")]
        );
    }

    #[test]
    fn single_quoted_value_is_literal() {
        assert_eq!(
            parse_dotenv("A='x # y \\n $z' # comment"),
            vec![pair("A", "x # y \\n $z")]
        );
    }

    #[test]
    fn double_quoted_value_supports_escapes() {
        assert_eq!(
            parse_dotenv(r#"A="say \"hi\"\nnext # not a comment" # comment"#),
            vec![pair("A", "say \"hi\"\nnext # not a comment")]
        );
    }

    #[test]
    fn unterminated_quote_keeps_the_rest_of_the_line() {
        assert_eq!(parse_dotenv("A=\"open"), vec![pair("A", "open")]);
    }

    #[test]
    fn malformed_lines_are_skipped() {
        assert_eq!(
            parse_dotenv("NOEQUALS\n=value\nBAD KEY=1\nOK=1"),
            vec![pair("OK", "1")]
        );
    }
}
