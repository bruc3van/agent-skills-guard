//! Homoglyph/Unicode 隐写检测模块
//!
//! 检测同形字攻击（homoglyph attacks）、零宽字符隐写和不可见控制字符。
//! 这些技术常用于：
//! - 伪装代码/文件名（如用西里尔字母 'а' 伪装拉丁字母 'a'）
//! - 在文本中隐藏秘密信息（零宽字符隐写）
//! - 插入不可见控制字符以绕过检测

use sha2::{Digest, Sha256};

use crate::models::security::{Finding, FindingMetadata, IssueSeverity, ThreatCategory};

// ── 常量 ──

const ANALYZER_NAME: &str = "homoglyph";

/// 西里尔字母 → 拉丁字母映射（小写和大写）
///
/// 这些是视觉上与拉丁字母几乎相同的西里尔字母
const CYRILLIC_TO_LATIN: &[(char, char)] = &[
    // 小写
    ('\u{0430}', 'a'),  // а → a
    ('\u{0435}', 'e'),  // е → e
    ('\u{043E}', 'o'),  // о → o
    ('\u{0440}', 'p'),  // р → p
    ('\u{0441}', 'c'),  // с → c
    ('\u{0443}', 'y'),  // у → y (看起来像 y)
    ('\u{0445}', 'x'),  // х → x
    // 大写
    ('\u{0410}', 'A'),  // А → A
    ('\u{0412}', 'B'),  // В → B
    ('\u{0415}', 'E'),  // Е → E
    ('\u{041A}', 'K'),  // К → K
    ('\u{041C}', 'M'),  // М → M
    ('\u{041D}', 'H'),  // Н → H
    ('\u{041E}', 'O'),  // О → O
    ('\u{0420}', 'P'),  // Р → P
    ('\u{0421}', 'C'),  // С → C
    ('\u{0422}', 'T'),  // Т → T
    ('\u{0423}', 'y'),  // У → y (看起来像 y)
    ('\u{0425}', 'X'),  // Х → X
];

/// 零宽字符列表
const ZERO_WIDTH_CHARS: &[char] = &[
    '\u{200B}', // ZERO WIDTH SPACE
    '\u{200C}', // ZERO WIDTH NON-JOINER
    '\u{200D}', // ZERO WIDTH JOINER
    '\u{FEFF}', // ZERO WIDTH NO-BREAK SPACE (BOM)
    '\u{2060}', // WORD JOINER
    '\u{00AD}', // SOFT HYPHEN
];

/// 零宽字符密度阈值：每 1000 字符超过此数量则报警
const ZERO_WIDTH_DENSITY_THRESHOLD: usize = 5;

/// Homoglyph 检测的最小西里尔字母数量
const MIN_CYRILLIC_COUNT: usize = 3;

/// Homoglyph 检测：西里尔字母占比低于此值时视为混入攻击
const CYRILLIC_RATIO_THRESHOLD: f64 = 0.3;

// ── 公共接口 ──

/// 检查内容中的 homoglyph/unicode 隐写
///
/// 返回所有检测到的 findings 列表
pub fn check(content: &str, file_path: &str) -> Vec<Finding> {
    let mut findings = Vec::new();

    // 1. 检测西里尔字母伪装
    if let Some(finding) = check_cyrillic_homoglyphs(content, file_path) {
        findings.push(finding);
    }

    // 2. 检测零宽字符隐写
    if let Some(finding) = check_zero_width(content, file_path) {
        findings.push(finding);
    }

    // 3. 检测不可见控制字符
    if let Some(finding) = check_invisible_chars(content, file_path) {
        findings.push(finding);
    }

    findings
}

// ── 内部检测函数 ──

/// 检测西里尔字母伪装拉丁字母
///
/// 逻辑：如果文本中混入了多个西里尔字母，且主要由拉丁字母组成，
/// 则可能是 homoglyph 攻击（如混淆文件名或代码）
fn check_cyrillic_homoglyphs(content: &str, file_path: &str) -> Option<Finding> {
    let suspicious_count = content
        .chars()
        .filter(|c| CYRILLIC_TO_LATIN.iter().any(|(from, _)| c == from))
        .count();

    // 如果有足够多的西里尔字母，检查是否为混入攻击
    if suspicious_count >= MIN_CYRILLIC_COUNT {
        // 统计 ASCII 字母数量（主要拉丁文本环境）
        let latin_count = content.chars().filter(|c| c.is_ascii_alphabetic()).count();

        if latin_count > 0 {
            let ratio = suspicious_count as f64 / (latin_count + suspicious_count) as f64;
            // 如果西里尔字母占比低于阈值，说明是混入拉丁文本中的攻击
            if ratio < CYRILLIC_RATIO_THRESHOLD {
                // 找出具体的可疑字符
                let suspicious_chars: Vec<char> = content
                    .chars()
                    .filter(|c| CYRILLIC_TO_LATIN.iter().any(|(from, _)| c == from))
                    .take(10) // 最多显示 10 个
                    .collect();

                let char_display: String = suspicious_chars
                    .iter()
                    .map(|c| {
                        let latin = CYRILLIC_TO_LATIN
                            .iter()
                            .find(|(from, _)| c == from)
                            .map(|(_, to)| *to)
                            .unwrap_or('?');
                        format!("'{}' (U+{:04X}) → '{}'", c, *c as u32, latin)
                    })
                    .collect::<Vec<_>>()
                    .join(", ");

                return Some(make_finding(
                    "HOMOGLYPH_ATTACK",
                    IssueSeverity::High,
                    "Cyrillic homoglyph attack detected",
                    format!(
                        "Found {} Cyrillic character(s) that visually resemble Latin letters. \
                         This could be used to disguise malicious code or filenames. \
                         Suspicious characters: {}",
                        suspicious_count, char_display
                    ),
                    Some(file_path.to_string()),
                    ThreatCategory::Obfuscation,
                ));
            }
        }
    }

    None
}

/// 检测零宽字符隐写
///
/// 零宽字符常被用于隐藏秘密信息（隐写术），如在文本中嵌入水印或隐藏指令
fn check_zero_width(content: &str, file_path: &str) -> Option<Finding> {
    let zw_count = content
        .chars()
        .filter(|c| ZERO_WIDTH_CHARS.contains(&c))
        .count();

    // 计算阈值：每 1000 字符允许 ZERO_WIDTH_DENSITY_THRESHOLD 个零宽字符
    let content_len = content.len().max(1);
    let threshold = (content_len / 1000).max(1) * ZERO_WIDTH_DENSITY_THRESHOLD;

    if zw_count > threshold {
        // 分类零宽字符类型
        let mut char_counts = std::collections::HashMap::new();
        for c in content.chars() {
            if ZERO_WIDTH_CHARS.contains(&c) {
                *char_counts.entry(c).or_insert(0) += 1;
            }
        }

        let breakdown: String = char_counts
            .iter()
            .map(|(c, count)| {
                let name = match *c {
                    '\u{200B}' => "ZERO WIDTH SPACE",
                    '\u{200C}' => "ZERO WIDTH NON-JOINER",
                    '\u{200D}' => "ZERO WIDTH JOINER",
                    '\u{FEFF}' => "BOM/ZERO WIDTH NO-BREAK SPACE",
                    '\u{2060}' => "WORD JOINER",
                    '\u{00AD}' => "SOFT HYPHEN",
                    _ => "UNKNOWN",
                };
                format!("U+{:04X} ({}): {}", *c as u32, name, count)
            })
            .collect::<Vec<_>>()
            .join(", ");

        return Some(make_finding(
            "UNICODE_STEGANOGRAPHY",
            IssueSeverity::Medium,
            "Suspicious density of zero-width characters",
            format!(
                "Found {} zero-width character(s) in {} bytes of content (threshold: {}). \
                 Zero-width characters can be used for steganography to hide secrets. \
                 Breakdown: {}",
                zw_count, content_len, threshold, breakdown
            ),
            Some(file_path.to_string()),
            ThreatCategory::Obfuscation,
        ));
    }

    None
}

/// 检测不可见控制字符
///
/// 控制字符（U+0000-U+001F 除了 \n \r \t，以及 U+007F DEL 和 C1 控制字符）
/// 可能被用于隐藏恶意内容或绕过检测
fn check_invisible_chars(content: &str, file_path: &str) -> Option<Finding> {
    let mut invisible_positions: Vec<(usize, char)> = Vec::new();

    for (pos, c) in content.char_indices() {
        let cp = c as u32;
        let is_invisible = (cp < 0x20 && cp != 0x0A && cp != 0x0D && cp != 0x09) // 控制字符（除 \n \r \t）
            || cp == 0x7F // DEL
            || (cp >= 0x80 && cp <= 0x9F); // C1 控制字符

        if is_invisible {
            invisible_positions.push((pos, c));
        }
    }

    if !invisible_positions.is_empty() {
        // 分类控制字符
        let mut categories = std::collections::HashMap::new();
        for (_, c) in &invisible_positions {
            let cp = *c as u32;
            let category = if cp < 0x20 {
                "C0 control"
            } else if cp == 0x7F {
                "DEL"
            } else {
                "C1 control"
            };
            *categories.entry(category).or_insert(0) += 1;
        }

        let breakdown: String = categories
            .iter()
            .map(|(cat, count)| format!("{}: {}", cat, count))
            .collect::<Vec<_>>()
            .join(", ");

        // 获取前几个位置作为示例
        let examples: String = invisible_positions
            .iter()
            .take(5)
            .map(|(pos, c)| format!("U+{:04X} at byte {}", *c as u32, pos))
            .collect::<Vec<_>>()
            .join(", ");

        return Some(make_finding(
            "UNICODE_INVISIBLE_CHARS",
            IssueSeverity::Medium,
            "Invisible control characters detected",
            format!(
                "Found {} invisible control character(s). \
                 These could be used to hide malicious content or bypass detection. \
                 Categories: {}. Examples: {}",
                invisible_positions.len(),
                breakdown,
                examples
            ),
            Some(file_path.to_string()),
            ThreatCategory::Obfuscation,
        ));
    }

    None
}

// ── 辅助函数 ──

/// 创建 Finding 实例
///
/// 使用 sha2 生成稳定的 finding ID
fn make_finding(
    rule_id: &str,
    severity: IssueSeverity,
    title: &str,
    description: String,
    file_path: Option<String>,
    category: ThreatCategory,
) -> Finding {
    let id_input = format!(
        "{}|{}",
        rule_id,
        file_path.as_deref().unwrap_or(""),
    );
    let mut hasher = Sha256::new();
    hasher.update(id_input.as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    let id = hash[..16].to_string();

    Finding {
        id,
        rule_id: rule_id.to_string(),
        category,
        severity,
        title: title.to_string(),
        description,
        file_path,
        line_number: None,
        snippet: None,
        remediation: Some(
            "Review the file for suspicious Unicode characters and remove any unauthorized homoglyphs or invisible characters".to_string()
        ),
        analyzer: ANALYZER_NAME.to_string(),
        metadata: Some(FindingMetadata {
            rule_source: Some("homoglyph".to_string()),
            ..Default::default()
        }),
    }
}

// ── 单元测试 ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normal_english_text_no_findings() {
        let content = "Hello, this is a normal English text with no suspicious characters.";
        let findings = check(content, "test.txt");
        assert!(
            findings.is_empty(),
            "Normal English text should not produce any findings"
        );
    }

    #[test]
    fn test_cyrillic_homoglyph_attack() {
        // "аdmin" 用西里尔字母 'а' (U+0430) 伪装 'a'
        // 混入足够多的西里尔字母但主要是拉丁文本
        let content = "аdmin pаssword is sаfe аnd secure";
        let findings = check(content, "test.txt");

        let homoglyph_findings: Vec<_> = findings
            .iter()
            .filter(|f| f.rule_id == "HOMOGLYPH_ATTACK")
            .collect();
        assert!(
            !homoglyph_findings.is_empty(),
            "Should detect Cyrillic homoglyph attack"
        );
        assert_eq!(homoglyph_findings[0].analyzer, "homoglyph");
        assert!(matches!(homoglyph_findings[0].severity, IssueSeverity::High));
    }

    #[test]
    fn test_cyrillic_only_text_no_false_positive() {
        // 纯西里尔文本（如俄语）不应误报
        let content = "Это текст на русском языке для тестирования.";
        let findings = check(content, "test.txt");

        let homoglyph_findings: Vec<_> = findings
            .iter()
            .filter(|f| f.rule_id == "HOMOGLYPH_ATTACK")
            .collect();
        assert!(
            homoglyph_findings.is_empty(),
            "Pure Cyrillic text should not trigger homoglyph attack"
        );
    }

    #[test]
    fn test_zero_width_steganography() {
        // 创建包含大量零宽字符的文本
        let mut content = String::from("Normal text ");
        for _ in 0..100 {
            content.push('\u{200B}'); // ZERO WIDTH SPACE
        }
        content.push_str(" more text");

        let findings = check(&content, "test.txt");

        let zw_findings: Vec<_> = findings
            .iter()
            .filter(|f| f.rule_id == "UNICODE_STEGANOGRAPHY")
            .collect();
        assert!(
            !zw_findings.is_empty(),
            "Should detect zero-width character steganography"
        );
        assert!(matches!(zw_findings[0].severity, IssueSeverity::Medium));
    }

    #[test]
    fn test_zero_width_single_bom_no_false_positive() {
        // 单个 BOM 字符（正常文件开头）不应误报
        let content = "\u{FEFF}This is a file with BOM";
        let findings = check(content, "test.txt");

        let zw_findings: Vec<_> = findings
            .iter()
            .filter(|f| f.rule_id == "UNICODE_STEGANOGRAPHY")
            .collect();
        assert!(
            zw_findings.is_empty(),
            "Single BOM character should not trigger steganography detection"
        );
    }

    #[test]
    fn test_invisible_control_chars() {
        // 包含不可见控制字符
        let content = "Hello\x00World\x07Test\x1BEnd";
        let findings = check(content, "test.txt");

        let invisible_findings: Vec<_> = findings
            .iter()
            .filter(|f| f.rule_id == "UNICODE_INVISIBLE_CHARS")
            .collect();
        assert!(
            !invisible_findings.is_empty(),
            "Should detect invisible control characters"
        );
        assert!(matches!(invisible_findings[0].severity, IssueSeverity::Medium));
    }

    #[test]
    fn test_normal_newlines_tabs_no_false_positive() {
        // 正常的换行和制表符不应误报
        let content = "Line 1\nLine 2\tTabbed\r\nWindows line ending";
        let findings = check(content, "test.txt");

        let invisible_findings: Vec<_> = findings
            .iter()
            .filter(|f| f.rule_id == "UNICODE_INVISIBLE_CHARS")
            .collect();
        assert!(
            invisible_findings.is_empty(),
            "Normal \\n, \\r, \\t should not trigger invisible character detection"
        );
    }

    #[test]
    fn test_cjk_text_no_false_positive() {
        // 中文/日文/韩文文本不应被误报为 homoglyph
        let content = "这是一个中文测试文本，包含日文假名：あいうえお";
        let findings = check(content, "test.txt");

        let homoglyph_findings: Vec<_> = findings
            .iter()
            .filter(|f| f.rule_id == "HOMOGLYPH_ATTACK")
            .collect();
        assert!(
            homoglyph_findings.is_empty(),
            "CJK characters should not trigger homoglyph detection"
        );
    }

    #[test]
    fn test_mixed_latin_and_cyrillic_few_no_attack() {
        // 少量西里尔字母（低于阈值）不应触发
        let content = "This has а couple of cyrillic chars but not enough";
        let findings = check(content, "test.txt");

        let homoglyph_findings: Vec<_> = findings
            .iter()
            .filter(|f| f.rule_id == "HOMOGLYPH_ATTACK")
            .collect();
        assert!(
            homoglyph_findings.is_empty(),
            "Few Cyrillic chars should not trigger homoglyph attack"
        );
    }

    #[test]
    fn test_finding_analyzer_is_homoglyph() {
        let content = "аdmin pаssword is sаfe аnd secure";
        let findings = check(content, "test.txt");

        for finding in &findings {
            assert_eq!(finding.analyzer, "homoglyph");
        }
    }

    #[test]
    fn test_finding_category_is_obfuscation() {
        let content = "аdmin pаssword is sаfe аnd secure";
        let findings = check(content, "test.txt");

        for finding in &findings {
            assert!(
                matches!(finding.category, ThreatCategory::Obfuscation),
                "All homoglyph findings should have Obfuscation category"
            );
        }
    }

    #[test]
    fn test_finding_has_stable_id() {
        let content = "аdmin pаssword is sаfe аnd secure";
        let f1 = check(content, "test.txt");
        let f2 = check(content, "test.txt");

        if let (Some(f1), Some(f2)) = (f1.first(), f2.first()) {
            assert_eq!(f1.id, f2.id, "Same inputs should produce same finding ID");
        }
    }

    #[test]
    fn test_multiple_detection_types() {
        // 包含多种问题的文本
        let mut content = String::from("аdmin pаssword is sаfe"); // 西里尔字母 (а, а, а = 3)
        for _ in 0..20 {
            content.push('\u{200B}'); // 零宽字符
        }
        content.push_str("test\x00file"); // 控制字符

        let findings = check(&content, "test.txt");

        // 应该检测到多种问题
        assert!(
            findings.iter().any(|f| f.rule_id == "HOMOGLYPH_ATTACK"),
            "Should detect homoglyph"
        );
        assert!(
            findings.iter().any(|f| f.rule_id == "UNICODE_STEGANOGRAPHY"),
            "Should detect steganography"
        );
        assert!(
            findings.iter().any(|f| f.rule_id == "UNICODE_INVISIBLE_CHARS"),
            "Should detect invisible chars"
        );
    }

    #[test]
    fn test_del_character_detected() {
        // DEL 字符 (U+007F) 应该被检测
        let content = "Hello\x7FWorld";
        let findings = check(content, "test.txt");

        let invisible_findings: Vec<_> = findings
            .iter()
            .filter(|f| f.rule_id == "UNICODE_INVISIBLE_CHARS")
            .collect();
        assert!(
            !invisible_findings.is_empty(),
            "DEL character should be detected"
        );
    }

    #[test]
    fn test_c1_control_chars_detected() {
        // C1 控制字符 (U+0080-U+009F) 应该被检测
        let content = "Hello\u{0080}\u{0081}\u{0082}World";
        let findings = check(content, "test.txt");

        let invisible_findings: Vec<_> = findings
            .iter()
            .filter(|f| f.rule_id == "UNICODE_INVISIBLE_CHARS")
            .collect();
        assert!(
            !invisible_findings.is_empty(),
            "C1 control characters should be detected"
        );
    }
}
