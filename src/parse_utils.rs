use std::collections::BTreeMap;

const EQUALS: char = '=';

/// Parse `KEY=value`, `KEY value`, and space-around-equals forms from an
/// ARG, ENV, or LABEL argument string.
///
/// `parse-dockerfile` stores those arguments as one unescaped string rather
/// than structured pairs. A missing value is `None` for ARG (a redeclaration
/// with no default) and `Some("")` for ENV and LABEL.
pub fn parse_kv_pairs(arguments: &str, missing_is_empty: bool) -> BTreeMap<String, Option<String>> {
    let mut pairs = BTreeMap::new();
    for (key, value) in tokenize_pairs(arguments) {
        let stored = match value {
            Some(value) => Some(value),
            None if missing_is_empty => Some(String::new()),
            None => None,
        };
        pairs.insert(key, stored);
    }
    pairs
}

/// Keep the first default when a later instruction repeats a key with no value.
///
/// Docker lets a stage redeclare a global ARG (`ARG VERSION` after
/// `ARG VERSION=1`) so the default stays in scope. A later instruction that
/// sets a value replaces the earlier one.
pub fn merge_kv_pairs(
    into: &mut BTreeMap<String, Option<String>>,
    pairs: BTreeMap<String, Option<String>>,
) {
    for (key, value) in pairs {
        match into.get(&key) {
            Some(Some(_)) if value.is_none() => {}
            _ => {
                into.insert(key, value);
            }
        }
    }
}

fn tokenize_pairs(arguments: &str) -> Vec<(String, Option<String>)> {
    let Some(tokens) = shlex::split(arguments) else {
        return Vec::new();
    };

    let mut processed = Vec::new();
    let mut prev_was_trailing_equals = false;
    for token in tokens {
        if token.is_empty() || token == "\r" {
            prev_was_trailing_equals = false;
            continue;
        }
        if token == "=" {
            prev_was_trailing_equals = true;
            continue;
        }
        if let Some(rest) = token.strip_prefix(EQUALS) {
            processed.push(rest.to_string());
            prev_was_trailing_equals = false;
            continue;
        }
        if let Some(rest) = token.strip_suffix(EQUALS) {
            processed.push(rest.to_string());
            prev_was_trailing_equals = true;
            continue;
        }
        if prev_was_trailing_equals {
            processed.push(token);
            prev_was_trailing_equals = false;
            continue;
        }
        if let Some((key, value)) = token.split_once(EQUALS) {
            processed.push(key.to_string());
            processed.push(value.to_string());
        } else {
            processed.push(token);
        }
        prev_was_trailing_equals = false;
    }

    processed
        .chunks(2)
        .filter_map(|chunk| match chunk {
            [key, value] => Some((key.clone(), Some(value.clone()))),
            [key] => Some((key.clone(), None)),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_map(arguments: &str) -> BTreeMap<String, String> {
        parse_kv_pairs(arguments, true)
            .into_iter()
            .map(|(key, value)| (key, value.unwrap_or_default()))
            .collect()
    }

    #[test]
    fn test_basic_equal() {
        assert_eq!(
            env_map("NODE_VERSION=22.18.0"),
            BTreeMap::from([("NODE_VERSION".into(), "22.18.0".into()),])
        );
    }

    #[test]
    fn test_multiline_equal() {
        let env = r#"
USER=appuser \
    UID= 1000 \
    GID =1001 \
    HOME=/home/appuser
"#;
        assert_eq!(
            env_map(env),
            BTreeMap::from([
                ("USER".into(), "appuser".into()),
                ("UID".into(), "1000".into()),
                ("GID".into(), "1001".into()),
                ("HOME".into(), "/home/appuser".into())
            ])
        );
    }

    #[test]
    fn test_multiline_space() {
        let env = r#"
USER appuser \
    UID 1000 \
    GID 1000 \
    HOME /home/appuser
"#;
        assert_eq!(
            env_map(env),
            BTreeMap::from([
                ("USER".into(), "appuser".into()),
                ("UID".into(), "1000".into()),
                ("GID".into(), "1000".into()),
                ("HOME".into(), "/home/appuser".into())
            ])
        );
    }

    #[test]
    fn test_var_with_space_equals() {
        assert_eq!(
            env_map("APP_NAME=\"My Application\""),
            BTreeMap::from([("APP_NAME".into(), "My Application".into()),])
        );
    }

    #[test]
    fn test_var_with_space() {
        assert_eq!(
            env_map("APP_NAME \"My Application\""),
            BTreeMap::from([("APP_NAME".into(), "My Application".into()),])
        );
    }

    #[test]
    fn test_var_with_equals_sign_in_value() {
        assert_eq!(
            env_map("VAR1 = \"key=value1\" VAR2 = \"another=value2\""),
            BTreeMap::from([
                ("VAR1".into(), "key=value1".into()),
                ("VAR2".into(), "another=value2".into())
            ])
        );
    }

    #[test]
    fn test_multiline_with_backslashes() {
        let s = r#"
LONG_CONFIG="value1,value2,value3,value4,value5" \
    ANOTHER_CONFIG="test" \
    THIRD_CONFIG="example"
        "#;
        assert_eq!(
            env_map(s),
            BTreeMap::from([
                (
                    "LONG_CONFIG".into(),
                    "value1,value2,value3,value4,value5".into()
                ),
                ("ANOTHER_CONFIG".into(), "test".into()),
                ("THIRD_CONFIG".into(), "example".into()),
            ])
        );
    }

    #[test]
    fn test_empty_value() {
        assert_eq!(
            env_map("EMPTY_VAR="),
            BTreeMap::from([("EMPTY_VAR".into(), "".into()),])
        );
    }

    #[test]
    fn test_empty_value_space_syntax() {
        assert_eq!(
            env_map("EMPTY_VAR \"\""),
            BTreeMap::from([("EMPTY_VAR".into(), "".into()),])
        );
    }

    #[test]
    fn test_single_quotes() {
        assert_eq!(
            env_map("MESSAGE='Hello World'"),
            BTreeMap::from([("MESSAGE".into(), "Hello World".into()),])
        );
    }

    #[test]
    fn test_mixed_quotes_in_value() {
        assert_eq!(
            env_map("JSON='{\"key\": \"value\"}'"),
            BTreeMap::from([("JSON".into(), "{\"key\": \"value\"}".into()),])
        );
    }

    #[test]
    fn test_escaped_quotes() {
        assert_eq!(
            env_map(r#"MESSAGE="Say \"Hello\"""#),
            BTreeMap::from([("MESSAGE".into(), "Say \"Hello\"".into()),])
        );
    }

    #[test]
    fn test_special_characters() {
        assert_eq!(
            env_map("SPECIAL=\"!@#$%^&*()_+-=[]{}|;:,.<>?\""),
            BTreeMap::from([("SPECIAL".into(), "!@#$%^&*()_+-=[]{}|;:,.<>?".into()),])
        );
    }

    #[test]
    fn test_path_with_spaces() {
        assert_eq!(
            env_map("PATH=\"/usr/local/my app/bin:/usr/bin\""),
            BTreeMap::from([("PATH".into(), "/usr/local/my app/bin:/usr/bin".into()),])
        );
    }

    #[test]
    fn test_value_with_newlines() {
        assert_eq!(
            env_map("MULTILINE=\"line1\\nline2\\nline3\""),
            BTreeMap::from([("MULTILINE".into(), "line1\\nline2\\nline3".into()),])
        );
    }

    #[test]
    fn test_numeric_values() {
        assert_eq!(
            env_map("PORT=8080 TIMEOUT=30.5 DEBUG=true"),
            BTreeMap::from([
                ("PORT".into(), "8080".into()),
                ("TIMEOUT".into(), "30.5".into()),
                ("DEBUG".into(), "true".into()),
            ])
        );
    }

    #[test]
    fn test_mixed_syntax_multiple_vars() {
        assert_eq!(
            env_map("VAR1=value1 VAR2 value2 VAR3=\"value 3\""),
            BTreeMap::from([
                ("VAR1".into(), "value1".into()),
                ("VAR2".into(), "value2".into()),
                ("VAR3".into(), "value 3".into()),
            ])
        );
    }

    #[test]
    fn test_tabs_and_extra_whitespace() {
        assert_eq!(
            env_map("VAR1=value1    VAR2\t\tvalue2"),
            BTreeMap::from([
                ("VAR1".into(), "value1".into()),
                ("VAR2".into(), "value2".into()),
            ])
        );
    }

    #[test]
    fn test_case_sensitive_keys() {
        assert_eq!(
            env_map("var=lower VAR=upper Var=mixed"),
            BTreeMap::from([
                ("var".into(), "lower".into()),
                ("VAR".into(), "upper".into()),
                ("Var".into(), "mixed".into()),
            ])
        );
    }

    #[test]
    fn test_underscore_and_numbers_in_keys() {
        assert_eq!(
            env_map("VAR_1=first VAR2=second _VAR3=third VAR_4_TEST=fourth"),
            BTreeMap::from([
                ("VAR_1".into(), "first".into()),
                ("VAR2".into(), "second".into()),
                ("_VAR3".into(), "third".into()),
                ("VAR_4_TEST".into(), "fourth".into()),
            ])
        );
    }

    #[test]
    fn test_url_values() {
        assert_eq!(
            env_map("API_URL=https://api.example.com:8080/v1?key=value"),
            BTreeMap::from([(
                "API_URL".into(),
                "https://api.example.com:8080/v1?key=value".into()
            ),])
        );
    }

    #[test]
    fn test_complex_multiline_mixed_syntax() {
        let env = r#"
DATABASE_URL="postgresql://user:pass@localhost/db" \
    REDIS_URL redis://localhost:6379/0 \
    LOG_LEVEL=info \
    FEATURES "feature1,feature2,feature3"
"#;
        assert_eq!(
            env_map(env),
            BTreeMap::from([
                (
                    "DATABASE_URL".into(),
                    "postgresql://user:pass@localhost/db".into()
                ),
                ("REDIS_URL".into(), "redis://localhost:6379/0".into()),
                ("LOG_LEVEL".into(), "info".into()),
                ("FEATURES".into(), "feature1,feature2,feature3".into()),
            ])
        );
    }

    #[test]
    fn test_only_keyword_removed_by_caller() {
        assert_eq!(env_map(""), BTreeMap::new());
    }

    #[test]
    fn test_whitespace_only() {
        assert_eq!(env_map("   \t  \n  "), BTreeMap::new());
    }

    #[test]
    fn test_very_long_value() {
        let long_value = "a".repeat(1000);
        let instruction = format!("LONG_VAR={long_value}");
        assert_eq!(
            env_map(&instruction),
            BTreeMap::from([("LONG_VAR".into(), long_value)])
        );
    }

    #[test]
    fn test_leading_and_trailing_whitespace_in_multiline() {
        let env = r#"
    VAR1=value1 \
        VAR2=value2 \
        VAR3=value3    
"#;
        assert_eq!(
            env_map(env),
            BTreeMap::from([
                ("VAR1".into(), "value1".into()),
                ("VAR2".into(), "value2".into()),
                ("VAR3".into(), "value3".into()),
            ])
        );
    }

    #[test]
    fn test_comment_like_values() {
        assert_eq!(
            env_map("COMMENT=\"# This looks like a comment\""),
            BTreeMap::from([("COMMENT".into(), "# This looks like a comment".into()),])
        );
    }

    #[test]
    fn test_nested_quotes() {
        assert_eq!(
            env_map("VAR=\"'inner single quotes'\""),
            BTreeMap::from([("VAR".into(), "'inner single quotes'".into()),])
        );
    }

    #[test]
    fn test_multiple_equals_signs() {
        assert_eq!(
            env_map("VAR1=value=with=equals VAR2=another=value"),
            BTreeMap::from([
                ("VAR1".into(), "value=with=equals".into()),
                ("VAR2".into(), "another=value".into()),
            ])
        );
    }

    #[test]
    fn test_key_with_special_characters() {
        assert_eq!(
            env_map("VAR-NAME=value1 VAR.NAME=value2"),
            BTreeMap::from([
                ("VAR-NAME".into(), "value1".into()),
                ("VAR.NAME".into(), "value2".into()),
            ])
        );
    }

    #[test]
    fn test_unicode_characters() {
        assert_eq!(
            env_map("MESSAGE=\"Hello 世界 🌍\" EMOJI=🚀"),
            BTreeMap::from([
                ("MESSAGE".into(), "Hello 世界 🌍".into()),
                ("EMOJI".into(), "🚀".into()),
            ])
        );
    }

    #[test]
    fn test_multiple_backslash_continuations() {
        let env = r#"VAR1=value1 \
\
VAR2=value2"#;
        assert_eq!(
            env_map(env),
            BTreeMap::from([
                ("VAR1".into(), "value1".into()),
                ("VAR2".into(), "value2".into()),
            ])
        );
    }

    #[test]
    fn test_null_bytes() {
        assert_eq!(
            env_map("VAR=value\0withNull"),
            BTreeMap::from([("VAR".into(), "value\0withNull".into()),])
        );
    }

    #[test]
    fn test_control_characters() {
        assert_eq!(
            env_map("VAR=\"line1\tline2\rline3\""),
            BTreeMap::from([("VAR".into(), "line1\tline2\rline3".into()),])
        );
    }

    #[test]
    fn test_value_looks_like_env_instruction() {
        assert_eq!(
            env_map("COMMAND=\"ENV INNER=value\""),
            BTreeMap::from([("COMMAND".into(), "ENV INNER=value".into()),])
        );
    }

    #[test]
    fn test_key_is_numeric() {
        assert_eq!(
            env_map("123=value 456 another"),
            BTreeMap::from([
                ("123".into(), "value".into()),
                ("456".into(), "another".into()),
            ])
        );
    }

    #[test]
    fn test_key_starts_with_number() {
        assert_eq!(
            env_map("9VAR=value"),
            BTreeMap::from([("9VAR".into(), "value".into()),])
        );
    }

    #[test]
    fn test_duplicate_keys() {
        assert_eq!(
            env_map("VAR=first VAR=second"),
            BTreeMap::from([("VAR".into(), "second".into()),])
        );
    }

    #[test]
    fn test_mixed_line_endings() {
        let env = "VAR1=value1 \\\r\n    VAR2=value2 \\\n    VAR3=value3";
        assert_eq!(
            env_map(env),
            BTreeMap::from([
                ("VAR1".into(), "value1".into()),
                ("VAR2".into(), "value2".into()),
                ("VAR3".into(), "value3".into()),
            ])
        );
    }

    #[test]
    fn test_binary_data_in_value() {
        let binary_data = vec![0u8, 1, 2, 255, 128, 64];
        let binary_string = String::from_utf8_lossy(&binary_data);
        let instruction = format!("BINARY=\"{binary_string}\"");

        let result = env_map(&instruction);
        assert_eq!(
            result,
            BTreeMap::from([("BINARY".into(), "\0\u{1}\u{2}��@".into())])
        );
    }

    #[test]
    fn test_extremely_long_key() {
        let long_key = "A".repeat(10000);
        let instruction = format!("{long_key}=value");
        assert_eq!(
            env_map(&instruction),
            BTreeMap::from([(long_key, "value".into())])
        );
    }

    #[test]
    fn test_value_with_multiple_spaces() {
        assert_eq!(
            env_map("VAR=\"value    with    multiple    spaces\""),
            BTreeMap::from([("VAR".into(), "value    with    multiple    spaces".into()),])
        );
    }

    #[test]
    fn test_embedded_dockerfile_instructions() {
        assert_eq!(
            env_map("DOCKERFILE=\"FROM ubuntu\\nRUN apt-get update\""),
            BTreeMap::from([(
                "DOCKERFILE".into(),
                "FROM ubuntu\\nRUN apt-get update".into()
            ),])
        );
    }

    #[test]
    fn test_arg_redeclaration_keeps_first_default() {
        let mut args = BTreeMap::new();
        merge_kv_pairs(&mut args, parse_kv_pairs("VERSION=1", false));
        merge_kv_pairs(&mut args, parse_kv_pairs("VERSION", false));
        assert_eq!(args, BTreeMap::from([("VERSION".into(), Some("1".into()))]));
    }

    #[test]
    fn test_arg_redeclaration_replaces_when_value_is_set() {
        let mut args = BTreeMap::new();
        merge_kv_pairs(&mut args, parse_kv_pairs("VERSION=1", false));
        merge_kv_pairs(&mut args, parse_kv_pairs("VERSION=2", false));
        assert_eq!(args, BTreeMap::from([("VERSION".into(), Some("2".into()))]));
    }
}
