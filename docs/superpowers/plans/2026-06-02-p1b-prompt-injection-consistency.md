# P1b: Prompt Injection、Allowed-tools 与行为一致性检查 — 实现计划

> **For agentic workers:** 使用 subagent-driven-development 执行。

**Goal:** 补齐 Agent Skill 特有威胁面，检测 Skill 声明与实际行为的不一致。

**Architecture:** Prompt Injection 规则添加到 YAML 规则包；allowed-tools/一致性/描述质量检查放入 `consistency_checker.rs`；集成到 scanner.rs。

---

## Task 1: Prompt Injection 规则添加到 YAML

**Files:**
- Modify: `src-tauri/resources/security/packs/core/signatures/core_rules.yaml`

添加 5 条 Prompt Injection 规则到 YAML 规则包末尾：
- `PROMPT_INJECTION_IGNORE_INSTRUCTIONS` (High, hard_trigger)
- `PROMPT_INJECTION_UNRESTRICTED_MODE` (High, hard_trigger)
- `PROMPT_INJECTION_BYPASS_POLICY` (High, hard_trigger)
- `PROMPT_INJECTION_REVEAL_SYSTEM` (Medium)
- `PROMPT_INJECTION_CONCEALMENT` (High, hard_trigger)

file_types 为 `.md`（markdown 文件）。

验证：`cargo test rules::loader`

---

## Task 2: consistency_checker 模块 — allowed-tools 检查

**Files:**
- Create: `src-tauri/src/security/consistency_checker.rs`
- Modify: `src-tauri/src/security/mod.rs`

实现 `check_allowed_tools(ctx: &SkillContext) -> Vec<Finding>`：
- 若 manifest.allowed_tools 为空，跳过
- 检查 6 种能力：Read/Write/Bash/Grep/Glob/Network
- 每种能力用 regex 匹配代码文件中的实际行为模式
- 未声明但实际使用 → 产生 finding

测试：各种 allowed_tools 组合 + 代码行为匹配/不匹配。

---

## Task 3: consistency_checker — manifest 行为一致性

实现 `check_manifest_consistency(ctx: &SkillContext) -> Vec<Finding>`：
- `TOOL_ABUSE_UNDECLARED_NETWORK` — 代码使用网络但 compatibility 未声明
- `SOCIAL_ENG_MISLEADING_DESC` — 描述暗示简单功能但代码使用网络/高风险能力

---

## Task 4: consistency_checker — description 质量检查

实现 `check_description_quality(ctx: &SkillContext) -> Vec<Finding>`：
- `TRIGGER_OVERLY_GENERIC` — 泛化描述模式匹配
- `TRIGGER_DESCRIPTION_TOO_SHORT` — 描述单词数 < 5
- `TRIGGER_VAGUE_DESCRIPTION` — 泛化词占比 > 40%
- `TRIGGER_KEYWORD_BAITING` — 逗号分隔关键词 > 8 个

---

## Task 5: Scanner 集成 + 回归测试

**Files:**
- Modify: `src-tauri/src/security/scanner.rs`

在 `scan_directory_with_options` 中调用 `consistency_checker::check(&skill_ctx)`。

验证：全部测试通过。
