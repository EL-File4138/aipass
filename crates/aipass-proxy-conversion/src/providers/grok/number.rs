//! Normalize integral JSON decimals lexically, without floating-point rounding.
#[derive(Default)]
pub(super) struct Numbers {
    string: bool,
    escape: bool,
    token: String,
}
impl Numbers {
    pub(super) fn push(&mut self, text: &str) -> Result<String, String> {
        let mut out = String::new();
        for c in text.chars() {
            if !self.token.is_empty() {
                if c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '+' | '-') {
                    self.token.push(c);
                    if self.token.len() > 1024 * 1024 {
                        return Err("Grok numeric argument exceeds limit".into());
                    }
                    continue;
                }
                out.push_str(&self.finish());
            }
            if self.string {
                if self.escape {
                    self.escape = false;
                } else if c == '\\' {
                    self.escape = true;
                } else if c == '"' {
                    self.string = false;
                }
            } else if c == '"' {
                self.string = true;
            } else if c == '-' || c.is_ascii_digit() {
                self.token.push(c);
                continue;
            }
            out.push(c);
        }
        Ok(out)
    }
    pub(super) fn finish(&mut self) -> String {
        let mut token = std::mem::take(&mut self.token);
        if !token.contains(['e', 'E']) {
            if let Some((whole, fraction)) = token.split_once('.') {
                if !fraction.is_empty() && fraction.bytes().all(|c| c == b'0') {
                    token.truncate(whole.len());
                }
            }
        }
        token
    }
}
pub(super) fn whole(text: &str) -> Result<String, String> {
    let mut numbers = Numbers::default();
    let mut out = numbers.push(text)?;
    out.push_str(&numbers.finish());
    Ok(out)
}
