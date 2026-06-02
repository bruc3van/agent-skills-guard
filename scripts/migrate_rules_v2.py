#!/usr/bin/env python3
"""迁移 Rust 硬编码规则到 YAML 格式（v2 - 处理注释）"""

import re
import yaml

# 读取 Rust 规则文件
with open("src-tauri/src/security/rules/builtin_compat.rs", "r", encoding="utf-8") as f:
    content = f.read()

# 移除行注释（保留字符串中的内容）
lines = content.split('\n')
cleaned_lines = []
for line in lines:
    # 简单移除 // 注释（不在字符串内的）
    # 这对于我们的规则定义足够了
    if '//' in line and 'r"' not in line and 'r#"' not in line:
        line = line[:line.index('//')]
    cleaned_lines.append(line)
content = '\n'.join(cleaned_lines)

# 提取所有 PatternRule::new 调用（使用更宽松的模式）
# 先提取整个 PatternRule::new(...) 块
blocks = re.findall(
    r'PatternRule::new\((.*?)\)',
    content,
    re.DOTALL
)

# 映射表
severity_map = {"Critical": "Critical", "High": "High", "Medium": "Medium", "Low": "Low", "Info": "Info"}
category_map = {
    "Destructive": "Destructive", "RemoteExec": "RemoteExec", "CmdInjection": "CmdInjection",
    "Network": "Network", "Privilege": "PrivilegeEscalation", "Secrets": "Secrets",
    "Persistence": "Persistence", "SensitiveFileAccess": "SensitiveFileAccess",
}

# file_types 映射
file_types_map = {
    "PY_EVAL": [".py", ".pyw", ".pyi"], "PY_EXEC": [".py", ".pyw", ".pyi"],
    "OS_SYSTEM": [".py", ".pyw", ".pyi"], "SUBPROCESS_SHELL": [".py", ".pyw", ".pyi"],
    "SUBPROCESS_CALL": [".py", ".pyw", ".pyi"], "PY_URLLIB": [".py", ".pyw", ".pyi"],
    "HTTP_REQUEST": [".py", ".pyw", ".pyi"],
    "NODE_CHILD_EXEC": [".js", ".jsx", ".ts", ".tsx", ".mjs", ".cjs"],
    "NODE_VM_RUN": [".js", ".jsx", ".ts", ".tsx", ".mjs", ".cjs"],
    "NODE_CHILD_SPAWN": [".js", ".jsx", ".ts", ".tsx", ".mjs", ".cjs"],
    "PHP_EXEC": [".php", ".phtml", ".php3", ".php4", ".php5", ".php7", ".php8"],
    "RUBY_SYSTEM_EXEC": [".rb", ".rake", ".gemspec", ".ru"],
    "GO_EXEC_COMMAND": [".go"],
    "JAVA_RUNTIME_EXEC": [".java", ".kt", ".kts", ".groovy"],
    "JAVA_PROCESS_BUILDER": [".java", ".kt", ".kts", ".groovy"],
    "CSHARP_PROCESS_START": [".cs", ".csx"],
    "POWERSHELL_BYPASS_POLICY": [".ps1", ".psm1", ".psd1"],
    "POWERSHELL_ENCODED_COMMAND": [".ps1", ".psm1", ".psd1"],
    "POWERSHELL_IEX_DOWNLOAD": [".ps1", ".psm1", ".psd1"],
    "POWERSHELL_PIPE_IEX": [".ps1", ".psm1", ".psd1"],
    "POWERSHELL_RUN_KEY": [".ps1", ".psm1", ".psd1"],
    "POWERSHELL_START_PROCESS": [".ps1", ".psm1", ".psd1"],
    "CMD_WRAPPER": [".bat", ".cmd", ".ps1"],
    "REG_RUN_KEY_ADD": [".bat", ".cmd", ".ps1", ".reg"],
    "WEBSOCKET_CONNECT": [".js", ".jsx", ".ts", ".tsx", ".mjs", ".cjs", ".py", ".rb"],
    "CURL_PIPE_SH_MENTION": [".py", ".pyw", ".pyi", ".js", ".jsx", ".ts", ".tsx", ".mjs", ".cjs",
                              ".php", ".phtml", ".rb", ".rake", ".go", ".java", ".kt", ".cs", ".csx",
                              ".ps1", ".psm1", ".psd1", ".bat", ".cmd"],
}

suppress_map = {"CURL_PIPE_SH_MENTION": ["CURL_PIPE_SH"]}

yaml_rules = []
seen_ids = set()

for block in blocks:
    # 提取字符串参数
    strings = re.findall(r'"([^"]*)"', block)
    if len(strings) < 4:
        continue

    rule_id = strings[0]
    if rule_id in seen_ids:
        continue
    seen_ids.add(rule_id)

    # 提取正则模式（r"..." 或 r#"..."# 格式）
    patterns = re.findall(r'r#?"(.*?)"#?', block, re.DOTALL)
    if not patterns:
        continue

    # 提取枚举值
    sev_match = re.search(r'Severity::(\w+)', block)
    cat_match = re.search(r'Category::(\w+)', block)
    conf_match = re.search(r'Confidence::(\w+)', block)

    if not sev_match or not cat_match:
        continue

    severity = sev_match.group(1)
    category = cat_match.group(1)
    confidence = conf_match.group(1) if conf_match else "Medium"

    # 提取数字
    nums = re.findall(r',\s*(\d+),', block)
    weight = int(nums[0]) if nums else 0

    # 提取布尔值
    hard_trigger = "true" in block.split("Confidence")[0].split(",")[-3] if "Confidence" in block else False

    # 提取描述（第二个字符串）
    name = strings[1] if len(strings) > 1 else rule_id
    description = strings[2] if len(strings) > 2 else name

    # 提取 remediation
    remediation = ""
    for s in strings[3:]:
        if len(s) > 10 and not s.startswith("CWE"):
            remediation = s
            break

    # 提取 CWE
    cwe_match = re.search(r'Some\("([^"]+)"\)', block)
    cwe_id = cwe_match.group(1) if cwe_match else None

    rule = {
        "id": rule_id,
        "category": category_map.get(category, category),
        "severity": severity_map.get(severity, severity),
        "weight": weight,
        "confidence": confidence,
        "hard_trigger": hard_trigger,
        "patterns": [p.replace('\\"', '"') for p in patterns],
        "description": description,
    }

    if remediation:
        rule["remediation"] = remediation
    if cwe_id:
        rule["cwe_id"] = cwe_id
    if rule_id in file_types_map:
        rule["file_types"] = file_types_map[rule_id]
    if rule_id in suppress_map:
        rule["suppress_if_matched"] = suppress_map[rule_id]

    yaml_rules.append(rule)

pack = {
    "name": "core",
    "version": "1.0",
    "description": "Core security rules migrated from Rust hardcoded rules",
    "rules": yaml_rules,
}

output_path = "src-tauri/resources/security/packs/core/signatures/core_rules.yaml"
with open(output_path, "w", encoding="utf-8") as f:
    yaml.dump(pack, f, default_flow_style=False, allow_unicode=True, sort_keys=False, width=200)

print(f"Migrated {len(yaml_rules)} rules to {output_path}")

# 检查关键规则
critical_rules = ["RM_RF_ROOT", "CURL_PIPE_SH", "REVERSE_SHELL", "PRIVATE_KEY", "AWS_KEY", "GITHUB_TOKEN"]
for r in critical_rules:
    if r in seen_ids:
        print(f"  ✓ {r}")
    else:
        print(f"  ✗ {r} MISSING!")

categories = {}
for r in yaml_rules:
    cat = r["category"]
    categories[cat] = categories.get(cat, 0) + 1

print("\nBy category:")
for cat, count in sorted(categories.items()):
    print(f"  {cat}: {count}")
