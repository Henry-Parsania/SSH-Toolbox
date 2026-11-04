//! Takes obvious secrets out of text before it is sent to a model: private keys, passwords and
//! tokens written as `name=value`, and well-known token shapes. It can't catch everything (a
//! password in plain prose looks like any other word), so the interface also tells the user what
//! is being sent and to whom.

const HIDDEN: &str = "[hidden]";

/// Words in a name that mark its value as secret.
const SECRET_NAMES: [&str; 12] = [
    "password",
    "passwd",
    "passphrase",
    "secret",
    "token",
    "api_key",
    "apikey",
    "api-key",
    "access_key",
    "private_key",
    "authorization",
    "credential",
];

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '$' | '+' | '/' | '=')
}

/// Whether a word has the shape of a well-known token or password hash.
fn looks_like_token(word: &str) -> bool {
    let w = word.trim_matches(|c| matches!(c, '.' | '=' | '/'));
    let long = |prefix: &str, min: usize| w.starts_with(prefix) && w.len() >= prefix.len() + min;
    // Amazon access key IDs.
    (w.len() == 20 && w.starts_with("AKIA") && w.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()))
        || long("ghp_", 30)
        || long("gho_", 30)
        || long("ghs_", 30)
        || long("github_pat_", 30)
        || long("sk-", 20)
        || long("xoxb-", 20)
        || long("xoxp-", 20)
        || long("AIza", 30)
        // JSON web tokens: three long base64url parts.
        || (w.starts_with("eyJ") && w.matches('.').count() == 2 && w.len() > 40)
        // Password hashes as /etc/shadow keeps them.
        || ["$1$", "$2a$", "$2b$", "$2y$", "$5$", "$6$", "$y$"].iter().any(|p| w.starts_with(p) && w.len() > 20)
}

/// A name such as `DB_PASSWORD`, `"api_key"` or `--token`.
fn is_secret_name(name: &str) -> bool {
    let name = name.trim_matches(|c: char| matches!(c, '"' | '\'' | '-' | ' ' | '\t'));
    if name.is_empty() || name.contains(char::is_whitespace) || name.len() > 64 {
        return false;
    }
    let lower = name.to_ascii_lowercase();
    SECRET_NAMES.iter().any(|s| lower.contains(s))
}

fn redact_words(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        if !word.is_empty() {
            if looks_like_token(word) {
                out.push_str(HIDDEN);
            } else {
                out.push_str(word);
            }
            word.clear();
        }
    };
    for c in line.chars() {
        if is_word_char(c) {
            word.push(c);
        } else {
            flush(&mut word, &mut out);
            out.push(c);
        }
    }
    flush(&mut word, &mut out);
    out
}

fn redact_line(line: &str) -> String {
    // `name=value`, `name: value` and `"name": "value"`.
    if let Some(at) = line.find(['=', ':']) {
        let (name, rest) = line.split_at(at);
        let sep = &rest[..1];
        let value = rest[1..].trim();
        // `export NAME=value`: the name is the last word before the sign.
        let last = name.split_whitespace().last().unwrap_or("");
        if is_secret_name(last) && !value.is_empty() && value != "\"\"" && value != "''" {
            let trailing_comma = if value.ends_with(',') { "," } else { "" };
            return format!("{name}{sep} {HIDDEN}{trailing_comma}");
        }
    }
    // `Authorization: Bearer abc` is caught above; a bare `Bearer abc` is caught here.
    let lower = line.to_ascii_lowercase();
    if let Some(at) = lower.find("bearer ") {
        let head = &line[..at + 7];
        return format!("{head}{HIDDEN}");
    }
    redact_words(line)
}

/// The text with private keys, secret-looking values and known token shapes replaced.
pub fn redact(text: &str) -> String {
    let mut out = Vec::new();
    let mut in_key = false;
    for line in text.lines() {
        if in_key {
            if line.contains("-----END") && line.contains("PRIVATE KEY") {
                in_key = false;
            }
            continue;
        }
        if line.contains("-----BEGIN") && line.contains("PRIVATE KEY") {
            out.push("[private key removed]".to_string());
            if !(line.contains("-----END") && line.contains("PRIVATE KEY")) {
                in_key = true;
            }
            continue;
        }
        out.push(redact_line(line));
    }
    let mut joined = out.join("\n");
    if text.ends_with('\n') {
        joined.push('\n');
    }
    joined
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_keys_are_removed_whole() {
        let text = "before\n-----BEGIN OPENSSH PRIVATE KEY-----\nAAAA\nBBBB\n-----END OPENSSH PRIVATE KEY-----\nafter\n";
        let out = redact(text);
        assert_eq!(out, "before\n[private key removed]\nafter\n");
        let cut = redact("x\n-----BEGIN RSA PRIVATE KEY-----\nAAAA\n");
        assert_eq!(cut, "x\n[private key removed]\n");
    }

    #[test]
    fn values_of_secret_names_are_hidden_but_the_names_stay() {
        assert_eq!(redact("DB_PASSWORD=hunter2"), "DB_PASSWORD= [hidden]");
        assert_eq!(
            redact("  \"api_key\": \"abc123\","),
            "  \"api_key\": [hidden],"
        );
        assert_eq!(
            redact("Authorization: Bearer abc.def"),
            "Authorization: [hidden]"
        );
        assert_eq!(
            redact("curl -H 'x: y' Bearer zzz"),
            "curl -H 'x: y' Bearer [hidden]"
        );
        assert!(!redact("export GITHUB_TOKEN=abc").contains("abc"));
    }

    #[test]
    fn ordinary_text_is_left_alone() {
        for line in [
            "PasswordAuthentication yes",
            "A token is a small thing: ok",
            "total 12",
            "drwxr-xr-x 2 root root 4096 Jan  1 00:00 .",
            "password:",
            "listening on 0.0.0.0:8080",
            "uptime: 3 days",
        ] {
            assert_eq!(redact(line), line, "{line}");
        }
    }

    #[test]
    fn known_token_shapes_are_hidden_anywhere() {
        let ghp = format!("ghp_{}", "a1".repeat(20));
        assert_eq!(
            redact(&format!("pushed with {ghp} ok")),
            "pushed with [hidden] ok"
        );
        assert_eq!(redact("key AKIAABCDEFGHIJKLMNOP end"), "key [hidden] end");
        assert_eq!(redact("sk-abcdefghijklmnopqrstuvwxyz1234"), "[hidden]");
        let jwt = format!("eyJhbGciOiJIUzI1NiJ9.{}.{}", "a".repeat(30), "b".repeat(30));
        assert_eq!(redact(&jwt), "[hidden]");
        assert!(
            !redact("root:$6$saltsaltsalt$abcdefghijklmnopqrstuvwxyz:19000:0")
                .contains("abcdefghij")
        );
    }
}
