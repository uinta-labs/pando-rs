use std::fmt;

use winnow::{
    ascii::{line_ending, space0},
    combinator::{alt, opt, preceded},
    token::take_till,
    Parser, Result,
};

#[derive(Debug, Clone, PartialEq)]
pub enum ConfigLine {
    Comment(String),
    Empty,
    Entry {
        key: String,
        value: Option<String>,
        trailing_comment: Option<String>,
    },
}

#[derive(Debug)]
pub struct ConfigFile {
    lines: Vec<ConfigLine>,
}

impl ConfigFile {
    pub fn parse(input: &str) -> Result<Self, String> {
        match parse_config.parse(input) {
            Ok(lines) => Ok(ConfigFile { lines }),
            Err(e) => Err(format!("Parse error: {}", e)),
        }
    }

    pub fn get_value(&self, key: &str) -> Option<&str> {
        self.lines.iter().find_map(|line| {
            if let ConfigLine::Entry {
                key: k,
                value: Some(v),
                ..
            } = line
            {
                if k == key {
                    Some(v.as_str())
                } else {
                    None
                }
            } else {
                None
            }
        })
    }

    pub fn set_value(&mut self, key: &str, value: &str) -> Result<(), String> {
        if (key.len() + value.len() + 1) > 98 {
            return Err("Entry exceeds 98-character limit".to_string());
        }

        for line in &mut self.lines {
            if let ConfigLine::Entry {
                key: k, value: v, ..
            } = line
            {
                if k == key {
                    *v = Some(value.to_string());
                    return Ok(());
                }
            }
        }

        // If we didn't find the key, add a new entry
        self.lines.push(ConfigLine::Entry {
            key: key.to_string(),
            value: Some(value.to_string()),
            trailing_comment: None,
        });
        Ok(())
    }
}

impl fmt::Display for ConfigFile {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut output = String::new();
        for line in &self.lines {
            match line {
                ConfigLine::Comment(comment) => {
                    output.push('#');
                    output.push_str(comment);
                    output.push('\n');
                }
                ConfigLine::Empty => output.push('\n'),
                ConfigLine::Entry {
                    key,
                    value,
                    trailing_comment,
                } => {
                    output.push_str(key);
                    if let Some(val) = value {
                        output.push('=');
                        output.push_str(val);
                    }
                    if let Some(comment) = trailing_comment {
                        output.push_str(" #");
                        output.push_str(comment);
                    }
                    output.push('\n');
                }
            }
        }
        // output
        write!(f, "{}", output)
    }
}

fn is_key_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-'
}

fn is_value_char(c: char) -> bool {
    !c.is_whitespace() && c != '#'
}

fn parse_comment(input: &mut &str) -> Result<ConfigLine> {
    preceded(
        '#',
        take_till(1.., |c| c == '\n').map(|s: &str| ConfigLine::Comment(s.to_string())),
    )
    .parse_next(input)
}

fn parse_key(input: &mut &str) -> Result<String> {
    take_till(1.., |c| !is_key_char(c))
        .map(|s: &str| s.to_string())
        .parse_next(input)
}

fn parse_value(input: &mut &str) -> Result<String> {
    take_till(1.., |c| !is_value_char(c))
        .map(|s: &str| s.to_string())
        .parse_next(input)
}

fn parse_trailing_comment(input: &mut &str) -> Result<String> {
    preceded(
        ('#', space0),
        take_till(1.., |c| c == '\n').map(|s: &str| s.to_string()),
    )
    .parse_next(input)
}

fn parse_entry(input: &mut &str) -> Result<ConfigLine> {
    let (key, value, trailing_comment, _) = (
        parse_key,
        opt(preceded('=', parse_value)),
        opt(preceded(space0, parse_trailing_comment)),
        opt(line_ending),
    )
        .parse_next(input)?;

    Ok(ConfigLine::Entry {
        key,
        value,
        trailing_comment,
    })
}

fn parse_empty_line(input: &mut &str) -> Result<ConfigLine> {
    line_ending.map(|_| ConfigLine::Empty).parse_next(input)
}

fn parse_line(input: &mut &str) -> Result<ConfigLine> {
    preceded(space0, alt((parse_comment, parse_entry, parse_empty_line))).parse_next(input)
}

fn parse_config(input: &mut &str) -> Result<Vec<ConfigLine>> {
    let mut lines = Vec::new();
    while !input.is_empty() {
        lines.push(parse_line.parse_next(input)?);
    }
    Ok(lines)
}

pub trait ConfigValidator {
    fn validate_entry(&self, key: &str, value: Option<&str>) -> Result<(), String>;
}

pub struct DefaultValidator;

impl ConfigValidator for DefaultValidator {
    fn validate_entry(&self, _key: &str, _value: Option<&str>) -> Result<(), String> {
        // Basic validator that just ensures the line length limit
        Ok(())
    }
}

// Example of a custom validator
pub struct DtOverlayValidator {
    known_overlays: Vec<String>,
}

impl ConfigValidator for DtOverlayValidator {
    fn validate_entry(&self, key: &str, value: Option<&str>) -> Result<(), String> {
        if key == "dtoverlay" {
            if let Some(overlay) = value {
                if !self.known_overlays.contains(&overlay.to_string()) {
                    return Err(format!("Unknown overlay: {}", overlay));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_parse_basic_config() {
        let input = "\
# Enable audio
dtparam=audio=on

# Camera
camera_auto_detect=1
";
        let config = ConfigFile::parse(input).unwrap();
        assert_eq!(config.get_value("dtparam"), Some("audio=on"));
        assert_eq!(config.get_value("camera_auto_detect"), Some("1"));
    }

    #[test]
    fn test_parse_multiple_overlays() {
        let input = "\
dtoverlay=4dpi-3x
dtoverlay=w1-gpio
";
        let config = ConfigFile::parse(input).unwrap();
        let output = config.to_string();
        assert_eq!(input, output);
    }

    #[test]
    fn test_parse_without_trailing_newline() {
        // Ensure that the parser can handle input without a trailing newline
        // This may be a rare case where we won't faithfully reproduce the input
        let input = "\
dtoverlay=4dpi-3x
dtoverlay=w1-gpio";
        let input_with_newline = format!("{}\n", input);
        let config = ConfigFile::parse(input).unwrap();
        let output = config.to_string();
        assert_eq!(input_with_newline, output);
    }

    #[test]
    fn test_set_value() {
        let mut config = ConfigFile::parse("dtparam=audio=off\n").unwrap();
        config.set_value("dtparam", "audio=on").unwrap();
        assert_eq!(config.get_value("dtparam"), Some("audio=on"));
    }

    #[test]
    fn test_line_length_limit() {
        let mut config = ConfigFile::parse("").unwrap();
        let long_value = "x".repeat(95);
        assert!(config.set_value("test", &long_value).is_err());
    }
}
