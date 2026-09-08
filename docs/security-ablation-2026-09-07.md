# 安全扫描模块消融实验报告（2026-09-07）

## 背景与方法

对 `src-tauri/src/security/`（19 个文件，约 15k 行）执行一次**行为保持**的消融实验：
逐层移除缺乏使用者的抽象与设计，以全量测试套件验证移除前后行为等价。

- 基线：main @ `7eec15c`，**569 个 Rust 测试全绿**
- 终态：**566 个测试全绿**（减少的 3 个测试专属被删除的便捷方法，见下文）
- 消融后实际变更：**+642 / -1088（净删 446 行，忽略缩进重排后统计）**
- `cargo clippy`：无新增告警（存量风格告警均在未触碰代码中）
- 前端与 `models/security.rs` 零改动，JSON 契约不变

## 已删除清单

### 1. 死代码与死配置（零调用者）

| 项 | 位置 | 判定依据 |
|---|---|---|
| `score_from_issues_with_policy` | scanner.rs | 全仓库零调用 |
| `ScanPolicy::from_yaml` | policy.rs | 零调用（默认策略走 `builtin_default` 的 LazyLock） |
| `ScanPolicy::is_known_installer_domain` | policy.rs | 被 `targets_known_installer` 取代；且为子串匹配，与白名单防冒充设计矛盾 |
| `CredentialPolicy`（`credentials.known_test_values`） | policy.rs + default.yaml | 字段定义后从未被任何分析器读取 |
| `FindingOutputPolicy`（`finding_output.dedupe`） | policy.rs + default.yaml | `finalize_issues` 无条件去重，标志从未被查询 |
| `ScanPolicy.preset_base` | policy.rs + default.yaml | 反序列化后无读取方 |
| `strict_structure::is_known_binary_ext` | strict_structure.rs | 完全死代码（惰性扩展名第三份副本） |
| `SkillContext::{skill_name, file_count, total_size_bytes, files_by_type, script_paths, asset_paths, is_referenced}` | skill_context.rs | 仅本文件测试使用的便捷访问器 |
| `AnalyzabilityResult::{score, total_bytes, analyzable_bytes, unanalyzable_bytes}` | analyzability.rs | 生产代码只读 `findings` 与 `has_risky_unanalyzable_content` |

注：`policy_fingerprint` 会因死字段删除而改变。已核实 services/commands 中不存在指纹相等性
比较（仅作为报告元数据写入 DB 供追溯），无缓存失效风险。

### 2. 转发包装层

| 删除 | 保留形式 |
|---|---|
| `scan_directory`（4 参转发）+ `scan_directory_with_options`（5 参，含 `on_file_scanned` 回调） | `scan_directory(dir, skill_id, locale, options)`——回调参数在生产代码中全部传 `None`、测试也不使用，直接删除 |
| `scan_file`（3 参转发）+ `scan_file_with_options` | `scan_file(content, file_path, locale, options)` |
| `calculate_score_from_issues`（无策略转发） | `score_from_issues` 直调 `calculate_score_from_issues_with_policy(_, None)` |
| `should_include_match` / `should_include_context_finding` / `should_include_structure_kind` 三层谓词 | `should_include(kind, policy)` + `should_include_finding` 两个 |

### 3. 无状态对象仪式（SecurityScanner 解体）

`pub struct SecurityScanner;` 是零字段单元结构体，全部方法为关联函数，`new()`/`Default`
纯仪式；`SkillManager` 与 `PluginManager` 为此持有无意义的 `scanner` 字段。

- 删除结构体、`new()`、`Default`，scanner.rs 全部关联函数改为模块级自由函数
- 公共 API 收敛为 `security::{scan_directory, scan_file, count_scan_files, calculate_checksum, score_from_issues, count_kinds, ScanOptions, ScanPolicy}`
- `issue_from_finding` 降为 `pub(crate)`（仅 `skill_reports` 与单测使用）
- 两个 Manager 删除 `scanner` 字段

### 4. 重命名别名与 Deref newtype

- 删除 `rules/types.rs` 整个文件：`type Category = ThreatCategory` 是纯重命名层，
  24 处使用点改回 `ThreatCategory`；`Confidence` 并入 `rules/mod.rs`
- 删除 `CrossSkillFinding` 的 `Deref<Target = Finding>`，调用点显式 `.finding`

### 5. 可见性收敛与重复配置合并

- 12 个仅文件内部使用的 `pub fn` 收敛为私有（file_magic / consistency_checker /
  referenced_files / asset_checks / pattern_engine / loader / finding_builder / strict_structure）
- `SKIP_DIR_NAMES` 从 `pub` 降为 `pub(crate)`
- `SkillContext::new` 标记 `#[cfg(test)]`（生产构造走 `for_single_file` / `for_directory`）
- 删除 scanner.rs 的 `MAX_BYTES_PER_FILE` 常量与 `MAX_SCAN_DEPTH` 常量，统一读取
  `policy.file_limits`（默认值 2 MiB / 20 完全一致，默认行为不变；策略覆盖路径下
  截断与 `OVERSIZED_FILE` 判定从"各用各的值"变为共用同一值）

## 保留清单（消融后仍然保留，及理由）

| 保留项 | 理由 |
|---|---|
| `ScanOptions.policy` 覆盖通道 + `ScanPolicy` 公共字段 | 集成测试（scoring_quality / rule_matrix / security_boundaries）的策略机制测试缝隙；生产始终 `builtin_default`，但 YAML 策略引擎本身是真实生效的 |
| policy 的嵌套子结构（`file_limits` 等 6 个） | 与 `default.yaml` 结构一一对应；扁平化需破坏 YAML 兼容，收益低 |
| `FindingSpec` 构造器与各分析器本地 `make_finding` 薄封装 | 14 字段构造在 6 个分析器复用，是合理便利而非多余间接层 |
| `SkillScanContext`（cross_skill） | 正当的输入 DTO：携带 DB 侧元数据（skill_id/name/description）+ 文本内容，与 `SkillContext` 职责不同 |
| `analyzability::is_inert_asset` 的超集扩展名列表 | **与 policy `inert_extensions` 刻意不一致**：policy 列表控制"扫描跳过"，本列表控制"可分析性统计口径"（覆盖音视频/办公文档/字节码）。原注释声称两者一致已过时，本次修正了注释 |
| `__pycache__` 在 context 与 scanner 中的不对称跳过 | 有意设计（单测注释明确："__pycache__ stays visible to structure analyzers"），.pyc 需计入 unanalyzable 统计，不动 |

## 行为等价验证

| 阶段 | 测试结果 |
|---|---|
| 基线（消融前） | 569 全绿 |
| 死代码/死配置清除后 | 566 全绿 |
| 包装层坍缩 + SecurityScanner 解体后 | 566 全绿 |
| 可见性收敛 + 别名/Deref 删除 + 限制合并后 | 566 全绿（终态） |

减少的 3 个测试为 `test_skill_context_name_fallback`、`test_skill_context_files_by_type`、
`test_skill_context_is_referenced`——它们仅覆盖被删除的便捷访问器；其背后的行为
（文件分类、引用提取）由 `for_directory` 路径上的既有测试继续覆盖。

扫描语义验证依赖的既有行为测试全部原样通过：硬触发阻断、rm -rf 变体、curl-pipe、
跨技能零权重关联、评分序列化往返、规则矩阵 55+ fixtures、test-skills 正/负样本集。

## 指标对比

| 指标 | 消融前 | 消融后 |
|---|---|---|
| security/ 模块对外 pub API | 12 个 `SecurityScanner::*` 方法 + 结构体 + 2 个 re-export | 6 个自由函数 + 2 个类型 re-export |
| security/ 文件数 | 19 | 18（删除 rules/types.rs） |
| 模块净行数（忽略缩进重排） | — | **-446** |
| 重复的"惰性扩展名"列表 | 3 份（policy / analyzability / strict_structure 死代码） | 1 份 policy + 1 份口径超集（职责分离、注释修正） |
| 重复的"单文件大小/深度"限制 | 2 份（scanner 常量 / policy 字段） | 1 份（policy） |
| 依赖（Cargo.toml/lock） | 不变 | 不变 |

调用方更新：`commands/security.rs`、`commands/plugins.rs`、`services/skill_manager.rs`
（删 scanner 字段）、`services/plugin_manager.rs`（删 scanner 字段）、`services/skill_reports.rs`
及 4 个集成测试文件，全部为机械替换，无逻辑变更。

## 后续可选消融项（未执行）

- `build_scan_context_from_skill_dir` 每次跨技能刷新都重新遍历全部 skill 目录——是性能
  问题而非抽象问题，涉及缓存设计，建议单独立项
- pipeline / consistency_checker 等分析器文件内部仍有较长的模式常量表，可考虑统一到
  YAML 规则包，但会改变规则治理边界，收益与风险需单独评估
