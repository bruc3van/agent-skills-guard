# Agent Skills Guard 深度 Review

- **日期**：2026-08-10
- **工具**：Grok Build（xAI Grok TUI）
- **模型**：Grok 4.5（`grok-4.5`）
- **版本**：v1.3.6（commit `727457f`）
- **范围**：`src-tauri/src/`（安全引擎 + 服务层 + IPC 命令层）、`src/`（React 前端）、Tauri 配置与能力声明、构建与 CI
- **性质**：仅诊断，本轮未修改任何业务代码
- **方法**：通读架构与安全关键路径，交叉验证现有 review 结论，并实测 typecheck / 单测基线

---

## 结论摘要

整体工程素养明显高于一般个人 / 小团队 Tauri 项目：生产代码近乎零 `unwrap()`、重 IO/CPU 路径普遍 `spawn_blocking`、SQL 参数化、ZIP zip-slip 防护到位、更新走 Tauri 签名校验、扫描引擎有误报治理与策略层。

本轮确认 **5 个 P1 + 4 个 P2 + 4 个 P3**。最值得注意的是两类：

1. **防护不一致**——「打开目录」有 canonicalize + 白名单，更危险的「删除目录」却零校验；
2. **状态一致性**——缓存解压非原子、更新清空失败仍 merge 写入、跨 Skill 扣分不落库，导致「报告 / SHA / 磁盘」可能分叉。

| 编号 | 问题 | 等级 | 类别 | 位置 |
|---|---|---|---|---|
| G1 | `uninstall_skill_path` 删除调用方传入的任意路径，无归属校验 | 🔴 P1 | 文件系统 / 信任边界 | `skill_manager.rs:1609-1636` |
| G2 | 仓库缓存解压非原子，可能继续扫描旧 commit | 🔴 P1 | 缓存一致性 | `github.rs:589-609` |
| G3 | 更新时清空目标失败仍覆盖写入，安全报告与磁盘分叉 | 🔴 P1 | 安装一致性 | `skill_manager.rs:2788-2808` |
| G4 | ZIP 解压无解压后体积 / 条目上限（zip bomb） | 🔴 P1 | 资源耗尽 | `github.rs:612-654` |
| G5 | 跨 Skill 协同攻击重算分数不落库，重启后风险消失 | 🔴 P1 | 持久化一致性 | `commands/security.rs:134-189` |
| G6 | 安装链路仍在 async 命令里跑同步扫描 | 🟠 P2 | 性能 / 阻塞 | `commands/mod.rs:635-668` |
| G7 | cross-skill 在并行扫描后再串行重读全部技能目录 | 🟠 P2 | 性能 / 磁盘 IO | `commands/security.rs:135-150` |
| G8 | 单路径卸载失败时仍可能改 DB `local_paths` | 🟠 P2 | 元数据一致性 | `skill_manager.rs:1638-1665` |
| G9 | **无 PR/push CI**：测试、typecheck、clippy 不自动运行 | 🟠 P2 | 工程流程 | `.github/workflows/release.yml` |
| G10 | `opener:allow-open-path` 授权 `path:"**"` 过宽 | 🟡 P3 | Tauri 能力 | `capabilities/default.json` |
| G11 | 自动应答 Claude Code 的 workspace trust 提示 | 🟡 P3 | 产品 / 安全语义 | `claude_cli.rs:240-251` |
| G12 | 全局 `Mutex<Connection>` 队头阻塞 | 🟡 P3 | 架构 | `database.rs` |
| G13 | 巨型模块（scanner / skill_manager / local_cli_scanner 各 3k–4k+ 行） | 🟡 P3 | 可维护性 | 多处 |

**建议修复顺序**：G1 → G3 → G2 → G4 → G5 → G9 → G6 → G8 → G10 → G7 → G11 → G12/G13

---

## 验证基线

本轮实测（`727457f`，本机 macOS）：

| 检查项 | 命令 | 结果 |
|---|---|---|
| 前端类型 | `pnpm typecheck` | ✅ 通过 |
| 前端单测 | `pnpm test:unit` | ✅ 26 文件 / 89 用例全绿 |
| Rust 库测 | `cargo test --lib` | ✅ **510 用例全绿**（约 3.3s） |

代码体量（生产 Rust 约 3.6 万行，节选）：

| 文件 | 约行数 | 角色 |
|---|---:|---|
| `security/scanner.rs` | 4521 | 核心扫描器 |
| `services/skill_manager.rs` | 4398 | 安装生命周期 |
| `services/local_cli_scanner.rs` | 3633 | CLI 发现 |
| `services/plugin_manager.rs` | 2794 | 插件管理 |
| `security/pipeline.rs` | 1920 | 多步攻击链 |

与同日其他 review 文档（Claude Opus 5 / Codex GPT-5 / ZCode GLM）结论高度重合，尤其是任意路径卸载、ZIP bomb、缓存非原子、更新 merge 残留——可视为可复核问题，而非单次模型幻觉。

---

## 项目画像

**Agent Skills Guard** 是 Tauri 2 + React 18 + Rust 桌面应用，定位为 Claude Code / Agent Skills 的「应用商店 + 安全卫士」：

| 能力 | 实现要点 |
|---|---|
| 技能发现 / 安装 / 更新 / 卸载 | GitHub zipball、staging 扫描、多工具软链同步 |
| 安全扫描 | 规则引擎 + pipeline 污点链 + Unicode / Magic / 跨 Skill |
| 插件市场 | Claude marketplace 同步 + 精选 YAML 远程刷新 |
| 本地 CLI 管理 | npm / pnpm / pip / brew / scoop / choco 发现与更新 |

### 架构速览

```
Renderer (React)
    │ invoke / safeInvoke
    ▼
Tauri commands (mod / security / plugins / local_cli)
    │
    ├─ SkillManager / PluginManager / GitHubService
    ├─ SecurityScanner + pipeline / cross_skill / policy
    └─ Database (rusqlite, Mutex<Connection>)
```

分层清晰（command → service → model/security），扫描策略（`ScanPolicy`）编译期嵌入。代价是 `skill_manager` / `scanner` / `local_cli_scanner` 已成上帝文件，改动回归成本高。

---

## G1 🔴 `uninstall_skill_path` 可删除任意路径

**位置**：

- `src-tauri/src/services/skill_manager.rs:1609-1636`
- IPC：`src-tauri/src/commands/mod.rs:719-729`
- 前端透传：`src/lib/api.ts` → `uninstallSkillPath(skillId, path)`

### 证据

```rust
pub fn uninstall_skill_path(&self, skill_id: &str, path_to_remove: &str) -> Result<()> {
    let mut skill = /* 从 DB 取 skill */;
    let path = PathBuf::from(path_to_remove);  // 直接来自前端参数
    if link_fs::is_dir_link(&path) {
        link_fs::remove_dir_link(&path)        // 无归属校验
    } else if path.exists() {
        if path.is_dir() {
            std::fs::remove_dir_all(&path)     // 无归属校验
        } else {
            std::fs::remove_file(&path)
        }
    }
    // 删除之后才从 local_paths 过滤
}
```

### 为什么是「不一致」而不只是「缺校验」

同仓库 `open_skill_directory`（`commands/mod.rs:991-1044`）做了完整防护：

1. `canonicalize()`
2. 白名单：`~/.claude`、`~/.agents`、各 AgentTool 技能目录、应用缓存、DB 中全部 `local_paths` / `local_path`
3. 不命中返回 `[PATH_NOT_ALLOWED]`

**「打开一个目录」的防护强度远高于「递归删除一个目录」**——能力具备，只是漏加在更危险的一侧。

### 威胁模型

只要有一个有效 `skill_id`，IPC 调用者即可要求后端递归删除任意可写路径。正常 UI 当前传入登记路径，但文件删除权限边界不能依赖：

- 前端永远传对参数
- WebView / renderer 永不被注入或误调用

### 建议修复

1. 任何 FS 操作前，规范化请求路径与 DB 登记路径
2. 要求请求路径与该 skill 的某个已登记路径**完全匹配**（不要仅用前缀）
3. 链接用 `symlink_metadata`，避免 canonicalize 误跟到链接源
4. 拒绝空路径、根目录、用户主目录、无法归属的路径
5. 测试矩阵：任意临时目录、父目录、相似前缀、合法登记路径、链接路径

---

## G2 🔴 仓库缓存解压非原子，可能继续扫旧 commit

**位置**：`src-tauri/src/services/github.rs:589-609`，以及 `extract_commit_sha_from_cache` / `find_repo_root`

### 机制

`download_repository_archive()` 将新 zipball 直接解压到既有 `extracted/`：

1. **未先清理**旧解压目录
2. 新旧 commit 根目录可能同时存在
3. 旧 `.commit_sha` 仍在时，`extract_commit_sha_from_cache()` 可能优先返回旧 SHA
4. `find_repo_root()` 返回 `read_dir()` 枚举到的第一个目录，不保证是新 commit
5. DB 可能继续保存旧 SHA → 后续反复判定「远端有更新」并重复下载

### 影响

- 用户看到的 Skills 可能已陈旧
- 自动更新路径进入「下载成功但基线不前进」的循环
- 显式「清缓存后刷新」可绕过，但普通自动更新不会

### 建议修复

1. 解压到同文件系统上的临时目录
2. 仅从本次下载产生的根目录推导 commit SHA
3. 完整解压并校验成功后，原子 rename 替换既有 `extracted/`
4. 回归测试：旧 SHA → 新 SHA 连续两次下载，断言只剩一个根目录、返回新 SHA

---

## G3 🔴 更新清空失败仍覆盖写入 → 报告与磁盘分叉

**位置**：`src-tauri/src/services/skill_manager.rs:2773-2808`

### 证据

更新确认阶段先扫描 staging 并生成 `scan_report`。当目标目录需要清空而 `remove_dir_all()` 失败时：

```rust
if let Err(clear_err) = std::fs::remove_dir_all(&target_install_dir) {
    log::warn!(
        "无法清空旧技能目录，将尝试直接覆盖写入（可能保留部分旧文件）: {}",
        clear_err
    );
}
// 随后 copy_dir_recursive + apply_scan_report(staging 的报告)
```

### 影响

- 磁盘 = 新文件 + **残留旧文件**（merge）
- DB 保存的是 **staging 扫描报告**（旧恶意脚本若在新版已删除，报告会变「干净」）
- 安全产品语义被破坏：UI 显示安全，实际可执行树仍含未扫描残留

### 建议修复

1. 清空失败立即终止更新，并恢复备份
2. 禁止将目录 merge 写入作为更新降级路径
3. 更稳妥：准备完整新目录后 rename 原子替换
4. 回归测试：旧版含危险脚本、新版删除该脚本；模拟清空失败 → 更新失败、旧安装完整恢复、DB 报告不变

---

## G4 🔴 ZIP 解压无解压后资源预算

**位置**：`src-tauri/src/services/github.rs:612-654`

### 已有防护（做得对）

- 压缩包大小上限 `MAX_ARCHIVE_BYTES = 100 MiB`（Content-Length + 实际字节双重检查）
- `enclosed_name` + `is_safe_path` 防 zip-slip

### 缺失

- 无解压后总字节上限
- 无条目数量上限
- 无单文件膨胀比 / 实际写入流式限额
- `std::io::copy` 可写爆磁盘

恶意仓库可构造「压缩后 &lt; 100MB、解压后巨大」的 zip bomb，耗尽缓存盘空间。

### 建议修复

1. 累计 `uncompressed_size` 与实际写入字节，设硬上限（例如 500 MiB 解压总量）
2. 限制最大条目数
3. 超限中止并清理临时目录
4. 单测：构造高膨胀比 fixture 断言失败且无残留

---

## G5 🔴 跨 Skill 协同攻击结果不落库

**位置**：`src-tauri/src/commands/security.rs:134-189`

### 机制

`scan_all_installed_skills` 在并行单 skill 扫描并 `db.save_skill` 之后：

1. 再遍历全部技能目录构建 `SkillScanContext`
2. `cross_skill::analyze_skill_set` 得到协同攻击 findings
3. 将 findings 追加到内存中的 `results`，并重算 `score` / `level`
4. **没有**再次 `db.save_skill` 写回

单 skill 扫描路径本身也不做 cross-skill。

### 影响

- 全量扫描 UI 瞬时可见跨 Skill 风险
- 重启后从 DB 加载的分数 / issues **不含**跨 Skill 扣分
- README 主打卖点在持久化层面断层

### 建议修复

1. 重算后写回 DB（issues + score + level + scanned_at）
2. 或独立表存 cross-skill findings，启动 / 全扫后统一合并展示
3. 明确产品语义：跨 Skill 仅全量扫描有效，还是单扫也触发邻域分析

---

## G6 🟠 安装链路同步扫描仍堵 async

**位置**：`commands/mod.rs` 的 `install_skill` / `prepare_skill_installation`

批量扫描路径已正确 `spawn_blocking` 并注释说明原因。安装 / 准备安装仍在持有 `skill_manager` async lock 时跑同步下载 + 扫描，会占住 tokio worker，其他 IPC 排队（界面「点什么都没反应」）。

### 建议

与 `confirm_skill_installation` / `scan_all_installed_skills` 对齐：整段重活放入 `spawn_blocking`，或至少把扫描段拆出阻塞线程。

---

## G7 🟠 cross-skill 二次磁盘遍历

并行扫描已读过各 skill 目录，随后为 cross-skill 再串行 `build_scan_context_from_skill_dir`，技能多时磁盘 IO 翻倍。

### 建议

扫描阶段缓存构建 context 所需摘要（域名、敏感读写特征等），cross-skill 只消费内存摘要。

---

## G8 🟠 卸载失败仍可能丢掉追踪元数据

`uninstall_skill_path` 在 FS 删除报错时仍会更新 / 过滤 `local_paths` 并 `save_skill`。若磁盘删除失败但元数据已删，残留文件无法再经应用卸载。

### 建议

FS 成功后再改 DB；或失败时保留 path 并返回明确错误，允许重试。

---

## G9 🟠 无 PR/push CI

`.github/workflows/release.yml` 仅在 tag / `workflow_dispatch` 发版。开发期 `typecheck` / `test:unit` / `cargo test` / `clippy` 全靠本机自觉。

### 建议

新增 `ci.yml`（push + PR）：

1. `pnpm typecheck` + `pnpm test:unit`
2. `cargo test`（可先 `--lib` 控时）
3. 可选：`cargo clippy --all-targets -- -D warnings`（可分阶段启用）

---

## G10–G13 🟡 次要项

### G10 Tauri opener 过宽

`capabilities/default.json` 中 `opener:allow-open-path` 为 `path: "**"`。应收窄到技能目录、工具目录、应用缓存等已知前缀（与 `open_skill_directory` 白名单对齐）。

### G11 自动确认 workspace trust

`claude_cli.rs` 检测到 trust 提示自动回车。省事，但扩大了非交互自动化的信任面。建议：可配置开关，默认记录审计日志，或仅在用户已确认的安装流程中启用。

### G12 全局 DB 互斥锁

`Mutex<Connection>` 导致写多读多时队头阻塞。已知取舍；中期可考虑连接池 / 读写分离 / 把重查询移出热路径。

### G13 巨型模块

`scanner` / `skill_manager` / `local_cli_scanner` 各 3k–4k+ 行，review 与回归成本高。中期按「安装确认 / 路径解析 / 规则匹配 / 包管理器发现」拆模块，比继续堆函数更可持续。

---

## 安全引擎（产品核心）评估

设计是多层流水线，而非单层正则：

1. **规则引擎**（YAML + pattern engine）
2. **Pipeline**（curl|sh、chmod→exec、敏感文件→外传、base64 解码执行等）
3. **Homoglyph / 零宽 / Magic 签名**
4. **Consistency / Strict structure / Analyzability**
5. **Cross-skill**
6. **Policy**（硬触发、安装器域名降级、文档降级、score kinds）

可见的误报治理：

- `INSTALLER_DOWNGRADABLE_RULES`：已知安装器域名降权、取消 hard block
- 包安装在展示型语句 / 字面量中抑制
- subprocess `shell=True` 有限窗口，避免跨调用误关联
- 扫描 cap：深度 20、单文件 2 MiB、跳过 `node_modules` 等

**预期内局限**（宜在对外文档写清）：

- 静态模式为主，无完整 AST / 数据流；混淆与动态调用可绕
- 扫描的是 Skill 文本，**不执行** Skill；运行时风险仍依赖宿主 Agent
- 规则质量决定误报 / 漏报；rule_matrix fixture 是正确方向，应持续加边界 case

---

## 做得好的实践

| 领域 | 观察 |
|---|---|
| 错误处理 | 生产路径极少 `unwrap()`；结构化错误码 |
| 并发 | 批量扫描 / CLI 发现 `spawn_blocking`；注释写清「为何不能堵 async」 |
| SQL | 基本 `params![]`；动态 IN 用占位符 |
| 归档 | zip-slip 防护 + 压缩体积上限 |
| 更新 | Tauri updater 公钥签名 |
| CSP | 限制 GitHub 相关域名；`withGlobalTauri: false` |
| 进程 | 参数数组起进程，无 shell 拼接注入面 |
| 前端 | `safeInvoke`、ErrorBoundary、导航保护、页面 lazy load |
| 产品 | 中英 i18n、卸载确认、软链 vs 真目录区分（`link_fs`） |
| 测试 | Rust 500+、前端 89；规则有 fixture |

---

## 前端与测试覆盖

- App 结构清楚：React Query + 页面 lazy + Local CLI「首次访问后常驻」
- 测试偏工具函数与部分组件
- **安装 / 卸载 / 扫描主路径 E2E 几乎没有**——桌面端可理解，但 G1–G3 类回归应优先用 Rust 单测补齐

---

## 修复优先级（可执行清单）

1. **[G1]** `uninstall_skill_path` 归属校验（与 `open_skill_directory` 对齐）+ 测试矩阵  
2. **[G3]** 更新清空失败即失败并回滚，禁止 merge 写入  
3. **[G2]** 缓存解压临时目录 + 原子替换 + SHA 只从本次根推导  
4. **[G4]** ZIP 解压总字节 / 条目预算  
5. **[G5]** cross-skill 分数与 issues 落库  
6. **[G9]** 增加 PR CI（typecheck + unit + cargo test）  
7. **[G6][G8]** 安装链路阻塞与卸载元数据事务性  
8. **[G10][G11]** 收紧 opener、trust 自动确认可配置  
9. **[G7][G12][G13]** 性能与模块拆分（中期）

---

## 一句话总结

产品定位清晰，安全扫描与工程素养已达到可对外发布水准，测试与边界处理明显用心。当前最该修的不是功能缺口，而是 **文件系统信任边界** 与 **缓存 / 更新原子性**：一边有完整 path allowlist，另一边危险删除 / 覆盖路径却放过。修完 G1–G5 后，整体可信度会再上一个台阶。

---

## 与同日其他 review 的对照

| 本轮 | Claude Opus 5 | Codex GPT-5 | 说明 |
|---|---|---|---|
| G1 | C1 | R3 | 任意路径卸载，三方一致 |
| G2 | — | R1 | 缓存非原子 / 旧 SHA |
| G3 | — | R2 | 更新 merge 残留 |
| G4 | C3 | R5 | zip bomb |
| G5 | C2 | — | cross-skill 不落库 |
| G6 | C4 | — | 安装链路阻塞 |
| G9 | P1 | — | 无 PR CI |

差异主要来自审查侧重（信任边界 vs 缓存原子性 vs 持久化），**核心 P1 集合互补而非矛盾**。
