//! Argument-template expansion for launch profiles (PLAN.md §4.5).
//!
//! A template is a single string like `-L {core} "{rom}"`. It is tokenized
//! shell-style (whitespace-separated, double quotes group), then placeholders
//! are substituted inside each token. Substituted values are never re-split,
//! so a ROM path with spaces stays one argument whether or not the author
//! quoted the placeholder. There is no escape syntax in v1.

#[derive(Debug, PartialEq, thiserror::Error)]
pub enum TemplateError {
    #[error("unclosed double quote in template")]
    UnclosedQuote,
    #[error("unclosed placeholder in template")]
    UnclosedPlaceholder,
    #[error("unknown placeholder {{{0}}}")]
    UnknownPlaceholder(String),
}

/// Expand `template` into an argument vector using `vars` (name → value).
pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<Vec<String>, TemplateError> {
    tokenize(template)?
        .into_iter()
        .map(|token| substitute(&token, vars))
        .collect()
}

fn tokenize(template: &str) -> Result<Vec<String>, TemplateError> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut has_content = false;

    for ch in template.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
                has_content = true; // `""` is a deliberate empty argument
            }
            c if c.is_whitespace() && !in_quotes => {
                if has_content {
                    tokens.push(std::mem::take(&mut current));
                    has_content = false;
                }
            }
            c => {
                current.push(c);
                has_content = true;
            }
        }
    }
    if in_quotes {
        return Err(TemplateError::UnclosedQuote);
    }
    if has_content {
        tokens.push(current);
    }
    Ok(tokens)
}

fn substitute(token: &str, vars: &[(&str, &str)]) -> Result<String, TemplateError> {
    let mut out = String::with_capacity(token.len());
    let mut rest = token;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let end = after.find('}').ok_or(TemplateError::UnclosedPlaceholder)?;
        let name = &after[..end];
        let value = vars
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| *v)
            .ok_or_else(|| TemplateError::UnknownPlaceholder(name.to_string()))?;
        out.push_str(value);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VARS: &[(&str, &str)] = &[
        ("rom", "C:/roms/Super Mario World (USA).sfc"),
        ("core", "snes9x"),
    ];

    #[test]
    fn expands_and_keeps_spaces_in_one_arg() {
        let args = expand("-L {core} {rom}", VARS).unwrap();
        assert_eq!(
            args,
            vec!["-L", "snes9x", "C:/roms/Super Mario World (USA).sfc"]
        );
    }

    #[test]
    fn quoting_groups_but_does_not_change_substitution() {
        let args = expand("--rom \"{rom}\" --fullscreen", VARS).unwrap();
        assert_eq!(
            args,
            vec![
                "--rom",
                "C:/roms/Super Mario World (USA).sfc",
                "--fullscreen"
            ]
        );
        assert_eq!(expand("\"\"", VARS).unwrap(), vec![""]);
    }

    #[test]
    fn errors_are_specific() {
        assert_eq!(
            expand("{romm}", VARS),
            Err(TemplateError::UnknownPlaceholder("romm".into()))
        );
        assert_eq!(
            expand("{rom", VARS),
            Err(TemplateError::UnclosedPlaceholder)
        );
        assert_eq!(expand("\"{rom}", VARS), Err(TemplateError::UnclosedQuote));
    }
}
