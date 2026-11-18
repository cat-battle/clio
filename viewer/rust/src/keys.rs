//! Tiny key-script parser for headless runs: `"j<space><enter><S-right>q"`.
//! Plain characters are typed as-is; `<name>` is a named key, `<S-name>` adds Shift.

use anyhow::{Result, bail};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub fn parse_keys(script: &str) -> Result<Vec<KeyEvent>> {
    let mut out = Vec::new();
    let mut chars = script.chars();
    while let Some(c) = chars.next() {
        if c != '<' {
            let mods = if c.is_uppercase() {
                KeyModifiers::SHIFT
            } else {
                KeyModifiers::NONE
            };
            out.push(KeyEvent::new(KeyCode::Char(c), mods));
            continue;
        }
        let mut name = String::new();
        loop {
            match chars.next() {
                Some('>') => break,
                Some(ch) => name.push(ch),
                None => bail!("unterminated <{name} in key script"),
            }
        }
        out.push(named_key(&name)?);
    }
    Ok(out)
}

fn named_key(name: &str) -> Result<KeyEvent> {
    let (mods, base) = match name.strip_prefix("S-") {
        Some(b) => (KeyModifiers::SHIFT, b),
        None => (KeyModifiers::NONE, name),
    };
    let code = match base.to_ascii_lowercase().as_str() {
        "space" => KeyCode::Char(' '),
        "enter" | "cr" => KeyCode::Enter,
        "esc" => KeyCode::Esc,
        "bs" | "backspace" => KeyCode::Backspace,
        "tab" => KeyCode::Tab,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "lt" => KeyCode::Char('<'),
        "gt" => KeyCode::Char('>'),
        other => bail!("unknown key <{other}>"),
    };
    Ok(KeyEvent::new(code, mods))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_and_named_keys() {
        let k = parse_keys("j<space><S-left>E").unwrap();
        assert_eq!(k.len(), 4);
        assert_eq!(k[0].code, KeyCode::Char('j'));
        assert_eq!(k[1].code, KeyCode::Char(' '));
        assert_eq!(k[2].code, KeyCode::Left);
        assert!(k[2].modifiers.contains(KeyModifiers::SHIFT));
        assert!(k[3].modifiers.contains(KeyModifiers::SHIFT));
        assert!(parse_keys("<bogus>").is_err());
        assert!(parse_keys("<space").is_err());
    }
}
