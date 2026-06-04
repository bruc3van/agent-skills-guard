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

/// 同形字映射表（多种 Unicode 字符 → 拉丁字母）
///
/// 包含西里尔字母、希腊字母、拉丁扩展字符等视觉上与拉丁字母几乎相同的字符
const HOMOGLYPH_TO_LATIN: &[(char, char)] = &[
    // ── 西里尔字母小写 ──
    ('\u{0430}', 'a'), // а → a
    ('\u{0435}', 'e'), // е → e
    ('\u{043E}', 'o'), // о → o
    ('\u{0440}', 'p'), // р → p
    ('\u{0441}', 'c'), // с → c
    ('\u{0443}', 'y'), // у → y
    ('\u{0445}', 'x'), // х → x
    ('\u{0456}', 'i'), // і → i
    ('\u{0458}', 'j'), // ј → j
    ('\u{04BB}', 'h'), // һ → h
    // ── 西里尔字母大写 ──
    ('\u{0410}', 'A'), // А → A
    ('\u{0412}', 'B'), // В → B
    ('\u{0415}', 'E'), // Е → E
    ('\u{041A}', 'K'), // К → K
    ('\u{041C}', 'M'), // М → M
    ('\u{041D}', 'H'), // Н → H
    ('\u{041E}', 'O'), // О → O
    ('\u{0420}', 'P'), // Р → P
    ('\u{0421}', 'C'), // С → C
    ('\u{0422}', 'T'), // Т → T
    ('\u{0423}', 'y'), // У → y
    ('\u{0425}', 'X'), // Х → X
    ('\u{0406}', 'I'), // І → I
    ('\u{0408}', 'J'), // Ј → J
    ('\u{04AE}', 'Y'), // Ү → Y
    // ── 希腊字母小写 ──
    ('\u{03B1}', 'a'), // α → a
    ('\u{03B5}', 'e'), // ε → e
    ('\u{03B9}', 'i'), // ι → i
    ('\u{03BA}', 'k'), // κ → k
    ('\u{03BD}', 'v'), // ν → v
    ('\u{03BF}', 'o'), // ο → o
    ('\u{03C1}', 'p'), // ρ → p
    ('\u{03C3}', 'o'), // σ → o (类似 o)
    ('\u{03C4}', 't'), // τ → t
    ('\u{03C5}', 'u'), // υ → u
    ('\u{03C7}', 'x'), // χ → x
    // ── 希腊字母大写 ──
    ('\u{0391}', 'A'), // Α → A
    ('\u{0392}', 'B'), // Β → B
    ('\u{0395}', 'E'), // Ε → E
    ('\u{0396}', 'Z'), // Ζ → Z
    ('\u{0397}', 'H'), // Η → H
    ('\u{0399}', 'I'), // Ι → I
    ('\u{039A}', 'K'), // Κ → K
    ('\u{039C}', 'M'), // Μ → M
    ('\u{039D}', 'N'), // Ν → N
    ('\u{039F}', 'O'), // Ο → O
    ('\u{03A1}', 'P'), // Ρ → P
    ('\u{03A4}', 'T'), // Τ → T
    ('\u{03A5}', 'Y'), // Υ → Y
    ('\u{03A7}', 'X'), // Χ → X
    // ── 拉丁扩展字符 ──
    ('\u{0251}', 'a'), // ɑ → a
    ('\u{0261}', 'g'), // ɡ → g
    ('\u{028C}', 'v'), // ʌ → v (类似 v)
    ('\u{029C}', 'H'), // ʜ → H
    // ── 数学字母数字符号 ──
    // 这些字符与拉丁字母视觉完全相同，但用于数学语境
    ('\u{2100}', 'a'), // ℀ → a (account of)
    ('\u{2101}', 'a'), // ℁ → a (addressed to the subject)
    ('\u{2102}', 'C'), // ℂ → C (double-struck capital C)
    ('\u{210A}', 'g'), // ℊ → g (script small g)
    ('\u{210B}', 'H'), // ℋ → H (script capital H)
    ('\u{210D}', 'H'), // ℍ → H (double-struck capital H)
    ('\u{2110}', 'I'), // ℐ → I (script capital I)
    ('\u{2112}', 'L'), // ℒ → L (script capital L)
    ('\u{2113}', 'l'), // ℓ → l (script small l)
    ('\u{2115}', 'N'), // ℕ → N (double-struck capital N)
    ('\u{2119}', 'P'), // ℙ → P (double-struck capital P)
    ('\u{211A}', 'Q'), // ℚ → Q (double-struck capital Q)
    ('\u{211B}', 'R'), // ℛ → R (script capital R)
    ('\u{211D}', 'R'), // ℝ → R (double-struck capital R)
    ('\u{2124}', 'Z'), // ℤ → Z (double-struck capital Z)
    ('\u{2128}', 'Z'), // ℨ → Z (fraktur capital Z)
    // ── 全角字符 ──
    ('\u{FF21}', 'A'), // Ａ → A
    ('\u{FF22}', 'B'), // Ｂ → B
    ('\u{FF23}', 'C'), // Ｃ → C
    ('\u{FF24}', 'D'), // Ｄ → D
    ('\u{FF25}', 'E'), // Ｅ → E
    ('\u{FF26}', 'F'), // Ｆ → F
    ('\u{FF27}', 'G'), // Ｇ → G
    ('\u{FF28}', 'H'), // Ｈ → H
    ('\u{FF29}', 'I'), // Ｉ → I
    ('\u{FF2A}', 'J'), // Ｊ → J
    ('\u{FF2B}', 'K'), // Ｋ → K
    ('\u{FF2C}', 'L'), // Ｌ → L
    ('\u{FF2D}', 'M'), // Ｍ → M
    ('\u{FF2E}', 'N'), // Ｎ → N
    ('\u{FF2F}', 'O'), // Ｏ → O
    ('\u{FF30}', 'P'), // Ｐ → P
    ('\u{FF31}', 'Q'), // Ｑ → Q
    ('\u{FF32}', 'R'), // Ｒ → R
    ('\u{FF33}', 'S'), // Ｓ → S
    ('\u{FF34}', 'T'), // Ｔ → T
    ('\u{FF35}', 'U'), // Ｕ → U
    ('\u{FF36}', 'V'), // Ｖ → V
    ('\u{FF37}', 'W'), // Ｗ → W
    ('\u{FF38}', 'X'), // Ｘ → X
    ('\u{FF39}', 'Y'), // Ｙ → Y
    ('\u{FF3A}', 'Z'), // Ｚ → Z
];

/// 零宽字符列表
const ZERO_WIDTH_CHARS: &[char] = &[
    '\u{200B}', // ZERO WIDTH SPACE
    '\u{200C}', // ZERO WIDTH NON-JOINER
    '\u{200D}', // ZERO WIDTH JOINER
    '\u{FEFF}', // ZERO WIDTH NO-BREAK SPACE (BOM)
    '\u{2060}', // WORD JOINER
    '\u{00AD}', // SOFT HYPHEN
    '\u{2028}', // LINE SEPARATOR
    '\u{2029}', // PARAGRAPH SEPARATOR
    '\u{2064}', // INVISIBLE PLUS
    '\u{2062}', // INVISIBLE TIMES
    '\u{2061}', // FUNCTION APPLICATION
    '\u{2063}', // INVISIBLE SEPARATOR
    '\u{FE0E}', // VARIATION SELECTOR-15 (文本样式)
    '\u{FE0F}', // VARIATION SELECTOR-16 (表情样式)
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
        .filter(|c| HOMOGLYPH_TO_LATIN.iter().any(|(from, _)| c == from))
        .count();

    // 如果有足够多的同形字字符，检查是否为混入攻击
    if suspicious_count >= MIN_CYRILLIC_COUNT {
        // 统计 ASCII 字母数量（主要拉丁文本环境）
        let latin_count = content.chars().filter(|c| c.is_ascii_alphabetic()).count();

        if latin_count > 0 {
            let ratio = suspicious_count as f64 / (latin_count + suspicious_count) as f64;
            // 如果同形字字符占比低于阈值，说明是混入拉丁文本中的攻击
            if ratio < CYRILLIC_RATIO_THRESHOLD {
                // 找出第一个可疑字符的位置和上下文
                let (line_number, snippet) = find_suspicious_char_location(content);

                // 找出具体的可疑字符
                let suspicious_chars: Vec<char> = content
                    .chars()
                    .filter(|c| HOMOGLYPH_TO_LATIN.iter().any(|(from, _)| c == from))
                    .take(10) // 最多显示 10 个
                    .collect();

                let char_display: String = suspicious_chars
                    .iter()
                    .map(|c| {
                        let latin = HOMOGLYPH_TO_LATIN
                            .iter()
                            .find(|(from, _)| c == from)
                            .map(|(_, to)| *to)
                            .unwrap_or('?');
                        format!("'{}' (U+{:04X}) → '{}'", c, *c as u32, latin)
                    })
                    .collect::<Vec<_>>()
                    .join(", ");

                return Some(make_finding_with_location(
                    "HOMOGLYPH_ATTACK",
                    IssueSeverity::High,
                    "Homoglyph attack detected",
                    format!(
                        "Found {} character(s) that visually resemble Latin letters. \
                         This could be used to disguise malicious code or filenames. \
                         Suspicious characters: {}",
                        suspicious_count, char_display
                    ),
                    Some(file_path.to_string()),
                    ThreatCategory::Obfuscation,
                    line_number,
                    snippet,
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

    // 计算阈值：每 1000 Unicode 码点允许 ZERO_WIDTH_DENSITY_THRESHOLD 个零宽字符
    // 使用 chars().count() 而非 len()，确保中文/日文等多字节字符不会被夸大
    let content_char_count = content.chars().count().max(1);
    let threshold = (content_char_count / 1000).max(1) * ZERO_WIDTH_DENSITY_THRESHOLD;

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

        // 查找第一个零宽字符的位置
        let (line_number, snippet) = find_zero_width_location(content);

        return Some(make_finding_with_location(
            "UNICODE_STEGANOGRAPHY",
            IssueSeverity::Medium,
            "Suspicious density of zero-width characters",
            format!(
                "Found {} zero-width character(s) in {} characters of content (threshold: {}). \
                 Zero-width characters can be used for steganography to hide secrets. \
                 Breakdown: {}",
                zw_count, content_char_count, threshold, breakdown
            ),
            Some(file_path.to_string()),
            ThreatCategory::Obfuscation,
            line_number,
            snippet,
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

        // 查找第一个不可见字符的位置
        let (line_number, snippet) = find_invisible_char_location(content);

        return Some(make_finding_with_location(
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
            line_number,
            snippet,
        ));
    }

    None
}

// ── 辅助函数 ──

/// 查找第一个满足谓词的字符位置（行号和代码片段）
fn find_first_match_location(
    content: &str,
    predicate: impl Fn(char) -> bool,
) -> (Option<usize>, Option<String>) {
    for (line_idx, line) in content.lines().enumerate() {
        if line.chars().any(&predicate) {
            let snippet: String = line.chars().take(200).collect();
            return (Some(line_idx + 1), Some(snippet));
        }
    }
    (None, None)
}

/// 查找第一个可疑同形字字符的位置（行号和代码片段）
fn find_suspicious_char_location(content: &str) -> (Option<usize>, Option<String>) {
    find_first_match_location(content, |c| {
        HOMOGLYPH_TO_LATIN.iter().any(|(from, _)| c == *from)
    })
}

/// 查找第一个零宽字符的位置（行号和代码片段）
fn find_zero_width_location(content: &str) -> (Option<usize>, Option<String>) {
    find_first_match_location(content, |c| ZERO_WIDTH_CHARS.contains(&c))
}

/// 查找第一个不可见字符的位置（行号和代码片段）
fn find_invisible_char_location(content: &str) -> (Option<usize>, Option<String>) {
    find_first_match_location(content, |c| c.is_control() && !c.is_ascii_whitespace())
}

/// 创建带位置信息的 Finding 实例
fn make_finding_with_location(
    rule_id: &str,
    severity: IssueSeverity,
    title: &str,
    description: String,
    file_path: Option<String>,
    category: ThreatCategory,
    line_number: Option<usize>,
    snippet: Option<String>,
) -> Finding {
    let id_input = format!(
        "{}|{}|{}|{}",
        rule_id,
        file_path.as_deref().unwrap_or(""),
        line_number.unwrap_or(0),
        snippet
            .as_deref()
            .unwrap_or("")
            .chars()
            .take(50)
            .collect::<String>(),
    );
    let mut hasher = Sha256::new();
    hasher.update(id_input.as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    let id = hash[..20].to_string();

    Finding {
        id,
        rule_id: rule_id.to_string(),
        category,
        severity,
        title: title.to_string(),
        description,
        file_path,
        line_number,
        snippet,
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
        assert!(matches!(
            homoglyph_findings[0].severity,
            IssueSeverity::High
        ));
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
        assert!(matches!(
            invisible_findings[0].severity,
            IssueSeverity::Medium
        ));
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
            findings
                .iter()
                .any(|f| f.rule_id == "UNICODE_STEGANOGRAPHY"),
            "Should detect steganography"
        );
        assert!(
            findings
                .iter()
                .any(|f| f.rule_id == "UNICODE_INVISIBLE_CHARS"),
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
