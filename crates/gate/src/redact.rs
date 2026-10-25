//! PII and secret redaction before anything leaves the device, and the secret patterns
//! the hard rules look for. Hand-rolled scanners (no regex dependency in the core).

/// Replaces emails, phone numbers, card-like numbers, CPF and API-key-looking tokens.
#[must_use]
pub fn redact(text: &str) -> String {
    let redacted = text
        .split_inclusive(|c: char| c.is_whitespace())
        .map(|chunk| {
            let (word, ws) = split_trailing_ws(chunk);
            let core = word.trim_matches(|c: char| ",.;:!?()[]{}<>\"'".contains(c));
            let replacement = classify_token(core);
            match replacement {
                // Emails keep their domain: "where does this go" is the exfiltration signal,
                // the mailbox name is the personal part.
                Some(Token::Email(domain)) => format!(
                    "{}{}",
                    word.replacen(core, &format!("[user]@{domain}"), 1),
                    ws
                ),
                Some(Token::Secret) => format!("{}{}", word.replacen(core, "[secret]", 1), ws),
                None => chunk.to_string(),
            }
        })
        .collect::<String>();
    redact_digit_runs(&redacted)
}

fn split_trailing_ws(s: &str) -> (&str, &str) {
    let i = s.trim_end().len();
    (&s[..i], &s[i..])
}

enum Token<'a> {
    Email(&'a str),
    Secret,
}

fn classify_token(t: &str) -> Option<Token<'_>> {
    if let Some(domain) = email_domain(t) {
        return Some(Token::Email(domain));
    }
    looks_like_secret(t).then_some(Token::Secret)
}

fn email_domain(t: &str) -> Option<&str> {
    let (user, host) = t.split_once('@')?;
    (!user.is_empty()
        && host.contains('.')
        && !host.starts_with('.')
        && !host.ends_with('.')
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-'))
    .then_some(host)
}

/// sk-…, ghp_…, AKIA…, xoxb-…, long base64/hex blobs.
#[must_use]
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
#[must_use]
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
fn redact_digit_runs(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_digit()
            || (chars[i] == '+' && chars.get(i + 1).is_some_and(char::is_ascii_digit))
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

/// Validates a nonempty ASCII digit sequence. Formatting belongs to the caller.
/// Keep only the remainder so input length cannot overflow the checksum sum.
#[must_use]
pub fn luhn(digits: &str) -> bool {
    if digits.is_empty() {
        return false;
    }
    let mut sum = 0_u8;
    for (i, byte) in digits.bytes().rev().enumerate() {
        if !byte.is_ascii_digit() {
            return false;
        }
        let mut digit = byte - b'0';
        if i % 2 == 1 {
            digit *= 2;
            if digit > 9 {
                digit -= 9;
            }
        }
        // Both terms are at most nine; this addition cannot overflow u8.
        sum = (sum + digit) % 10;
    }
    sum == 0
}

/// Card number anywhere in the text (hard rule for outbound messages).
#[must_use]
pub fn contains_card(text: &str) -> bool {
    redact_digit_runs(text).contains("[card]")
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

    #[test]
    fn luhn_requires_nonempty_ascii_digits_without_substituting_zero() {
        for invalid in [
            "",
            "x",
            "0x",
            "4242 4242 4242 4242",
            "４２４２",
            "٤٢٤٢",
            "0\0",
        ] {
            assert!(!luhn(invalid), "invalid checksum input: {invalid:?}");
        }
        assert!(luhn("0"));
        assert!(luhn("4242424242424242"));
        assert!(!luhn("4242424242424243"));
    }

    #[test]
    fn luhn_keeps_only_the_checksum_remainder_for_long_inputs() {
        let valid = "4242424242424242".repeat(4096);
        assert!(luhn(&valid));
        let mut invalid = valid;
        invalid.pop();
        invalid.push('3');
        assert!(!luhn(&invalid));
    }

    #[test]
    fn redaction_preserves_unicode_punctuation_whitespace_and_exact_domain() {
        assert_eq!(
            redact("Olá, (ç@sub.example.org),\tana@example.com!\r\n"),
            "Olá, ([user]@sub.example.org),\t[user]@example.com!\r\n",
        );
        for invalid in [
            "@example.org",
            "a@.example.org",
            "a@example.",
            "a@exämple.org",
        ] {
            assert_eq!(redact(invalid), invalid);
        }
        assert_eq!(
            redact("card 4242 4242 4242 4242, CPF 123.456.789-09"),
            "card [card], CPF [cpf]"
        );
    }
}
