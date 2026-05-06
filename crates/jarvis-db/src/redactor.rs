//! Redactor (Section 15.6).
//!
//! `raw_content` stays exact for forensic replay; `safe_content` is the
//! sanitised view we return to displays / external integrations.
//! Patterns mirror the PRD's defaults: API keys, tokens, passwords,
//! cookies, authorization headers, private keys, e-mail verification
//! codes, phone numbers, ID numbers, and bank cards.

use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Debug, Clone)]
pub struct RedactorPattern {
    pub label: &'static str,
    pub regex: Regex,
}

fn patterns() -> &'static [RedactorPattern] {
    static P: Lazy<Vec<RedactorPattern>> = Lazy::new(|| {
        vec![
            RedactorPattern {
                label: "anthropic_api_key",
                regex: Regex::new(r"sk-ant-[A-Za-z0-9_\-]{20,}").unwrap(),
            },
            RedactorPattern {
                label: "openai_api_key",
                regex: Regex::new(r"sk-[A-Za-z0-9]{20,}").unwrap(),
            },
            RedactorPattern {
                label: "github_token",
                regex: Regex::new(r"gh[pousr]_[A-Za-z0-9]{30,}").unwrap(),
            },
            RedactorPattern {
                label: "bearer_header",
                regex: Regex::new(r"(?i)Authorization:\s*Bearer\s+[A-Za-z0-9._\-]+").unwrap(),
            },
            RedactorPattern {
                label: "cookie_header",
                regex: Regex::new(r"(?i)Cookie:\s*[^\r\n]+").unwrap(),
            },
            RedactorPattern {
                label: "private_key_block",
                regex: Regex::new(
                    r"-----BEGIN (?:RSA|OPENSSH|EC|DSA|PGP)? ?PRIVATE KEY-----[\s\S]*?-----END (?:RSA|OPENSSH|EC|DSA|PGP)? ?PRIVATE KEY-----",
                )
                .unwrap(),
            },
            RedactorPattern {
                label: "password_assignment",
                regex: Regex::new(r#"(?i)(password|pwd|passwd)\s*[:=]\s*"?[^\s"',;]{4,}"#)
                    .unwrap(),
            },
            RedactorPattern {
                label: "verification_code",
                regex: Regex::new(r"(?i)(?:verification|verify|code)[^\d]{0,5}\b\d{4,8}\b")
                    .unwrap(),
            },
            RedactorPattern {
                // Mainland China phone number (11 digit)
                label: "cn_phone",
                regex: Regex::new(r"\b1[3-9]\d{9}\b").unwrap(),
            },
            RedactorPattern {
                label: "cn_id_card",
                // 18-digit ID with optional X check digit.
                regex: Regex::new(r"\b\d{17}[\dXx]\b").unwrap(),
            },
            RedactorPattern {
                label: "credit_card_like",
                regex: Regex::new(r"\b(?:\d[ -]*?){13,19}\b").unwrap(),
            },
            RedactorPattern {
                label: "email_address",
                regex: Regex::new(r"[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}").unwrap(),
            },
        ]
    });
    P.as_slice()
}

/// Single-pass redactor. Returns the sanitised string and the labels
/// of every pattern that matched (each at least once).
pub fn redact(raw: &str) -> (String, Vec<&'static str>) {
    let mut out = raw.to_string();
    let mut hits: Vec<&'static str> = Vec::new();
    for p in patterns() {
        if p.regex.is_match(&out) {
            hits.push(p.label);
            out = p.regex.replace_all(&out, "[REDACTED]").into_owned();
        }
    }
    hits.sort();
    hits.dedup();
    (out, hits)
}
