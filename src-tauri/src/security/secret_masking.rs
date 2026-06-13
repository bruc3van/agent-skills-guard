use lazy_regex::{lazy_regex, Lazy};
use regex::Regex;

static AWS_KEY_RE: Lazy<Regex> = lazy_regex!(r"(AKIA|ASIA)[A-Z0-9]{16}");
static GITHUB_TOKEN_RE: Lazy<Regex> = lazy_regex!(r"(gh[opusr]_[a-zA-Z0-9]{36}|github_pat_[a-zA-Z0-9_]{36,})");
static PRIVATE_KEY_RE: Lazy<Regex> = lazy_regex!(
    r"-----BEGIN\s+(?:RSA|OPENSSH|EC|DSA)?\s*PRIVATE KEY-----[\s\S]*?-----END\s+(?:RSA|OPENSSH|EC|DSA)?\s*PRIVATE KEY-----"
);
static JWT_RE: Lazy<Regex> = lazy_regex!(r"eyJ[a-zA-Z0-9_-]{10,}\.[a-zA-Z0-9_-]{10,}\.[a-zA-Z0-9_-]{10,}");
static DB_CONN_RE: Lazy<Regex> = lazy_regex!(r#"(mongodb|mysql|postgresql|postgres)://[^\s"'\\]{10,}"#);
static GENERIC_TOKEN_RE: Lazy<Regex> = lazy_regex!(r#"(?:secret|token|key)\s*[=:]\s*["']([a-zA-Z0-9_-]{16,})["']"#);

// Stripe key patterns
static STRIPE_LIVE_KEY_RE: Lazy<Regex> = lazy_regex!(r"sk_live_[a-zA-Z0-9]{24,}");
static STRIPE_TEST_KEY_RE: Lazy<Regex> = lazy_regex!(r"pk_test_[a-zA-Z0-9]{24,}");

// OpenAI key pattern
static OPENAI_KEY_RE: Lazy<Regex> = lazy_regex!(r"sk-[a-zA-Z0-9]{20,}T3BlbkFJ[a-zA-Z0-9]{20,}");

/// 保留前 `prefix_len` 与后 `suffix_len` 字节，中间用 `*` 填充。
///
/// 护栏：若输入短于 `prefix_len + suffix_len`（例如正则最小长度被改动），
/// 整体替换为 `[REDACTED]`，避免字节切片越界 panic。
fn mask_keep_ends(s: &str, prefix_len: usize, suffix_len: usize) -> String {
    if s.len() < prefix_len + suffix_len {
        return "[REDACTED]".to_string();
    }
    let masked_len = s.len() - prefix_len - suffix_len;
    format!(
        "{}{}{}",
        &s[..prefix_len],
        "*".repeat(masked_len),
        &s[s.len() - suffix_len..]
    )
}

/// 保留前 `prefix_len` 字节，其余替换为 `[REDACTED]`。
///
/// 护栏：若输入短于 `prefix_len`，整体替换为 `[REDACTED]`。
fn mask_prefix_redacted(s: &str, prefix_len: usize) -> String {
    if s.len() < prefix_len {
        return "[REDACTED]".to_string();
    }
    format!("{}[REDACTED]", &s[..prefix_len])
}

/// 对代码片段中的 secret 进行脱敏处理，返回新的 String。
pub fn mask_secrets(snippet: &str) -> String {
    // 1. AWS Key: 保留前缀 + 后 4 位
    let result = AWS_KEY_RE.replace_all(snippet, |caps: &regex::Captures| {
        mask_keep_ends(&caps[0], 4, 4)
    });

    // 2. GitHub Token: 保留前 8 字符 + 后 4 位
    let result = GITHUB_TOKEN_RE.replace_all(&result, |caps: &regex::Captures| {
        mask_keep_ends(&caps[0], 8, 4)
    });

    // 3. 私钥: 替换为 [REDACTED]
    let result = PRIVATE_KEY_RE.replace_all(&result, "[REDACTED]");

    // 4. JWT Token: 保留前 10 字符 + [REDACTED]
    let result = JWT_RE.replace_all(&result, |caps: &regex::Captures| {
        mask_prefix_redacted(&caps[0], 10)
    });

    // 5. DB 连接串: 保留协议 + ://[REDACTED]
    let result = DB_CONN_RE.replace_all(&result, |caps: &regex::Captures| {
        let protocol = &caps[1];
        format!("{}://[REDACTED]", protocol)
    });

    // 6. 通用 token: 保留前 4 字符
    let result = GENERIC_TOKEN_RE.replace_all(&result, |caps: &regex::Captures| {
        let full = caps.get(0).unwrap().as_str();
        let secret_val = &caps[1];
        let prefix = &secret_val[..4];
        let masked = "*".repeat(secret_val.len() - 4);
        let secret_start = caps.get(1).unwrap().start() - caps.get(0).unwrap().start();
        format!("{}\"{}{}\"", &full[..secret_start], prefix, masked)
    });

    // 7. Stripe live key: 保留前 8 字符 + 后 4 位
    let result = STRIPE_LIVE_KEY_RE.replace_all(&result, |caps: &regex::Captures| {
        mask_keep_ends(&caps[0], 8, 4)
    });

    // 8. Stripe test key: 保留前 8 字符 + 后 4 位
    let result = STRIPE_TEST_KEY_RE.replace_all(&result, |caps: &regex::Captures| {
        mask_keep_ends(&caps[0], 8, 4)
    });

    // 9. OpenAI key: 保留前 8 字符 + [REDACTED]
    let result = OPENAI_KEY_RE.replace_all(&result, |caps: &regex::Captures| {
        mask_prefix_redacted(&caps[0], 8)
    });

    result.into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_aws_key() {
        let input = "AKIAIOSFODNN7EXAMPLE";
        let masked = mask_secrets(input);
        assert!(!masked.contains("AKIAIOSFODNN7EXAMPLE"));
        assert!(masked.starts_with("AKIA"));
        assert!(masked.ends_with("MPLE"));
        assert!(masked.contains("*"));
    }

    #[test]
    fn test_mask_github_token() {
        let input = "ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij";
        let masked = mask_secrets(input);
        assert!(!masked.contains("ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij"));
        assert!(masked.starts_with("ghp_ABCD"));
        assert!(masked.ends_with("hij"));
        assert!(masked.contains("*"));
    }

    #[test]
    fn test_mask_private_key() {
        let input =
            "-----BEGIN RSA PRIVATE KEY-----\nMIIEpAIBAAKCAQEA...\n-----END RSA PRIVATE KEY-----";
        let masked = mask_secrets(input);
        assert!(!masked.contains("BEGIN RSA PRIVATE KEY"));
        assert!(!masked.contains("MIIEpAIBAAKCAQEA"));
        assert!(masked.contains("[REDACTED]"));
    }

    #[test]
    fn test_mask_jwt() {
        let input = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U";
        let masked = mask_secrets(input);
        assert!(!masked.contains("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"));
        assert!(masked.starts_with("eyJhbGciOi"));
        assert!(masked.contains("[REDACTED]"));
    }

    #[test]
    fn test_mask_db_connection_string() {
        let input = "mongodb://user:pass@host:27017/mydb";
        let masked = mask_secrets(input);
        assert!(!masked.contains("user:pass@host"));
        assert!(masked.contains("mongodb://[REDACTED]"));
    }

    #[test]
    fn test_mask_db_postgresql() {
        let input = "postgresql://admin:secret123@db.example.com:5432/app";
        let masked = mask_secrets(input);
        assert!(masked.contains("postgresql://[REDACTED]"));
        assert!(!masked.contains("admin:secret123"));
    }

    #[test]
    fn test_no_masking_on_normal_text() {
        let input = "fn main() { println!(\"hello world\"); }";
        let masked = mask_secrets(input);
        assert_eq!(masked, input);
    }

    #[test]
    fn test_mask_stripe_live_key() {
        let input = concat!("sk_live_", "abc123def456ghi789jkl012mno345");
        let masked = mask_secrets(input);
        // Stripe key should be masked
        assert!(masked.contains("*"), "Should contain masked characters");
        assert!(masked.starts_with("sk_live_"), "Should preserve prefix");
    }

    #[test]
    fn test_mask_stripe_test_key() {
        let input = concat!("pk_test_", "abc123def456ghi789jkl012mno345");
        let masked = mask_secrets(input);
        // Stripe key should be masked
        assert!(masked.contains("*"), "Should contain masked characters");
        assert!(masked.starts_with("pk_test_"), "Should preserve prefix");
    }

    #[test]
    fn test_mask_openai_key() {
        // OpenAI key format: sk-{20+}T3BlbkFJ{20+}
        let input = concat!(
            "sk-",
            "abcdefghijklmnopqrst",
            "T3BlbkFJ",
            "abcdefghijklmnopqrst"
        );
        let masked = mask_secrets(input);
        // OpenAI key should be masked
        assert!(
            masked.contains("[REDACTED]"),
            "Should contain [REDACTED], got: {}",
            masked
        );
        assert!(
            masked.starts_with("sk-abcde"),
            "Should preserve prefix, got: {}",
            masked
        );
    }
}
