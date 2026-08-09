# 全项目深度 Review（安全 + 架构 + 质量）

- **日期**：2026-08-10
- **工具**：ZCode
- **模型**：GLM-5.2（builtin:bigmodel-coding-plan/GLM-5.2）
- **版本**：v1.3.6（commit `727457f`，工作区干净）
- **范围**：`src-tauri/src/security/`（扫描引擎）、`src-tauri/src/{services,commands,models}/`（服务层 + IPC）、`src/`（React 前端）、Tauri 配置与能力声明
- **性质**：仅诊断，本轮未修改任何代码
- **方法**：Rust 安全引擎、Rust 服务/命令层、前端与配置三条线并行深读，交叉汇总

---

## 结论摘要

整体工程素养高于一般个人项目：shell 执行零注入面、zip-slip 防护到位、SQL 全参数化、更新有签名校验、XSS 面干净。但在**信任边界**（前端绕过后端校验、路径删除无 allowlist）和**输入校验**（GitHub URL 无 host/charset 校验）上存在真实可利用的缺口，应优先处理。

| 编号 | 问题 | 等级 | 类别 | 位置 |
|---|---|---|---|---|
| H1 | `uninstall_skill_path` 可删除任意路径 | 🔴 P0 | 文件系统/信任边界 | `skill_manager.rs:1608-1636` |
| H2 | 前端 fallback `openPath` 绕过后端 allowlist | 🔴 P0 | 信任边界 | `ToolIcons.tsx:77-83`、`InstalledSkillsPage.tsx:2300-2306,2471-2477` |
| H3 | `opener:allow-open-path` 授权 `path:"**"` 过宽 | 🔴 P0 | Tauri 能力 | `capabilities/default.json:17-20` |
| H4 | GitHub URL→owner/repo 无 host/charset 校验 | 🔴 P0 | 网络/注入 | `repository.rs:46-72`、`commands/mod.rs:131-142` |
| M1 | `staging_path`（DB 来源）删除/拷贝无 cache-containment 校验 | 🟠 P1 | 文件系统 | `skill_manager.rs:2965-2975, 2774-2799` |
| M2 | 下载的 skill 包无完整性/签名校验 | 🟠 P1 | 网络/信任 | `github.rs:504-610` |
| M3 | IPC 结果零运行时校验，`zod` 装了却没用 | 🟠 P1 | 前端/类型 | `package.json`、`src/lib/api.ts` |
| M4 | 扫描器 skill 内部无并行（串行 80 规则 × 2MiB） | 🟠 P1 | 性能 | `scanner.rs:1012-1058` |
| M5 | 三处冗余文件 I/O（context / scanner / analyzer 各读一遍） | 🟠 P1 | 性能 | `skill_context.rs:344`、`scanner.rs:1480`、`pipeline.rs:1215` |
| M6 | `is_binary` 判定在 context 与 scanner 间不一致（UTF-16） | 🟠 P1 | 正确性 | `skill_context.rs:411-420` ↔ `scanner.rs` |
| L1 | `find_line_number` 循环内逐行 `to_lowercase()` | 🟡 P2 | 性能 | `pipeline.rs:708-714, 746, 1279` |
| L2 | `secret_masking` 硬编码 `"` 引号，单引号被破坏 | 🟡 P2 | 正确性 | `secret_masking.rs:80` |
| L3 | `is_known_installer_domain` 用 `contains` 与注释自相矛盾 | 🟡 P2 | 正确性/潜在 | `policy.rs:511-517` |
| L4 | finding ID 双格式 + symlink finding 绕过 builder | 🟡 P2 | 一致性 | `homoglyph.rs:425-440`、`scanner.rs:1511-1539` |
| L5 | `CyberSelect` 无 ARIA/键盘导航，不可访问 | 🟡 P2 | 可访问性 | `ui/CyberSelect.tsx:62-103` |
| L6 | `ErrorBoundary` 兜底 UI 硬编码颜色、无 reset | 🟡 P2 | 质量/一致性 | `ErrorBoundary.tsx:34,43,49` |
| L7 | 6 处 `href={...repository_url}` 无 scheme 白名单 | 🟡 P2 | 前端/注入 | `InstalledSkillsPage.tsx:2097,2264,2454` 等 |
| L8 | ~17 处 `catch (error: any)` 绕过类型化 `ApiError` | 🟡 P2 | 质量 | `InstalledSkillsPage.tsx` |

**建议修复顺序**：H1 → H2/H3 → H4 → M3 → M1 → M2 → M4/M5 → M6 → L 系列

---

## H1 🔴 `uninstall_skill_path` 可删除任意路径

**位置**：`src-tauri/src/services/skill_manager.rs:1608-1636`，经 `commands/mod.rs:718-730` 暴露。

`uninstall_skill_path(path_to_remove: String)` 直接对前端传入的自由字符串执行：

```rust
std::fs::remove_dir_all(&path)   // :1630
std::fs::remove_file(&path)      // :1633
```

没有任何校验确认它落在 `~/.agents/skills`、工具 skills 目录或 DB 记录的 `skill.local_paths` 内。唯一的判断是 `link_fs::is_dir_link` / `path.exists()`。

对比 `open_skill_directory`（`commands/mod.rs:992-1044`）做了完整的 canonicalize + allowlist —— **打开路径有防护，删除路径反而没有**。一旦 webview 被注入、前端有 bug，或调用方传错，传入 `~/Documents` 会被递归删除。

**影响**：任意目录/文件删除，破坏性。

---

## H2 🔴 前端 fallback `openPath` 绕过后端 allowlist

**位置**：`src/components/ui/ToolIcons.tsx:77-83`、`src/components/InstalledSkillsPage.tsx:2300-2306, 2471-2477`。

当 `api.openSkillDirectory(path)` 抛错时，代码 fallback 调用 `@tauri-apps/plugin-opener` 的 `openPath(path)`：

```ts
} catch {
  await openPath(path);  // 直接连 plugin-opener，不经后端校验
}
```

这条路径直接命中 H3 的 `path: "**"` capability，**后端的 allowlist 形同虚设** —— 渲染层任意 JS 都能让系统打开器处理任意路径。

---

## H3 🔴 `opener:allow-open-path` 授权过宽

**位置**：`src-tauri/capabilities/default.json:17-20`。

```json
"opener:allow-open-path",
{ "name": "opener:allow-open-url", "allow": [{ "url": "**" }] }
```

实际配置中 `opener:allow-open-path` 用 `{ "path": "**" }` 允许打开任意路径。对桌面应用这是不必要的面，配合 H2 可被任意渲染层 JS 触发。

**建议**：收窄 `path` 到 `.agents`/`.claude`/工具目录等白名单，或前端禁止 `openPath` fallback。

---

## H4 🔴 GitHub URL → owner/repo 无 host/charset 校验

**位置**：`src-tauri/src/models/repository.rs:46-72`、`src-tauri/src/commands/mod.rs:131-142`。

`from_github_url` 仅按 `/` 切最后两段当作 `owner`/`repo`，**不校验 host 是否为 `github.com`**，也不限制字符集。这些字符串随后被 `format!` 拼进多处 API URL：

- `github.rs:283-288`（`/repos/{owner}/{repo}/contents/{path}`）
- `github.rs:425-427`（`raw.githubusercontent.com/{owner}/{repo}/...`）
- `github.rs:521-524`（`/repos/{owner}/{repo}/zipball/{branch}`）
- `github.rs:662, 713, 942`

**多数路径没有 percent-encode**（只有 `:946-968` 用了 `.query()`）。同时 `owner`/`repo` 还被用作**缓存目录名**（`github.rs:511`、`skill_manager.rs:2552`）。含 `../` 或 `/` 的 repo 名既能操纵 API 路径，又能让缓存目录逃出 cache root。

注：由于 `from_github_url` 只取最后两段，`https://evil.example/x/github.com/owner/repo` 仍会打到 `api.github.com`，所以**不是经典 SSRF**；但路径注入与缓存目录逃逸是真实风险。

---

## M1 🟠 `staging_path`（DB 来源）无 cache-containment 校验

**位置**：`src-tauri/src/services/skill_manager.rs:2965-2975`（`cancel_skill_update`）、`:2774-2799`（`confirm_skill_update`）。

`staging_dir` 直接从 `skill.staging_path`（DB 字段）读取，`cancel_skill_update` 用作 `remove_dir_all` 目标，`confirm_skill_update` 用作递归 copy 源，**均未校验它落在 `<cache>/agent-skills-guard/staging` 下**。当前只有代码自己往里写 cache 路径，利用需要 DB 篡改，但缺乏纵深防御。

---

## M2 🟠 下载的 skill 包无完整性/签名校验

**位置**：`src-tauri/src/services/github.rs:504-610`。

`download_repository_archive` 记录 `commit_sha`（从 zipball 顶层目录名派生）和 SKILL.md 的 SHA256，但**不校验已知签名或 pinned hash**。信任完全来自到 `api.github.com`/`codeload.github.com` 的 TLS。对一个"安全管理器"，扫描器是启发式而非信任闸门 —— 这一点至少应该在文档里对用户讲清楚。

---

## M3 🟠 IPC 结果零运行时校验，`zod` 未使用

**位置**：`package.json`（`"zod": "^4.1.12"`）、`src/lib/api.ts`。

全仓 `grep z\.` 对 `zod` **零匹配**。所有 `invoke<T>` 靠 TS 泛型直接强转：

```ts
return invoke<Repository[]>('get_repositories');          // api.ts
return invoke<SecurityReport>('scan_skill', { ... });     // 无运行时 schema
```

Rust 端某天少返回一个 `issues` 字段，渲染层会在 `countIssuesBySeverity` / `groupIssuesBySignature` 里崩溃而非优雅降级。Rust struct（`#[derive(Serialize)]`）与 `src/types/*.ts` 是手工双份维护，没有编译期关联。

---

## M4 🟠 扫描器 skill 内部无并行

**位置**：`src-tauri/src/security/scanner.rs:1012-1058`。

`rayon` 是依赖项，但只在 `commands/security.rs` 的 **skill 之间** 做了 `par_iter`。单个 skill 内部（~80 条规则 × 2MiB 文件）是完全串行的热点路径：

```rust
for compiled_rule in yaml_rules {
    for (line_number, line) in &scan_lines {
        match_yaml_rule(...);
    }
}
```

`yaml_rules.par_iter().flat_map(...)` 会是显著提速，对大 skill 尤为明显。

---

## M5 🟠 三处冗余文件 I/O

**位置**：`skill_context.rs:344`、`scanner.rs:1480`、`pipeline.rs:1215`。

同一份 skill 内容被读多次：

1. `SkillContext::for_directory`（`skill_context.rs:344`）走一遍目录，对每个文件读 512 字节样本；
2. `scan_directory_with_options`（`scanner.rs:1480`）再走一遍目录并完整读每个文件；
3. `pipeline::analyze` 通过 `read_text_file`（`pipeline.rs:1215`）第三次从磁盘读脚本文件 —— 因为目录扫描模式下 `file_contents` 为空，`read_text_file` 回落磁盘。

对大 skill 可测量。建议合并成一次遍历 + 内容缓存。

---

## M6 🟠 `is_binary` 判定不一致

**位置**：`src-tauri/src/security/skill_context.rs:411-420` ↔ `scanner.rs`。

`SkillFile.is_binary` 用朴素的 `contains(0u8)` 判定，而 `scanner.rs` 用 UTF-16 感知的 `detect_utf16_encoding`。结果：

- UTF-16 文本文件被 context 标记为二进制（`analyzability.rs`、`pipeline.rs` 据此判定"不可分析"）；
- 但 scanner 仍把它解码后扫描。

**状态不一致**：同一个文件"不可分析"却又产出了 finding。建议统一判定函数。

---

## L 系列（P2，按主题归并）

### L1 性能：`find_line_number` 逐行分配
`pipeline.rs:708-714` 在循环内对每行 `line.to_lowercase()` 分配新 String，`needle.to_lowercase()` 也未外提。被 `:746, 1279` 多次调用。`char::eq_ignore_ascii_case` 可零分配。另外 `:746` 用它重找外层 `content.lines()` 已知的行号，浪费且可能在重复前缀时返回错行。

### L2 正确性：`secret_masking` 破坏单引号
`secret_masking.rs:80` 重建 snippet 硬编码 `"`：`format!("{}\"{}{}\"", ...)`，但 `GENERIC_TOKEN_RE`（`:11`）同时匹配 `'...'`，单引号场景的遮蔽输出会破坏引号。

### L3 潜在：installer 域匹配自相矛盾
`policy.rs:511-517` 的 `is_known_installer_domain` 用 `contains` 子串匹配（`bun.sh` 会被 `evil-bun.sh` 命中），与 `:449-450` 注释明确反对的写法自相矛盾。当前未被调用，属潜在隐患。

### L4 一致性：finding ID 双格式
- `homoglyph.rs:425-440` 手写 ID 用 `hash[..20]` + 带 snippet salt；
- 共享 `finding_builder::make_finding_id` 用 `hash[..16]` + 无 snippet。
- `scanner.rs:1511-1539` 的 symlink finding 绕过 builder，`id` 可能为空，参与 `dedupe_issues`（`:394`）时存在碰撞隐患（虽然 dedup key 含 rule_id+path+line+snippet，碰撞概率低）。

### L5 可访问性：`CyberSelect` 不可用
`ui/CyberSelect.tsx:62-103` 自定义下拉框**无任何 ARIA role / 键盘导航**（缺 `role="listbox"`/`option"`、`aria-expanded`、方向键/Enter/Escape），键盘和读屏用户无法操作。`SecurityDashboard`、`MarketplacePage`、`InstalledSkillsPage` 都在用。

### L6 一致性：`ErrorBoundary` 兜底 UI
`ErrorBoundary.tsx:34,43,49` 用硬编码 `bg-gray-800/text-red-400/bg-blue-600`，不走主题 token（`bg-card`/`text-destructive`），亮色模式视觉割裂；也无 prop 变化时的 reset，只能手动点 Retry。

### L7 前端：URL scheme 无白名单
6 处 `href={...repository_url}`（`InstalledSkillsPage.tsx:2097,2264,2454`、`MarketplacePage.tsx:881,1030`、`RepositoriesPage.tsx:697`）未做 `http(s):` scheme 白名单。CSP 与 OS 链接处理器能兜底，但渲染端没显式校验；若 DB 存了 `javascript:`/`data:`，理论上可执行。

### L8 质量：`catch (error: any)` 冗余
`InstalledSkillsPage.tsx` 约 17 处（`405,434,542,568,1182,1195,1289,2308` 等）`catch (error: any)`，而 `safeInvoke` 已产出类型化 `ApiError`，可直接读 `.message`，绕过 `tsconfig.json:18` 的 `strict: true`。

### L9 一致性：阈值与清单双份维护
- `consistency_checker.rs:457` 硬编码 "至少 5 词"，与 `policy.trigger.min_description_length`（10 字符）是两套独立阈值；
- `analyzability.rs:167-192` 的 `is_inert_asset` 重复 `policy.rs:266-275` 的 `default_inert_extensions`，两份清单可漂移，且没有 `policy.rs:538` 那样的同步测试。

### L10 i18n：en 非唯一真相源
`ToolIcons.tsx:144,187,251,261,272`、`InstalledSkillsPage.tsx:915,1214,1236` 以**中文字面量**作 `t()` fallback。en/zh 各 803 key 严格对齐（✅），但这些点 en 不是唯一真相源。

### L11 质量：`ErrorBoundary` 无 reset
见 L6。无 `componentWillUnmount` / prop 变化 reset，错误态只能手动 Retry。

### L12 静态初始化哲学不一致
`policy.rs:409` 对静态 YAML 用 `expect`（fail-fast），但 `loader.rs:94-116` 的 `load_merged_builtin_rules` 对内置规则编译错误用 `eprintln!` 吞掉并返回空/部分规则集 —— 一条坏规则会静默失效。两条静态初始化路径哲学不一致。

---

## ✅ 做得好的地方

**安全敏感路径（重点核查）：**
- **Shell 执行零注入面**：所有 CLI 驱动（Claude PTY、npm/pip/brew/scoop/choco 更新卸载）都用 argv 传递，包名是独立参数；Windows `.cmd/.bat` 正确用 `cmd /d /c` 包裹（`claude_cli.rs:170-176`）。
- **Zip-slip 防护到位**：`github.rs:613-655` 用 `enclosed_name()` + 额外 `is_safe_path`（`:19-35`）双重校验，失败条目跳过而非 panic；下载有 100MiB `Content-Length` + 实际字节双重大小上限（`:559-578`）。
- **SQL 全参数化**：DDL 静态，动态 `IN (?, ?2, ...)` 用 `params_from_iter`（`database.rs:624-634`）。
- **DB 恢复审慎**：WAL + busy_timeout + foreign_keys；mutex 中毒显式 ROLLBACK（`database.rs:170-181`）；corruption 只在 `SQLITE_CORRUPT/NOTADB` 时备份重建，BUSY/PERM/FULL/CANTOPEN/READONLY 保留原文件（`lib.rs:388-417`，含测试）。
- **更新签名校验**：minisign pubkey 已配置，`createUpdaterArtifacts: true`。
- **XSS 面干净**：全仓零 `dangerouslySetInnerHTML` / `eval` / `innerHTML` / `document.write`；自研 markdown 解析器输出结构化 React 节点（`SettingsPage.tsx:51-201`）。
- **CSP 基本 OK**：`script-src 'self'` 无 `unsafe-eval`，`withGlobalTauri: false`，`connect-src` 限定 GitHub + IPC。
- **ReDoS 风险低**：用线性时间 `regex` 引擎，2MiB/文件 + 2000 文件上限。
- **TOCTOU 面低**：`walkdir` 配 `follow_links(false)`，symlink 显式硬阻断产 `SYMLINK` finding（`scanner.rs:1511-1539`）。

**架构与工程：**
- **状态组装清晰**：`AppState`（`commands/mod.rs:64-72`）用 `Arc<Database>` + `Arc<Mutex<SkillManager/PluginManager>>` + `Arc<GitHubService>` + `RwLock<Option<CliScanCache>>` + `tokio::sync::Mutex<()>` 串行化 CLI 变更。
- **扫描引擎分层合理**：`SecurityScanner` 负责遍历/装配，`SkillContext` 一次构建供所有 analyzer 复用，各 analyzer 独立模块统一 `fn analyze(...) -> Vec<Finding>` 形态。
- **防御推理有文档化**：如 `pipeline.rs:630-665` 解释 installer 白名单为何降级而非豁免，URL 必须在真实下载行匹配，并带回归测试（`:1427-1495`）。
- **rule_matrix 测试夹具完备**：P0~P2 正负样本覆盖签名、流水线、一致性、Unicode、伪装等类别。
- **每个 tab 独立 `ErrorBoundary`**（`App.tsx:416-492`），单页崩溃不连累全局。
- **IPC 错误处理一致**：`safeInvoke`（`api-error.ts:35-41`）统一产出 `ApiError`，`error-codes.ts` 用正则匹配 `[CODE]`/`CODE:` 前缀翻译。
- **阻塞/异步纪律好**：重活一致包在 `tokio::task::spawn_blocking`（`commands/security.rs:25,43`、`commands/mod.rs:607,681,1472` 等），mutex 中毒处处理处显式 `into_inner` 恢复。
- **i18n 中英 key 严格对齐**：各 803 key / 892 行，无缺译。

---

## 建议优先级

| 顺序 | 事项 | 工作量 | 对应 |
|---|---|---|---|
| 1 | `uninstall_skill_path` 加 allowlist + canonicalize（对齐 `open_skill_directory`） | 小 | H1 |
| 2 | 移除前端 `openPath` fallback，或收窄 `opener:allow-open-path` | 小 | H2/H3 |
| 3 | `from_github_url`/`add_repository` 校验 host 与 `[A-Za-z0-9._-]+`，缓存目录名 sanitize | 中 | H4 |
| 4 | `zod` 真正落地，至少给 `SecurityReport`/`Skill` 加运行时 schema | 中 | M3 |
| 5 | `staging_path` 加 cache-containment 校验 | 小 | M1 |
| 6 | 文档明确"下载无签名校验，扫描器是启发式而非信任闸门" | 小 | M2 |
| 7 | 扫描器 skill 内并行 + 合并三处文件 I/O | 中 | M4/M5 |
| 8 | 统一 `is_binary` 判定、finding ID 格式、installer 域匹配 | 小～中 | M6/L2/L3/L4 |
| 9 | `CyberSelect` 补 ARIA + 键盘导航；`ErrorBoundary` 走主题 token + reset | 中 | L5/L6 |
| 10 | `href` scheme 白名单；清理 `catch (error: any)` | 小 | L7/L8 |

---

*本 review 仅诊断，未修改任何代码。如需就某一条动手修复，可基于上方优先级表直接展开。*
