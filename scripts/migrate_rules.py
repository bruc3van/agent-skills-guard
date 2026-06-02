#!/usr/bin/env python3
"""迁移 Rust 硬编码规则到 YAML 格式"""

import re
import yaml

# 读取 Rust 规则文件
with open("src-tauri/src/security/rules/builtin_compat.rs", "r", encoding="utf-8") as f:
    content = f.read()

# 提取所有 PatternRule::new 调用
# 格式: PatternRule::new("ID", "name", r"pattern", Severity::X, Category::Y, weight, "desc", hard_trigger, Confidence::Z, "remediation", Some("CWE-X")/None)
pattern = r'PatternRule::new\(\s*"([^"]+)",\s*"([^"]+)",\s*r"((?:[^"\\]|\\.)*)"\s*,\s*Severity::(\w+),\s*Category::(\w+),\s*(\d+),\s*"([^"]+)",\s*(true|false),\s*Confidence::(\w+),\s*"([^"]+)",\s*(Some\("([^"]+)"\)|None)'

matches = re.findall(pattern, content, re.DOTALL)

# 映射表
severity_map = {
    "Critical": "Critical",
    "High": "High",
    "Medium": "Medium",
    "Low": "Low",
    "Info": "Info",
}

category_map = {
    "Destructive": "Destructive",
    "RemoteExec": "RemoteExec",
    "CmdInjection": "CmdInjection",
    "Network": "Network",
    "Privilege": "PrivilegeEscalation",
    "Secrets": "Secrets",
    "Persistence": "Persistence",
    "SensitiveFileAccess": "SensitiveFileAccess",
}

# file_types 映射（从 rule_applies_to_extension 逻辑提取）
file_types_map = {
    # Python 规则
    "PY_EVAL": [".py", ".pyw", ".pyi"],
    "PY_EXEC": [".py", ".pyw", ".pyi"],
    "OS_SYSTEM": [".py", ".pyw", ".pyi"],
    "SUBPROCESS_SHELL": [".py", ".pyw", ".pyi"],
    "SUBPROCESS_CALL": [".py", ".pyw", ".pyi"],
    "PY_URLLIB": [".py", ".pyw", ".pyi"],
    "HTTP_REQUEST": [".py", ".pyw", ".pyi"],
    # Node.js 规则
    "NODE_CHILD_EXEC": [".js", ".jsx", ".ts", ".tsx", ".mjs", ".cjs"],
    "NODE_VM_RUN": [".js", ".jsx", ".ts", ".tsx", ".mjs", ".cjs"],
    "NODE_CHILD_SPAWN": [".js", ".jsx", ".ts", ".tsx", ".mjs", ".cjs"],
    # PHP 规则
    "PHP_EXEC": [".php", ".phtml", ".php3", ".php4", ".php5", ".php7", ".php8"],
    # Ruby 规则
    "RUBY_SYSTEM_EXEC": [".rb", ".rake", ".gemspec", ".ru"],
    # Go 规则
    "GO_EXEC_COMMAND": [".go"],
    # Java 规则
    "JAVA_RUNTIME_EXEC": [".java", ".kt", ".kts", ".groovy"],
    "JAVA_PROCESS_BUILDER": [".java", ".kt", ".kts", ".groovy"],
    # C# 规则
    "CSHARP_PROCESS_START": [".cs", ".csx"],
    # PowerShell 规则
    "POWERSHELL_BYPASS_POLICY": [".ps1", ".psm1", ".psd1"],
    "POWERSHELL_ENCODED_COMMAND": [".ps1", ".psm1", ".psd1"],
    "POWERSHELL_IEX_DOWNLOAD": [".ps1", ".psm1", ".psd1"],
    "POWERSHELL_PIPE_IEX": [".ps1", ".psm1", ".psd1"],
    "POWERSHELL_RUN_KEY": [".ps1", ".psm1", ".psd1"],
    "POWERSHELL_START_PROCESS": [".ps1", ".psm1", ".psd1"],
    # CMD 规则
    "CMD_WRAPPER": [".bat", ".cmd", ".ps1"],
    # 注册表规则
    "REG_RUN_KEY_ADD": [".bat", ".cmd", ".ps1", ".reg"],
    # WebSocket 规则
    "WEBSOCKET_CONNECT": [".js", ".jsx", ".ts", ".tsx", ".mjs", ".cjs", ".py", ".rb"],
    # CURL_PIPE_SH_MENTION 只在非 shell 代码中
    "CURL_PIPE_SH_MENTION": [".py", ".pyw", ".pyi", ".js", ".jsx", ".ts", ".tsx", ".mjs", ".cjs",
                              ".php", ".phtml", ".rb", ".rake", ".go", ".java", ".kt", ".cs", ".csx",
                              ".ps1", ".psm1", ".psd1", ".bat", ".cmd"],
}

# suppress_if_matched 映射
suppress_map = {
    "CURL_PIPE_SH_MENTION": ["CURL_PIPE_SH"],
}

# 生成 YAML 规则
yaml_rules = []
for m in matches:
    rule_id, name, pattern, severity, category, weight, desc, hard_trigger, confidence, remediation, cwe_full, cwe_id = m

    rule = {
        "id": rule_id,
        "category": category_map.get(category, category),
        "severity": severity_map.get(severity, severity),
        "weight": int(weight),
        "confidence": confidence,
        "hard_trigger": hard_trigger == "true",
        "patterns": [pattern.replace('\\"', '"')],
        "description": desc,
        "remediation": remediation,
    }

    if cwe_id:
        rule["cwe_id"] = cwe_id

    # 添加 file_types
    if rule_id in file_types_map:
        rule["file_types"] = file_types_map[rule_id]

    # 添加 suppress_if_matched
    if rule_id in suppress_map:
        rule["suppress_if_matched"] = suppress_map[rule_id]

    yaml_rules.append(rule)

# 创建规则包
pack = {
    "name": "core",
    "version": "1.0",
    "description": "Core security rules migrated from Rust hardcoded rules",
    "rules": yaml_rules,
}

# 输出 YAML
output_path = "src-tauri/resources/security/packs/core/signatures/core_rules.yaml"
with open(output_path, "w", encoding="utf-8") as f:
    yaml.dump(pack, f, default_flow_style=False, allow_unicode=True, sort_keys=False, width=200)

print(f"Migrated {len(yaml_rules)} rules to {output_path}")

# 打印统计
categories = {}
for r in yaml_rules:
    cat = r["category"]
    categories[cat] = categories.get(cat, 0) + 1

print("\nBy category:")
for cat, count in sorted(categories.items()):
    print(f"  {cat}: {count}")
