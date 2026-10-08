//! PII and secret redaction before anything leaves the device, and the secret patterns
//! the hard rules look for. Hand-rolled scanners (no regex dependency in the core).

/// Replaces emails, phone numbers, card-like numbers, CPF and API-key-looking tokens.
pub fn redact(text: &str) -> String {
    text.split_inclusive(|c: char| c.is_whitespace())
        .map(|chunk| {
            let (word, ws) = split_trailing_ws(chunk);
            let core = word.trim_matches(|c: char| ",.;:!?()[]{}<>\"'".contains(c));
            let replacement = classify_token(core);
            match replacement {
                // Emails keep their domain: "where does this go" is the exfiltration signal,
                // the mailbox name is the personal part.
                Some("[email]") => format!(
                    "{}{}",
                    word.replacen(
                        core,
                        &format!("[user]@{}", core.split_once('@').map(|x| x.1).unwrap_or("")),
                        1
                    ),
                    ws
                ),
                Some(tag) => format!("{}{}", word.replacen(core, tag, 1), ws),
                None => chunk.to_string(),
            }
        })
        .collect::<String>()
        .pipe(redact_digit_runs)
}

trait Pipe: Sized {
    fn pipe<F: FnOnce(Self) -> Self>(self, f: F) -> Self {
        f(self)
    }
}
impl Pipe for String {}

fn split_trailing_ws(s: &str) -> (&str, &str) {
    let i = s.trim_end().len();
    (&s[..i], &s[i..])
}

fn classify_token(t: &str) -> Option<&'static str> {
    if t.is_empty() {
        return None;
    }
    if is_email(t) {
        return Some("[email]");
    } // replaced below, keeping the domain
    if looks_like_secret(t) {
        return Some("[secret]");
    }
    None
}

fn is_email(t: &str) -> bool {
    let Some((user, host)) = t.split_once('@') else {
        return false;
    };
    !user.is_empty()
        && host.contains('.')
        && !host.starts_with('.')
        && !host.ends_with('.')
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
}

/// sk-…, ghp_…, AKIA…, xoxb-…, long base64/hex blobs.
pub fn looks_like_secret(t: &str) -> bool {
    let prefixes = [
        "sk-",
        "sk_live_",
        "pk_live_",
        "ghp_",
        "gho_",
        "github_pat_",
        "xoxb-",
        "xoxp-",
        "AKIA",
        "AIza",
        "eyJhbGci",
    ];
    if prefixes.iter().any(|p| t.starts_with(p)) && t.len() >= 16 {
        return true;
    }
    let long = t.len() >= 32
        && t.chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_+/=".contains(c));
    let mixed = t.chars().any(|c| c.is_ascii_digit()) && t.chars().any(|c| c.is_ascii_alphabetic());
    long && mixed
}

/// Text that contains a credential (hard rule, no model call).
pub fn contains_secret(text: &str) -> bool {
    text.contains("BEGIN PRIVATE KEY")
        || text.contains("BEGIN RSA PRIVATE KEY")
        || text.contains("BEGIN OPENSSH PRIVATE KEY")
        || text
            .split(|c: char| c.is_whitespace() || "\"'`,;()[]{}<>=".contains(c))
            .any(looks_like_secret)
}

/// Runs of 9+ digits (allowing spaces, dots, dashes, parentheses, a leading +): phones,
/// card numbers, CPF. Card numbers that pass Luhn become [card].
fn redact_digit_runs(s: String) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_digit()
            || (chars[i] == '+' && chars.get(i + 1).is_some_and(|c| c.is_ascii_digit()))
        {
            let start = i;
            let mut j = i;
            let mut digits = String::new();
            while j < chars.len() && (chars[j].is_ascii_digit() || " .-()+".contains(chars[j])) {
                if chars[j].is_ascii_digit() {
                    digits.push(chars[j]);
                }
                // Stop at a space not followed by a digit (end of the number).
                if chars[j] == ' '
                    && !chars
                        .get(j + 1)
                        .is_some_and(|c| c.is_ascii_digit() || *c == '(')
                {
                    break;
                }
                j += 1;
            }
            if digits.len() >= 9 {
                let tag = if (13..=19).contains(&digits.len()) && luhn(&digits) {
                    "[card]"
                } else if digits.len() == 11
                    && !chars[start..j].contains(&'+')
                    && chars[start..j].iter().filter(|c| **c == '.').count() == 2
                {
                    "[cpf]"
                } else {
                    "[phone]"
                };
                out.push_str(tag);
                // Keep a trailing space we may have consumed.
                if j > start && chars[j - 1] == ' ' {
                    out.push(' ');
                }
                i = j;
                continue;
            }
            out.extend(&chars[start..j.max(start + 1)]);
            i = j.max(start + 1);
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

pub fn luhn(digits: &str) -> bool {
    let mut sum = 0;
    for (i, c) in digits.chars().rev().enumerate() {
        let mut d = c.to_digit(10).unwrap_or(0);
        if i % 2 == 1 {
            d *= 2;
            if d > 9 {
                d -= 9;
            }
        }
        sum += d;
    }
    sum % 10 == 0
}

/// Card number anywhere in the text (hard rule for outbound messages).
pub fn contains_card(text: &str) -> bool {
    redact_digit_runs(text.to_string()).contains("[card]")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_contact_details_and_secrets() {
        let r = redact("mail me at ana.souza@gmail.com or +55 11 98765-4321, card 4242 4242 4242 4242, key sk-proj-abcdefghijklmnop1234");
        assert!(!r.contains("ana.souza"), "{r}");
        assert!(!r.contains("98765"), "{r}");
        assert!(r.contains("[card]"), "{r}");
        assert!(r.contains("[secret]"), "{r}");
        assert!(r.contains("[user]@gmail.com"), "{r}");
    }

    #[test]
    fn leaves_ordinary_text_alone() {
        let s = "8:15 Marina · Hayes Valley, 9.5 mi, 1,100 ft";
        assert_eq!(redact(s), s);
    }
}
