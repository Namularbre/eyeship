//! Compose variable interpolation (`$VAR`, `${VAR:-default}`, `$$`...).
//!
//! Interpolation never fails: a variable that cannot be resolved becomes an
//! empty string and is reported as [`Unresolved`].

/// The result of interpolating one string.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Interpolated {
    /// The string with every variable replaced.
    pub value: String,
    /// Variables that had no value and no default, in input order.
    pub unresolved: Vec<Unresolved>,
}

/// A variable reference that could not be resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unresolved {
    /// The variable name.
    pub name: String,
    /// Byte offset of the `$` of the reference in the input.
    pub offset: usize,
    /// The error message of a `${VAR:?message}` / `${VAR?message}` reference.
    pub message: Option<String>,
}

/// Interpolates `input`, resolving variables with `lookup`.
pub fn interpolate(input: &str, lookup: &dyn Fn(&str) -> Option<String>) -> Interpolated {
    interpolate_at(input, 0, lookup)
}

type Lookup<'a> = &'a dyn Fn(&str) -> Option<String>;

impl Interpolated {
    fn literal(text: impl Into<String>) -> Self {
        Self {
            value: text.into(),
            unresolved: Vec::new(),
        }
    }

    fn missing(name: &str, offset: usize, message: Option<String>) -> Self {
        Self {
            value: String::new(),
            unresolved: vec![Unresolved {
                name: name.to_owned(),
                offset,
                message,
            }],
        }
    }

    fn append(self, other: Self) -> Self {
        Self {
            value: self.value + &other.value,
            unresolved: self
                .unresolved
                .into_iter()
                .chain(other.unresolved)
                .collect(),
        }
    }
}

/// Interpolates `input`, whose first byte sits at `base` in the original input.
fn interpolate_at(input: &str, base: usize, lookup: Lookup<'_>) -> Interpolated {
    std::iter::successors(next_segment(input, 0, base, lookup), |(_, pos)| {
        next_segment(input, *pos, base, lookup)
    })
    .map(|(segment, _)| segment)
    .fold(Interpolated::default(), Interpolated::append)
}

/// Expands the segment starting at `pos`; returns it with the next position.
fn next_segment(
    input: &str,
    pos: usize,
    base: usize,
    lookup: Lookup<'_>,
) -> Option<(Interpolated, usize)> {
    let rest = input.get(pos..).filter(|rest| !rest.is_empty())?;
    Some(match rest.find('$') {
        Some(0) => {
            let (segment, len) = expand(rest, base + pos, lookup);
            (segment, pos + len)
        }
        Some(i) => (Interpolated::literal(&rest[..i]), pos + i),
        None => (Interpolated::literal(rest), input.len()),
    })
}

/// Expands the reference at the start of `rest` (which starts with `$`);
/// returns the expansion and the number of bytes consumed.
fn expand(rest: &str, offset: usize, lookup: Lookup<'_>) -> (Interpolated, usize) {
    let after = &rest[1..];
    match after.chars().next() {
        Some('$') => (Interpolated::literal("$"), 2),
        Some('{') => braced(rest, offset, lookup).unwrap_or((Interpolated::literal("$"), 1)),
        Some(c) if is_name_start(c) => {
            let len = name_len(after);
            let name = &after[..len];
            let segment = lookup(name).map_or_else(
                || Interpolated::missing(name, offset, None),
                Interpolated::literal,
            );
            (segment, 1 + len)
        }
        _ => (Interpolated::literal("$"), 1),
    }
}

/// What follows the name in a braced reference.
#[derive(Clone, Copy)]
enum Modifier {
    /// `${VAR}`
    Plain,
    /// `${VAR-default}` / `${VAR:-default}`
    Default,
    /// `${VAR?message}` / `${VAR:?message}`
    Error,
    /// `${VAR+alternative}` / `${VAR:+alternative}`
    Alternative,
}

/// Expands a `${...}` reference, or returns `None` when it is malformed.
fn braced(rest: &str, offset: usize, lookup: Lookup<'_>) -> Option<(Interpolated, usize)> {
    let body_start = 2;
    let body = &rest[body_start..body_start + closing_brace(&rest[body_start..])?];
    let name = &body[..name_len(body)];
    if !name.chars().next().is_some_and(is_name_start) {
        return None;
    }
    let (modifier, strict, op_len) = modifier(&body[name.len()..])?;
    let operand_start = body_start + name.len() + op_len;
    let operand = &rest[operand_start..body_start + body.len()];
    let expand_operand = || interpolate_at(operand, offset + operand_start, lookup);
    // With a colon, an empty value counts as unset.
    let chosen = lookup(name).filter(|value| !strict || !value.is_empty());
    let segment = match modifier {
        Modifier::Plain => chosen.map_or_else(
            || Interpolated::missing(name, offset, None),
            Interpolated::literal,
        ),
        Modifier::Default => chosen.map_or_else(expand_operand, Interpolated::literal),
        Modifier::Error => chosen.map_or_else(
            || {
                let message = expand_operand().value;
                Interpolated::missing(name, offset, Some(message).filter(|m| !m.is_empty()))
            },
            Interpolated::literal,
        ),
        Modifier::Alternative => chosen.map(|_| expand_operand()).unwrap_or_default(),
    };
    Some((segment, body_start + body.len() + 1))
}

/// Parses the modifier after the name: `(modifier, has_colon, length)`.
fn modifier(text: &str) -> Option<(Modifier, bool, usize)> {
    let (strict, unprefixed) = text
        .strip_prefix(':')
        .map_or((false, text), |stripped| (true, stripped));
    let colon_len = usize::from(strict);
    match unprefixed.chars().next() {
        None if !strict => Some((Modifier::Plain, false, 0)),
        Some('-') => Some((Modifier::Default, strict, colon_len + 1)),
        Some('?') => Some((Modifier::Error, strict, colon_len + 1)),
        Some('+') => Some((Modifier::Alternative, strict, colon_len + 1)),
        _ => None,
    }
}

/// Byte index of the `}` closing a reference body, skipping nested braces.
fn closing_brace(text: &str) -> Option<usize> {
    text.bytes()
        .enumerate()
        .scan(0usize, |depth, (i, byte)| {
            Some(match (byte, *depth) {
                (b'}', 0) => Some(i),
                (b'}', _) => {
                    *depth -= 1;
                    None
                }
                (b'{', _) => {
                    *depth += 1;
                    None
                }
                _ => None,
            })
        })
        .flatten()
        .next()
}

fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

/// Length in bytes of the variable-name characters at the start of `text`.
fn name_len(text: &str) -> usize {
    text.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(text.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(name: &str) -> Option<String> {
        match name {
            "TAG" => Some("1.2".to_owned()),
            "EMPTY" => Some(String::new()),
            "HOST" => Some("db".to_owned()),
            _ => None,
        }
    }

    fn value(input: &str) -> String {
        interpolate(input, &env).value
    }

    fn unresolved(name: &str, offset: usize) -> Unresolved {
        Unresolved {
            name: name.to_owned(),
            offset,
            message: None,
        }
    }

    #[test]
    fn plain_text_is_unchanged() {
        assert_eq!(
            interpolate("nginx:latest", &env),
            Interpolated {
                value: "nginx:latest".to_owned(),
                unresolved: vec![],
            }
        );
    }

    #[test]
    fn bare_variable() {
        assert_eq!(value("nginx:$TAG"), "nginx:1.2");
        assert_eq!(value("$HOST:5432"), "db:5432");
        assert_eq!(value("$TAG.x"), "1.2.x");
    }

    #[test]
    fn braced_variable() {
        assert_eq!(value("nginx:${TAG}-alpine"), "nginx:1.2-alpine");
    }

    #[test]
    fn double_dollar_escapes() {
        assert_eq!(value("echo $$HOME $${TAG}"), "echo $HOME ${TAG}");
    }

    #[test]
    fn lone_dollar_is_literal() {
        assert_eq!(value("cost: 5$"), "cost: 5$");
        assert_eq!(value("a $ b $1"), "a $ b $1");
    }

    #[test]
    fn unset_variable_is_empty_and_reported() {
        assert_eq!(
            interpolate("x${MISSING}y$OTHER", &env),
            Interpolated {
                value: "xy".to_owned(),
                unresolved: vec![unresolved("MISSING", 1), unresolved("OTHER", 12)],
            }
        );
    }

    #[test]
    fn empty_variable_is_resolved() {
        assert_eq!(
            interpolate("a${EMPTY}b", &env),
            Interpolated {
                value: "ab".to_owned(),
                unresolved: vec![],
            }
        );
    }

    #[test]
    fn colon_dash_default_applies_when_unset_or_empty() {
        assert_eq!(value("${MISSING:-fallback}"), "fallback");
        assert_eq!(value("${EMPTY:-fallback}"), "fallback");
        assert_eq!(value("${TAG:-fallback}"), "1.2");
    }

    #[test]
    fn dash_default_applies_only_when_unset() {
        assert_eq!(value("${MISSING-fallback}"), "fallback");
        assert_eq!(value("${EMPTY-fallback}"), "");
        assert_eq!(value("${TAG-fallback}"), "1.2");
    }

    #[test]
    fn default_may_be_empty_or_contain_colons() {
        assert_eq!(interpolate("${MISSING:-}", &env), Interpolated::default());
        assert_eq!(value("${MISSING:-http://x:80}"), "http://x:80");
    }

    #[test]
    fn colon_question_reports_message_when_unset_or_empty() {
        assert_eq!(
            interpolate("${MISSING:?set it}", &env),
            Interpolated {
                value: String::new(),
                unresolved: vec![Unresolved {
                    name: "MISSING".to_owned(),
                    offset: 0,
                    message: Some("set it".to_owned()),
                }],
            }
        );
        assert_eq!(interpolate("${EMPTY:?set it}", &env).unresolved.len(), 1);
        assert_eq!(value("${TAG:?set it}"), "1.2");
    }

    #[test]
    fn question_reports_only_when_unset() {
        assert_eq!(interpolate("${EMPTY?err}", &env), Interpolated::default());
        let result = interpolate("${MISSING?err}", &env);
        assert_eq!(result.unresolved[0].message, Some("err".to_owned()));
    }

    #[test]
    fn question_without_message_has_no_message() {
        assert_eq!(
            interpolate("${MISSING:?}", &env).unresolved,
            vec![unresolved("MISSING", 0)]
        );
    }

    #[test]
    fn colon_plus_alternative_when_set_and_not_empty() {
        assert_eq!(value("${TAG:+alt}"), "alt");
        assert_eq!(value("${EMPTY:+alt}"), "");
        assert_eq!(
            interpolate("${MISSING:+alt}", &env),
            Interpolated::default()
        );
    }

    #[test]
    fn plus_alternative_when_set() {
        assert_eq!(value("${TAG+alt}"), "alt");
        assert_eq!(value("${EMPTY+alt}"), "alt");
        assert_eq!(value("${MISSING+alt}"), "");
    }

    #[test]
    fn nested_default() {
        assert_eq!(value("${MISSING:-${HOST}}"), "db");
        assert_eq!(value("${MISSING:-${NOPE:-deep}}:x"), "deep:x");
        assert_eq!(value("${TAG:-${HOST}}"), "1.2");
    }

    #[test]
    fn unresolved_inside_used_default_is_reported_at_its_offset() {
        assert_eq!(
            interpolate("${MISSING:-${NOPE}}", &env).unresolved,
            vec![unresolved("NOPE", 11)]
        );
    }

    #[test]
    fn unused_default_is_not_evaluated() {
        assert_eq!(interpolate("${TAG:-${NOPE}}", &env).unresolved, vec![]);
    }

    #[test]
    fn offsets_count_bytes() {
        assert_eq!(
            interpolate("é$NOPE", &env).unresolved,
            vec![unresolved("NOPE", 2)]
        );
    }

    #[test]
    fn unterminated_brace_is_kept_literally() {
        assert_eq!(
            interpolate("a${TAG", &env),
            Interpolated {
                value: "a${TAG".to_owned(),
                unresolved: vec![],
            }
        );
    }

    #[test]
    fn invalid_braced_expression_is_kept_literally() {
        assert_eq!(value("${}"), "${}");
        assert_eq!(value("${TAG!x} $TAG"), "${TAG!x} 1.2");
    }
}
