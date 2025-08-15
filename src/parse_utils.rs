use std::collections::HashMap;

const ARG_LC: &str = "arg";
const ENV_LC: &str = "env";
const EQUALS: char = '=';
const LABEL_LC: &str = "label";

pub fn parse_kv_instruction(ins: &str) -> HashMap<String, String> {
    let toks = extract_tokens_from_instr(ins);
    vec_to_map(&toks)
}

pub fn parse_kv_instruction_opt_val(ins: &str) -> HashMap<String, Option<String>> {
    let toks = extract_tokens_from_instr(ins);
    vec_to_map_opt_val(&toks)
}

fn extract_tokens_from_instr(ins: &str) -> Vec<String> {
    let mut processed: Vec<String> = vec![];

    if let Some(mut toks) = shlex::split(ins) {
        toks.retain(|s| {
            !s.is_empty()
                && s.to_lowercase() != ARG_LC
                && s.to_lowercase() != ENV_LC
                && s.to_lowercase() != LABEL_LC
                && s != "\r"
        });
        let mut prev: Option<String> = None;

        for tok in toks {
            if tok == "=" {
            } else if tok.starts_with(EQUALS) {
                processed.push(tok.strip_prefix(EQUALS).unwrap().to_string());
            } else if tok.ends_with(EQUALS) {
                processed.push(tok.strip_suffix(EQUALS).unwrap().to_string());
            } else if prev.is_some() && prev.as_ref().unwrap().ends_with(EQUALS) {
                processed.push(tok.clone());
            } else if let Some((k, v)) = tok.split_once(EQUALS) {
                processed.push(k.to_string());
                processed.push(v.to_string());
            } else {
                processed.push(tok.clone());
            }

            prev = Some(tok);
        }
    }
    processed
}

fn vec_to_map(v: &[String]) -> HashMap<String, String> {
    let mut res = HashMap::new();
    for chunk in v.chunks(2) {
        match chunk {
            [k, v] => {
                res.insert(k.to_string(), v.to_string());
            }
            [k] => {
                res.insert(k.to_string(), "".to_string());
            }
            _ => unreachable!(),
        }
    }

    res
}

fn vec_to_map_opt_val(v: &[String]) -> HashMap<String, Option<String>> {
    let mut res: HashMap<String, Option<String>> = HashMap::new();
    for chunk in v.chunks(2) {
        match chunk {
            [k, v] => {
                res.insert(k.to_string(), Some(v.to_string()));
            }
            [k] => {
                res.insert(k.to_string(), None);
            }
            _ => unreachable!(),
        }
    }

    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_equal() {
        assert_eq!(
            parse_kv_instruction("ENV NODE_VERSION=22.18.0"),
            HashMap::from([("NODE_VERSION".into(), "22.18.0".into()),])
        );
    }

    #[test]
    fn test_basic_space() {
        assert_eq!(
            parse_kv_instruction("ENV NODE_VERSION=22.18.0"),
            HashMap::from([("NODE_VERSION".into(), "22.18.0".into()),])
        );
    }

    #[test]
    fn test_multiline_equal() {
        let env = r#"
ENV USER=appuser \
    UID= 1000 \
    GID =1001 \
    HOME=/home/appuser
"#;
        assert_eq!(
            parse_kv_instruction(env),
            HashMap::from([
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
ENV USER appuser \
    UID 1000 \
    GID 1000 \
    HOME /home/appuser
"#;
        assert_eq!(
            parse_kv_instruction(env),
            HashMap::from([
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
            parse_kv_instruction("ENV APP_NAME=\"My Application\""),
            HashMap::from([("APP_NAME".into(), "My Application".into()),])
        );
    }

    #[test]
    fn test_var_with_space() {
        assert_eq!(
            parse_kv_instruction("ENV APP_NAME \"My Application\""),
            HashMap::from([("APP_NAME".into(), "My Application".into()),])
        );
    }

    #[test]
    fn test_var_with_equals_sign_in_value() {
        assert_eq!(
            parse_kv_instruction("ENV VAR1 = \"key=value1\" VAR2 = \"another=value2\""),
            HashMap::from([
                ("VAR1".into(), "key=value1".into()),
                ("VAR2".into(), "another=value2".into())
            ])
        );
    }

    #[test]
    fn test_multiline_with_backslashes() {
        let s = r#"
ENV LONG_CONFIG="value1,value2,value3,value4,value5" \
    ANOTHER_CONFIG="test" \
    THIRD_CONFIG="example"
        "#;
        assert_eq!(
            parse_kv_instruction(s),
            HashMap::from([
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
            parse_kv_instruction("ENV EMPTY_VAR="),
            HashMap::from([("EMPTY_VAR".into(), "".into()),])
        );
    }

    #[test]
    fn test_empty_value_space_syntax() {
        assert_eq!(
            parse_kv_instruction("ENV EMPTY_VAR \"\""),
            HashMap::from([("EMPTY_VAR".into(), "".into()),])
        );
    }

    #[test]
    fn test_single_quotes() {
        assert_eq!(
            parse_kv_instruction("ENV MESSAGE='Hello World'"),
            HashMap::from([("MESSAGE".into(), "Hello World".into()),])
        );
    }

    #[test]
    fn test_mixed_quotes_in_value() {
        assert_eq!(
            parse_kv_instruction("ENV JSON='{\"key\": \"value\"}'"),
            HashMap::from([("JSON".into(), "{\"key\": \"value\"}".into()),])
        );
    }

    #[test]
    fn test_escaped_quotes() {
        assert_eq!(
            parse_kv_instruction(r#"ENV MESSAGE="Say \"Hello\"""#),
            HashMap::from([("MESSAGE".into(), "Say \"Hello\"".into()),])
        );
    }

    #[test]
    fn test_special_characters() {
        assert_eq!(
            parse_kv_instruction("ENV SPECIAL=\"!@#$%^&*()_+-=[]{}|;:,.<>?\""),
            HashMap::from([("SPECIAL".into(), "!@#$%^&*()_+-=[]{}|;:,.<>?".into()),])
        );
    }

    #[test]
    fn test_path_with_spaces() {
        assert_eq!(
            parse_kv_instruction("ENV PATH=\"/usr/local/my app/bin:/usr/bin\""),
            HashMap::from([("PATH".into(), "/usr/local/my app/bin:/usr/bin".into()),])
        );
    }

    #[test]
    fn test_value_with_newlines() {
        assert_eq!(
            parse_kv_instruction("ENV MULTILINE=\"line1\\nline2\\nline3\""),
            HashMap::from([("MULTILINE".into(), "line1\\nline2\\nline3".into()),])
        );
    }

    #[test]
    fn test_numeric_values() {
        assert_eq!(
            parse_kv_instruction("ENV PORT=8080 TIMEOUT=30.5 DEBUG=true"),
            HashMap::from([
                ("PORT".into(), "8080".into()),
                ("TIMEOUT".into(), "30.5".into()),
                ("DEBUG".into(), "true".into()),
            ])
        );
    }

    #[test]
    fn test_mixed_syntax_multiple_vars() {
        assert_eq!(
            parse_kv_instruction("ENV VAR1=value1 VAR2 value2 VAR3=\"value 3\""),
            HashMap::from([
                ("VAR1".into(), "value1".into()),
                ("VAR2".into(), "value2".into()),
                ("VAR3".into(), "value 3".into()),
            ])
        );
    }

    #[test]
    fn test_tabs_and_extra_whitespace() {
        assert_eq!(
            parse_kv_instruction("ENV\t\tVAR1=value1    VAR2\t\tvalue2"),
            HashMap::from([
                ("VAR1".into(), "value1".into()),
                ("VAR2".into(), "value2".into()),
            ])
        );
    }

    #[test]
    fn test_case_sensitive_keys() {
        assert_eq!(
            parse_kv_instruction("ENV var=lower VAR=upper Var=mixed"),
            HashMap::from([
                ("var".into(), "lower".into()),
                ("VAR".into(), "upper".into()),
                ("Var".into(), "mixed".into()),
            ])
        );
    }

    #[test]
    fn test_underscore_and_numbers_in_keys() {
        assert_eq!(
            parse_kv_instruction("ENV VAR_1=first VAR2=second _VAR3=third VAR_4_TEST=fourth"),
            HashMap::from([
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
            parse_kv_instruction("ENV API_URL=https://api.example.com:8080/v1?key=value"),
            HashMap::from([(
                "API_URL".into(),
                "https://api.example.com:8080/v1?key=value".into()
            ),])
        );
    }

    #[test]
    fn test_complex_multiline_mixed_syntax() {
        let env = r#"
ENV DATABASE_URL="postgresql://user:pass@localhost/db" \
    REDIS_URL redis://localhost:6379/0 \
    LOG_LEVEL=info \
    FEATURES "feature1,feature2,feature3"
"#;
        assert_eq!(
            parse_kv_instruction(env),
            HashMap::from([
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
    fn test_only_env_keyword() {
        assert_eq!(parse_kv_instruction("ENV"), HashMap::new());
    }

    #[test]
    fn test_env_with_only_whitespace() {
        assert_eq!(parse_kv_instruction("ENV   \t  \n  "), HashMap::new());
    }

    #[test]
    fn test_very_long_value() {
        let long_value = "a".repeat(1000);
        let instruction = format!("ENV LONG_VAR={}", long_value);
        assert_eq!(
            parse_kv_instruction(&instruction),
            HashMap::from([("LONG_VAR".into(), long_value)])
        );
    }

    #[test]
    fn test_leading_and_trailing_whitespace_in_multiline() {
        let env = r#"
    ENV VAR1=value1 \
        VAR2=value2 \
        VAR3=value3    
"#;
        assert_eq!(
            parse_kv_instruction(env),
            HashMap::from([
                ("VAR1".into(), "value1".into()),
                ("VAR2".into(), "value2".into()),
                ("VAR3".into(), "value3".into()),
            ])
        );
    }

    #[test]
    fn test_comment_like_values() {
        assert_eq!(
            parse_kv_instruction("ENV COMMENT=\"# This looks like a comment\""),
            HashMap::from([("COMMENT".into(), "# This looks like a comment".into()),])
        );
    }

    #[test]
    fn test_nested_quotes() {
        assert_eq!(
            parse_kv_instruction("ENV VAR=\"'inner single quotes'\""),
            HashMap::from([("VAR".into(), "'inner single quotes'".into()),])
        );
    }

    #[test]
    fn test_multiple_equals_signs() {
        assert_eq!(
            parse_kv_instruction("ENV VAR1=value=with=equals VAR2=another=value"),
            HashMap::from([
                ("VAR1".into(), "value=with=equals".into()),
                ("VAR2".into(), "another=value".into()),
            ])
        );
    }

    #[test]
    fn test_key_with_special_characters() {
        // Some of these might be invalid Docker syntax, but test parser robustness
        assert_eq!(
            parse_kv_instruction("ENV VAR-NAME=value1 VAR.NAME=value2"),
            HashMap::from([
                ("VAR-NAME".into(), "value1".into()),
                ("VAR.NAME".into(), "value2".into()),
            ])
        );
    }

    #[test]
    fn test_unicode_characters() {
        assert_eq!(
            parse_kv_instruction("ENV MESSAGE=\"Hello 世界 🌍\" EMOJI=🚀"),
            HashMap::from([
                ("MESSAGE".into(), "Hello 世界 🌍".into()),
                ("EMOJI".into(), "🚀".into()),
            ])
        );
    }

    #[test]
    fn test_multiple_backslash_continuations() {
        let env = r#"ENV VAR1=value1 \
\
VAR2=value2"#;
        assert_eq!(
            parse_kv_instruction(env),
            HashMap::from([
                ("VAR1".into(), "value1".into()),
                ("VAR2".into(), "value2".into()),
            ])
        );
    }

    #[test]
    fn test_env_case_insensitive() {
        // Test different cases of ENV keyword
        assert_eq!(
            parse_kv_instruction("env VAR=value"),
            HashMap::from([("VAR".into(), "value".into()),])
        );

        assert_eq!(
            parse_kv_instruction("Env VAR=value"),
            HashMap::from([("VAR".into(), "value".into()),])
        );
    }

    #[test]
    fn test_null_bytes() {
        // Test with null bytes (might not be valid, but test robustness)
        assert_eq!(
            parse_kv_instruction("ENV VAR=value\0withNull"),
            HashMap::from([("VAR".into(), "value\0withNull".into()),])
        );
    }

    #[test]
    fn test_control_characters() {
        assert_eq!(
            parse_kv_instruction("ENV VAR=\"line1\tline2\rline3\""),
            HashMap::from([("VAR".into(), "line1\tline2\rline3".into()),])
        );
    }

    #[test]
    fn test_value_looks_like_env_instruction() {
        assert_eq!(
            parse_kv_instruction("ENV COMMAND=\"ENV INNER=value\""),
            HashMap::from([("COMMAND".into(), "ENV INNER=value".into()),])
        );
    }

    #[test]
    fn test_key_is_numeric() {
        // Invalid variable name but test parser robustness
        assert_eq!(
            parse_kv_instruction("ENV 123=value 456 another"),
            HashMap::from([
                ("123".into(), "value".into()),
                ("456".into(), "another".into()),
            ])
        );
    }

    #[test]
    fn test_key_starts_with_number() {
        // Also invalid in most shells but test robustness
        assert_eq!(
            parse_kv_instruction("ENV 9VAR=value"),
            HashMap::from([("9VAR".into(), "value".into()),])
        );
    }

    #[test]
    fn test_duplicate_keys() {
        assert_eq!(
            parse_kv_instruction("ENV VAR=first VAR=second"),
            HashMap::from([("VAR".into(), "second".into()),]) // last one wins
        );
    }

    #[test]
    fn test_mixed_line_endings() {
        let env = "ENV VAR1=value1 \\\r\n    VAR2=value2 \\\n    VAR3=value3";
        assert_eq!(
            parse_kv_instruction(env),
            HashMap::from([
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
        let instruction = format!("ENV BINARY=\"{}\"", binary_string);

        let result = parse_kv_instruction(&instruction);
        assert_eq!(
            result,
            HashMap::from([("BINARY".into(), "\0\u{1}\u{2}��@".into())])
        );
    }

    #[test]
    fn test_extremely_long_key() {
        let long_key = "A".repeat(10000);
        let instruction = format!("ENV {}=value", long_key);
        assert_eq!(
            parse_kv_instruction(&instruction),
            HashMap::from([(long_key, "value".into())])
        );
    }

    #[test]
    fn test_value_with_multiple_spaces() {
        assert_eq!(
            parse_kv_instruction("ENV VAR=\"value    with    multiple    spaces\""),
            HashMap::from([("VAR".into(), "value    with    multiple    spaces".into()),])
        );
    }

    #[test]
    fn test_embedded_dockerfile_instructions() {
        assert_eq!(
            parse_kv_instruction("ENV DOCKERFILE=\"FROM ubuntu\\nRUN apt-get update\""),
            HashMap::from([(
                "DOCKERFILE".into(),
                "FROM ubuntu\\nRUN apt-get update".into()
            ),])
        );
    }
}
