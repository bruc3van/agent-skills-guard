# 安全扫描器修复计划

> 基于 review 核实结果，按优先级排列。

## P0 收尾：规则体系统一

### Fix 1: disabled_rules / severity_overrides 接入 scanner
- 在 `collect_matches_for_content` 的 YAML 规则循环中，检查 `policy.is_rule_disabled(rule_id)` 跳过禁用规则
- 在 finding 生成时，检查 `policy.get_severity_override(rule_id)` 覆盖严重度
- 测试验证 disabled_rules 和 severity_overrides 生效

### Fix 2: 删除 builtin 重复规则路径
- 目标：scanner 只运行 YAML 规则，不再运行 builtin_compat 的 PATTERN_RULES
- 步骤：确认 YAML 规则与 builtin 规则完全等价后，移除 `collect_matches_for_content` 中的 builtin 匹配循环
- 移除 `rule_applies_to_extension` 函数和 `get_filtered_rule_set` 函数
- 移除 `FILTERED_RULE_SETS` lazy_static
- 保留 `builtin_compat.rs` 作为兼容备份（但不再被 scanner 使用）
- 全量回归测试

## P1a 闭环：引用文件提取与结构校验

### Fix 3: 引用文件提取集成
- 在 `SkillContext::for_directory` 中调用 `referenced_files::extract_references` 填充 `referenced_files`
- 扫描 instruction_body 和所有脚本文件的内容
- 生成 orphan script finding：脚本文件在 `script_files` 中但不在 `referenced_files` 中
- 生成 missing reference finding：`referenced_files` 中的路径在 `files` 中不存在
- 测试验证 orphan/missing findings

### Fix 4: 结构校验补充
- 添加 `STRUCTURE_NAME_DIR_MISMATCH`：name 与目录名不一致（Medium）
- 添加 `STRUCTURE_NON_UTF8`：文本文件非 UTF-8 编码（Low）
- 添加 `STRUCTURE_COMPATIBILITY_TOO_LONG`：compatibility 字段超过 500 字符（Low）

## P1b 补充：PI 规则扩展

### Fix 5: PI 规则补充 3 类
- 添加 `PROMPT_INJECTION_CAPABILITY_INFLATION`（Medium）：声称拥有超出声明能力的模式
- 添加 `PROMPT_INJECTION_TOOL_CHAINING`（High）：组合使用工具进行越权操作
- 添加 `PROMPT_INJECTION_AUTONOMY_ABUSE`（High）：声称可自主决策无需用户确认

### Fix 6: PI 扫描扩展到 assets/references
- PI 规则的 file_types 从 `[.md]` 扩展为 `[.md, .txt, .html, .svg, .json, .yaml]`
- 或在 scanner 中对 assets/references 目录的文件也运行 PI 规则

## P2 补充：Archive 扩展

### Fix 7: .jar/.war/.apk 支持
- 在 `ArchiveType::from_path` 中添加 `.jar`、`.war`、`.apk` 映射到 Zip 类型
- 这些格式本质是 ZIP，复用现有提取逻辑

### Fix 8: TAR 系列提取
- 添加 `tar` crate 依赖（或使用现有依赖）
- 实现 TAR/TarGz 提取逻辑
- 复用现有的安全检查（路径穿越、大小限制、文件数限制）

### Fix 9: Archive entry symlink 检测
- 在 ZIP 提取循环中检查 entry 是否为 symlink（`entry.unix_mode()` 检查 symlink 位）
- 产生高风险 finding

## 其他改进

### Fix 10: SecurityIssue 增加 threat_category 字段
- 在 `SecurityIssue` 中添加 `#[serde(default, skip_serializing_if = "Option::is_none")] pub threat_category: Option<String>`
- 在 Finding → SecurityIssue 转换时填充 `threat_category`
- 前端可逐步展示

### Fix 11: Secret 脱敏补充
- 添加 Stripe key 模式：`sk_live_[a-zA-Z0-9]{24,}`、`pk_test_[a-zA-Z0-9]{24,}`
- 添加 OpenAI key 模式：`sk-[a-zA-Z0-9]{20,}T3BlbkFJ[a-zA-Z0-9]{20,}`

### Fix 12: EXCESSIVE_FILE_COUNT 添加 type_breakdown
- 在 metadata 中添加按 SkillFileType 分组的文件统计
