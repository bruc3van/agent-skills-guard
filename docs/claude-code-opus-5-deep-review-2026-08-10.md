# 全项目深度 Review（安全 + 性能 + 工程流程）

- **日期**：2026-08-10
- **工具**：Claude Code
- **模型**：Opus 5（`claude-opus-5`）
- **版本**：v1.3.6（commit `727457f`，工作区仅有两份未提交的同期 review 文档）
- **范围**：`src-tauri/src/`（安全引擎 + 服务层 + IPC 命令层）、`src/`（React 前端）、Tauri 配置与能力声明、构建与 CI 配置
- **性质**：仅诊断，本轮未修改任何代码
- **方法**：先跑通验证基线（`pnpm typecheck` / `pnpm test:unit` / `cargo test` / `cargo clippy --all-targets`），再沿「攻击面 → 数据流 → 持久化一致性 → 运行时阻塞」四条线深读

---

## 结论摘要

整体工程素养明显高于一般个人项目：**生产代码近乎零 `unwrap()`**（scanner / database / migration 三个大文件为 0）、注释普遍写清了「为什么这样做」而非「这里做了什么」、shell 执行全程参数数组无注入面、SQL 全参数化、zip-slip 防护到位、更新走 Tauri 签名校验。上一轮 [`perf-stability-review-2026-08-07.md`](./perf-stability-review-2026-08-07.md) 的 R1–R7 基本都已落地。

本轮确认 **3 个 P1 + 2 个 P2 + 3 个 P3**，外加 1 项流程级缺口。其中最值得注意的是两类：

1. **防护不一致**——同一代码库内，「打开目录」做了 canonicalize + 白名单，而危险得多的「删除目录」却零校验；
2. **持久化断层**——跨 Skill 协同攻击的扣分只存在于内存，重启即消失，直接削弱 README 主打的卖点。

| 编号 | 问题 | 等级 | 类别 | 位置 |
|---|---|---|---|---|
| C1 | `uninstall_skill_path` 删除调用方传入的任意路径，无归属校验 | 🔴 P1 | 文件系统/信任边界 | `skill_manager.rs:1609-1636` |
| C2 | 跨 Skill 协同攻击的重算分数从不落库，重启后风险消失 | 🔴 P1 | 持久化一致性 | `commands/security.rs:115` ↔ `:134-190` |
| C3 | ZIP 解压无解压后体积/条目上限（zip bomb） | 🔴 P1 | 网络/资源耗尽 | `github.rs:613-655` |
| C4 | 安装链路仍在 async 命令里跑同步扫描（R2 残留） | 🟠 P2 | 性能/阻塞 | `commands/mod.rs:635-668`、`skill_manager.rs:803` |
| C5 | cross-skill 上下文在并行扫描后串行重读全部技能目录 | 🟠 P2 | 性能/磁盘 IO | `commands/security.rs:135-150` |
| C6 | `opener:allow-open-path` 授权 `path:"**"` 过宽 | 🟡 P3 | Tauri 能力 | `capabilities/default.json:17-20` |
| C7 | 自动应答 Claude Code 的 workspace trust 提示 | 🟡 P3 | 产品/安全语义 | `claude_cli.rs:240-251` |
| C8 | 全局 `Mutex<Connection>` 队头阻塞（已知取舍，待演进） | 🟡 P3 | 架构 | `database.rs:129-147` |
| P1 | **无 PR/push CI**：测试、typecheck、clippy 全不自动运行 | 🟠 P2 | 工程流程 | `.github/workflows/release.yml` |

**建议修复顺序**：C1 → C2 → P1 → C3 → C4 → C6 → C5 → C7 → C8

---

## 验证基线

本轮所有结论建立在以下实测结果之上（`727457f`，本机 darwin 25.5.0）：

| 检查项 | 命令 | 结果 |
|---|---|---|
| 前端类型 | `pnpm typecheck` | ✅ 通过，零错误 |
| 前端单测 | `pnpm test:unit` | ✅ 26 文件 / 89 用例全绿（2.08s） |
| Rust 测试 | `cargo test` | ✅ 510 + 4 + 1 = **515 用例全绿** |
| Rust lint | `cargo clippy --all-targets` | ⚠️ exit 0，但 **58 条 warning**（全为 style 类） |

生产代码 `unwrap()` 统计（排除 `#[cfg(test)]` 之后的模块）：

| 文件 | 生产代码 `unwrap()` |
|---|---|
| `security/scanner.rs`（4521 行） | **0** |
| `services/database.rs`（1713 行） | **0** |
| `services/migration.rs`（647 行） | **0** |
| `services/skill_manager.rs`（4398 行） | 1 |
| `services/local_cli_scanner.rs`（3633 行） | 1 |

这是本次 review 中最能说明代码质量的单项数据。

---

## C1 🔴 `uninstall_skill_path` 可删除任意路径

**位置**：`src-tauri/src/services/skill_manager.rs:1609-1636`，经 `commands/mod.rs:718-730` 暴露为 IPC。

### 证据

```rust
pub fn uninstall_skill_path(&self, skill_id: &str, path_to_remove: &str) -> Result<()> {
    let mut skill = /* ...从 DB 取 skill... */;

    let path = PathBuf::from(path_to_remove);       // ← 直接来自前端参数
    if link_fs::is_dir_link(&path) {
        link_fs::remove_dir_link(&path)             // ← 无归属校验
    } else if path.exists() {
        if path.is_dir() {
            std::fs::remove_dir_all(&path)          // ← 无归属校验
        } else {
            std::fs::remove_file(&path)
        }
    }

    // 删除**之后**才从 local_paths 里过滤
    let normalized_remove = normalize_path_for_compare(&path);
    if let Some(mut paths) = skill.local_paths.clone() {
        paths.retain(|p| normalize_path_for_compare(&PathBuf::from(p)) != normalized_remove);
        ...
    }
}
```

参数从 `src/lib/api.ts:128` 的 `uninstallSkillPath(skillId, path)` 一路透传到 `remove_dir_all`，全程没有验证 `path_to_remove` 是否属于 `skill.local_paths`。删除动作发生在过滤之前——即便该路径根本不在列表里，目录也已经消失了。

### 为什么这是「不一致」而不只是「缺校验」

同一代码库的 `commands/mod.rs:974-1045`（`open_skill_directory`）做了完整防护：`canonicalize()` 之后逐一比对 `~/.claude`、`~/.agents`、各 AgentTool 技能目录、应用缓存目录、以及 DB 中登记的全部 `local_paths` 前缀，不命中就返回 `[PATH_NOT_ALLOWED]`。

**「打开一个目录」的防护强度远高于「递归删除一个目录」**——这说明防护能力是具备的，只是漏加在了更危险的那一侧。

### 建议

在删除前置一道归属校验，复用已有的 `normalize_path_for_compare`：

```rust
let normalized_remove = normalize_path_for_compare(&path);
let belongs = skill.local_paths.as_ref().is_some_and(|paths| {
    paths.iter().any(|p| normalize_path_for_compare(&PathBuf::from(p)) == normalized_remove)
});
if !belongs {
    anyhow::bail!("UNINSTALL_PATH_NOT_TRACKED: {}", path_to_remove);
}
```

> **交叉印证**：Codex+GPT-5 报告记为 R3、ZCode+GLM-5.2 报告记为 H1。**三份独立评审一致命中，应作为最高优先级处理。**

---

## C2 🔴 跨 Skill 协同攻击的评分从不落库

**位置**：`src-tauri/src/commands/security.rs`，`scan_installed_skills_blocking`。

### 证据

时序如下：

| 行 | 动作 |
|---|---|
| `:88-103` | 并行扫描每个 skill，得到单体 `report` |
| `:108-117` | 把单体分数写入 DB：`db.save_skill(&updated)` |
| `:135-150` | 重新构建 cross-skill 上下文 |
| `:152-190` | 追加 cross-skill findings，**重算 `score` / `level`** |

关键在于 `:152-190` 这段：

```rust
for (_, result) in &mut results {
    result.report.issues.extend(cross_issues);
    let new_score = SecurityScanner::score_from_issues(&result.report.issues, result.report.blocked);
    result.score = new_score;
    result.report.score = new_score;
    result.level = SecurityLevel::from_score(new_score).as_str().to_string();
    result.report.level = SecurityLevel::from_score(new_score);
}
```

它**只修改内存里的 `results`（即本次 IPC 的返回值），没有任何 `db.save_skill`**。而 DB 里存的是 `:115` 写入的、未包含 cross-skill 扣分的旧报告。

### 可观察后果

1. 全量扫描当次：界面显示「发现跨 Skill 协同攻击、分数已扣」；
2. 重启应用（或切页触发 `get_scan_results` 读缓存）：`collect_scan_results` 从 DB 读到未扣分的报告，**协同攻击的 issue 与扣分双双消失**；
3. 用户看到同一批技能在两个时刻有两个不同的安全评分，且更安全的那个是错的。

README 将「跨 Skill 协同攻击检测：发现多技能间的数据中继、共享恶意域名等协同攻击行为」列为核心卖点之一，这个持久化断层会让该能力在多数使用路径下不可见。

### 建议

在 `:190` 的 `if` 块结束前，对每个受影响的 skill 回写 DB。注意需要同时更新 `security_score` / `security_level` / `security_issues` / `security_report` 四个字段，与 `:109-113` 保持一致。

> **交叉印证**：本轮独有，另两份报告均未涉及。

---

## C3 🔴 ZIP 解压没有解压后资源预算

**位置**：`src-tauri/src/services/github.rs:613-655`（`extract_zip`）。

### 证据

路径遍历防护做得很扎实——`file.enclosed_name()` 之外还叠了一层自实现的 `is_safe_path`（`github.rs:19-35`，逐 component 归一化后比对前缀）。**但体积维度完全没有闸门**：

```rust
for i in 0..archive.len() {                      // ← 无条目数上限
    let mut file = archive.by_index(i)?;
    ...
    let mut outfile = File::create(&outpath)?;
    std::io::copy(&mut file, &mut outfile)?;     // ← 无累计写入上限
}
```

上游只有 `MAX_ARCHIVE_BYTES = 100 * 1024 * 1024`（`github.rs:13`）的**压缩后**限制。而 DEFLATE 对高度重复数据的压缩比轻易超过 1000:1——100 MiB 的恶意归档展开到 100 GB 属于常规构造，且仓库内容完全由攻击者控制（任意 GitHub 仓库均可被用户添加）。

### 次要问题：先缓冲后检查

`github.rs:570` 的 `response.bytes().await` 会把整个响应体读进内存，**之后**才在 `:572` 检查大小。当 `content_length` 缺失（chunked 传输）时，`:559-567` 的前置检查会被跳过，理论上存在 OOM 路径。

不过 host 硬编码为 `https://api.github.com`（`github.rs:126`），攻击者需要先控制 GitHub 的响应，**实际可利用性很低**。改成流式写盘 + 边写边计数可以顺手把这一项和上面的 zip bomb 一起解决。

### 建议

在解压循环中累计已写字节，超过阈值即中止并清理已落盘的部分：

```rust
const MAX_EXTRACTED_BYTES: u64 = 500 * 1024 * 1024;
const MAX_ENTRIES: usize = 20_000;

if archive.len() > MAX_ENTRIES { anyhow::bail!("归档条目数超过安全上限"); }

let mut written: u64 = 0;
// 循环内：
written += std::io::copy(&mut file, &mut outfile)?;
if written > MAX_EXTRACTED_BYTES {
    let _ = fs::remove_dir_all(extract_dir);
    anyhow::bail!("解压体积超过安全上限，可能为恶意归档");
}
```

> **交叉印证**：Codex+GPT-5 报告记为 R5。**两份独立评审一致命中。**（ZCode 报告判定「zip-slip 防护到位」——就路径遍历而言这个判断正确，两者关注的是不同维度。）

---

## C4 🟠 安装链路仍在 async 命令里跑同步扫描（R2 残留）

**位置**：`src-tauri/src/commands/mod.rs:635-668`、`src-tauri/src/services/skill_manager.rs:803`。

### 证据

上一轮 review 的 R2 结论（「同步重活跑在 async 命令 / tokio 工作线程上」）在扫描与插件链路上已经修得很规范，`commands/security.rs:40-48` 甚至留了详尽的注释说明原因：

> 扫描是纯 CPU + 磁盘 IO 的同步任务。此前直接在 async 命令里 `pool.install(...)`，会阻塞 tokio 工作线程直到全部扫完，期间其他 IPC 请求排队 —— 这正是扫描期间整个界面卡顿的原因。

**但安装链路漏掉了**：

```rust
// commands/mod.rs:654-668
pub async fn prepare_skill_installation(...) -> Result<SecurityReport, String> {
    let manager = state.skill_manager.lock().await;
    manager.prepare_skill_installation(&skill_id, &locale, ...).await   // ← 无 spawn_blocking
        .map_err(|e| e.to_string())
}
```

而 `skill_manager.rs:803` 里的 `self.scanner.scan_directory_with_options(...)` 是**同步函数**，直接跑在这个 async fn 里，会占住一个 tokio worker 直到整个技能目录扫完（含最多 `max_files` 个文件、每文件最多 2 MiB 的正则匹配）。

同一文件的 `confirm_skill_installation`（`mod.rs:670-694`）已经正确用了 `spawn_blocking`，`install_skill`（`mod.rs:635-650`，内部委托 prepare + confirm）则同样未包。

**对照结论**：三个安装相关命令中，只有 `confirm` 是对的。

### 建议

按 `commands/security.rs:25-30` 的既有写法统一：

```rust
let skill_manager = Arc::clone(&state.skill_manager);
tokio::task::spawn_blocking(move || {
    let manager = skill_manager.blocking_lock();
    // ...
}).await.map_err(|e| format!("[TASK_JOIN_ERROR] {e}"))?
```

注意 `prepare_skill_installation` 本身是 `async`（内含网络下载），需要拆成「async 下载」+「blocking 扫描」两段，不能整体塞进 `spawn_blocking`。

> **交叉印证**：本轮独有。ZCode 报告的 M4/M5 也属性能类，但指向 scanner 内部并行度与冗余 IO，与本条不同。

---

## C5 🟠 cross-skill 上下文在并行扫描后串行重读全部技能目录

**位置**：`src-tauri/src/commands/security.rs:135-150`。

```rust
let cross_skill_contexts: Vec<SkillScanContext> = installed_skills
    .iter()                                     // ← 串行，非 par_iter
    .filter_map(|skill| {
        ...
        cross_skill::build_scan_context_from_skill_dir(...)   // ← 重新读盘
    })
    .collect();
```

`:60-132` 的 `par_iter` 已经把每个技能目录完整读过一遍，这里为了构建协同分析上下文又串行重读一遍。全量扫描路径上的磁盘 IO 因此接近翻倍，且第二遍不享受 rayon 线程池。

**建议**：在第一趟 `par_iter` 的 `filter_map` 里顺带产出 `SkillScanContext`（内容已在内存中），或至少把这一趟也改成 `par_iter`。

> **交叉印证**：与 ZCode 报告 M5（`skill_context.rs` / `scanner.rs` / `pipeline.rs` 三处冗余 IO）属同一类问题，但站点不同，可合并规划。

---

## C6 🟡 `opener:allow-open-path` 授权范围过宽

**位置**：`src-tauri/capabilities/default.json:17-20`。

```json
{
  "identifier": "opener:allow-open-path",
  "allow": [{ "path": "**" }]
}
```

`**` 意味着渲染层可以请求系统打开**任意路径**，包括可执行文件——在 Tauri 的能力模型里，这基本等价于「渲染层被攻破即任意程序启动」。

**当前不可达**，因为：

- CSP 写得克制：`script-src 'self'`，无 `unsafe-eval`（`tauri.conf.json`）；
- 前端全量 grep 无 `dangerouslySetInnerHTML` / `innerHTML` / `eval(`；
- `withGlobalTauri: false`。

但对一个**以安全扫描为卖点**的产品而言，把 scope 收窄到技能目录与缓存目录是低成本的纵深防御，也与 `open_skill_directory` 后端已有的白名单形成一致的双层防护。

> **交叉印证**：ZCode+GLM-5.2 报告记为 H3（其评级为 P0，因其同时发现了 `ToolIcons.tsx` 中绕过后端 allowlist 直接调用 `openPath` 的前端 fallback 路径——两条合起来确实构成可达链路，建议按 ZCode 的评级处理）。

---

## C7 🟡 自动应答 Claude Code 的 workspace trust 提示

**位置**：`src-tauri/src/services/claude_cli.rs:240-251`、`:291-296`。

```rust
fn is_workspace_trust_prompt(output: &str) -> bool {
    let text = output.to_lowercase();
    text.contains("quick safety check")
        || (text.contains("trust this folder") && text.contains("enter to confirm"))
        || (text.contains("accessing workspace") && text.contains("trust"))
}
// 命中后：send_enter(writer)，最多 3 次，间隔 400ms
```

这是可以理解的 UX 取舍——PTY 驱动非交互子命令时，卡在信任提示上会让插件安装直接超时失败。相邻的 `is_unsupported_interactive_prompt`（`:298-306`）对 sudo / password / administrator 类提示做了拦截并主动放弃，方向是正确的。

但需要指出：**一个以「安全扫描」为核心卖点的产品，替用户回答了另一个安全工具的安全确认**。这在语义上值得单独说明——建议在 README 的插件安装章节或首次使用时明示该行为，让用户知情。

> **交叉印证**：本轮独有。

---

## C8 🟡 全局 `Mutex<Connection>` 的队头阻塞

**位置**：`src-tauri/src/services/database.rs:129-147`。

这一条**不是缺陷，是已知取舍**——代码里的类型文档把来龙去脉写得比多数库还清楚：

> 必须使用独占 `Mutex` 而非 `RwLock`：`rusqlite::Connection` 内部是 `RefCell<InnerConnection>`，且**只读查询也会 `borrow_mut()`**……用 `RwLock` 让多个读者并发持有 `&Connection` 会造成 `RefCell` 数据竞争：轻则 panic 导致 IPC 请求永不返回，重则借用计数错乱产生真正的 UB。
>
> 若将来确需并行查询，正确做法是使用多个独立连接 / 连接池 + WAL，而不是共享同一个 `Connection`。

上一轮 R1（UB）已经通过换成独占 `Mutex` 彻底解决，`lock_conn` 还处理了锁中毒后的事务回滚。当前唯一的成本是全局串行化：全量扫描时 `par_iter` 里每个 skill 都要 `db.save_skill`，N 个 rayon 线程在同一把锁上排队。

**演进方向**（非当前必须）：按注释所指，引入 `r2d2_sqlite` 之类的连接池 + 已启用的 WAL，读路径即可真并发。记录在此以免这段设计意图随时间流失。

---

## P1 🟠 工程流程：没有 PR/push CI

**位置**：`.github/workflows/release.yml` 是仓库唯一的 workflow。

```yaml
name: Release
on:
  push:
    tags:
      - ...
```

**这是本轮发现的最大流程缺口**。仓库里已经有一套相当可观的自动化验证资产：

- 515 个 Rust 测试（含 `tests/scan_test_skills.rs`、`tests/rule_matrix.rs` 两套集成测试 + `test/test-skills/` 下的正负样本语料）
- 89 个前端测试
- `pnpm typecheck`、`pnpm format:check` 均已配置

**但它们只在开发者本地手动运行时才会执行**。发布流程只在打 tag 时触发，且不跑测试——一次未经本地验证的合并可以一路走到 release。

### 建议

新增 `.github/workflows/ci.yml`，`on: [push, pull_request]`，跑：

```yaml
- pnpm install --frozen-lockfile
- pnpm typecheck
- pnpm format:check
- pnpm test:unit
- cargo fmt --check
- cargo clippy --all-targets -- -D warnings   # 需先清理现存 58 条 warning
- cargo test
```

投入产出比在本轮所有建议中最高。

---

## 工程与可维护性

### clippy warning

`cargo clippy --all-targets` exit 0，但产生 **58 条 warning**，全部为 style 类，无一涉及正确性：

| 类型 | 数量级 | 示例位置 |
|---|---|---|
| `map_or` 可简化为 `is_some_and` / `is_none_or` | 最多（12+） | `scanner.rs:2861, 2890, 2922, 2958, 3208...` |
| 多余借用（`&x` 而 x 已实现所需 trait） | 6+ | `rules/loader.rs:315, 336, 343, 360` |
| `&PathBuf` 应为 `&Path` | 2 | `skill_manager.rs:2471`、`tests/scan_test_skills.rs:42` |
| 可折叠的嵌套 `if let` | 1 | `lib.rs:335` |

clippy 提示其中 40 条可由 `cargo clippy --fix --lib` 自动修复。清理后即可在 CI 加 `-D warnings` 守住这条线。

### 超大文件

| 文件 | 行数 |
|---|---|
| `security/scanner.rs` | 4521 |
| `services/skill_manager.rs` | 4398 |
| `services/local_cli_scanner.rs` | 3633 |
| `services/plugin_manager.rs` | 2794 |
| `components/InstalledSkillsPage.tsx` | 2606（28 个 `useState`、10 个 `useEffect`） |

均已到达「改一处需通读全文」的规模。`scanner.rs` 的测试模块从 2050 行开始，意味着生产逻辑本身也有 2000 行——是拆分的首要候选（例如把抑制规则 `should_suppress_match` / `package_install_is_inert_hint` 那一组启发式独立成 `suppression.rs`）。

前端 `InstalledSkillsPage.tsx` 同时承载 skill / plugin / marketplace 三种实体的列表、筛选、批量操作与多个对话框，按实体拆分子组件的收益很直接。

### 测试配比

| 侧 | 代码量 | 测试数 | 密度 |
|---|---|---|---|
| Rust | ~36.5K 行 | 515 | 高 |
| 前端 | ~16.5K 行 | 89 | **偏低** |

前端多个测试文件仅 1 个用例（`useSkills.test.ts`、`api.test.ts`、`security-utils.test.ts`、`agent-tools.test.ts`、`version-consistency.test.ts` 等）。已装 `msw` 与 `@testing-library/user-event` 但未充分利用。

建议优先补的交互：**安装确认弹窗的阻断分支**（`SECURITY_CHECK_BLOCKED` / `SECURITY_PARTIAL_SCAN_BLOCKED` 两条路径）、**卸载路径选择对话框**（与 C1 直接相关）、**批量同步工具链接的部分失败态**（`LINK_CREATION_ALL_FAILED`）。

### 仓库卫生

- `reference/cc-switch-main/`（另一个完整项目的副本）已在 `.gitignore` 中且**未被 track** ✅
- `.DS_Store` 与 `src-tauri/.DS_Store` **已被 track**，建议 `git rm --cached` 并确认 `.gitignore` 生效

---

## 值得肯定的部分

以下几处明显高于同类项目平均水平，记录下来以免在后续重构中被无意破坏：

1. **数据库损坏恢复的判定收得极紧**（`lib.rs:332-417`）
   只有 `SQLITE_CORRUPT` / `SQLITE_NOTADB` 才触发「备份 + 重建」；`BUSY` / `PERM` / `FULL` / `CANTOPEN` / `READONLY` 一律保留原库并如实报错。还配了 `only_corruption_codes_trigger_rebuild` 与 `corruption_is_detected_through_context_chain` 两个针对性测试（后者验证 `anyhow::Context` 包装后仍能识别底层错误码）。很多项目在这里会「一律重建」然后静默清空用户数据。

2. **TOCTOU 防护**
   `prepare` 阶段扫描后，`confirm` 阶段会**重新扫描**（`skill_manager.rs:1225` → `rescan_skill_directory_for_confirmation`）再复制文件，而非信任 prepare 的结论。

3. **符号链接一律硬阻止**（`scanner.rs:1511-1539`）
   `WalkDir::follow_links(false)` 之后，扫到 symlink 直接 `blocked = true` + `CWE-59` finding，而不是尝试解析。这堵死了「链接指向 skill 目录外」的整类绕过。

4. **规则包编译期内嵌**（`rules/loader.rs:88-92`、`policy.rs:404`）
   `include_str!` 打进二进制，运行时不可篡改。正则设了 `size_limit(10MB)`，且 Rust `regex` 无回溯，天然免疫 ReDoS——`compile_rule_regex` 里那句注释把这层意图写清楚了。

5. **子进程调用零注入面**
   `local_cli_updater.rs`、`local_cli.rs`、`claude_cli.rs` 全部使用参数数组，无一处字符串拼 shell。Windows 下对 `.cmd` / `.bat` 的 `cmd.exe /d /c` 包装（`claude_cli.rs:164-189`）处理得当，并配有单测。

6. **`open_skill_directory` 的路径白名单**（`commands/mod.rs:998-1045`）
   canonicalize + 多源前缀比对，防护完整——正因如此，C1 的缺失才格外突兀。

---

## 与同期两份评审的关系

同一基线（`727457f`）下另有两份独立评审：

- [`codex-gpt-5-deep-review-2026-08-10.md`](./codex-gpt-5-deep-review-2026-08-10.md)（Codex + GPT-5，5 条发现）
- [`zcode-glm52-code-review-2026-08-10.md`](./zcode-glm52-code-review-2026-08-10.md)（ZCode + GLM-5.2，18 条发现）

三方交叉情况：

| 发现 | Claude Code + Opus 5 | Codex + GPT-5 | ZCode + GLM-5.2 |
|---|---|---|---|
| 单路径卸载可删任意路径 | C1 | R3 | H1 |
| ZIP 无解压后资源预算 | C3 | R5 | — |
| `opener` 授权 `**` 过宽 | C6 | — | H3 |
| **cross-skill 分数不落库** | **C2（独有）** | — | — |
| **安装链路 R2 残留** | **C4（独有）** | — | — |
| **无 PR/push CI** | **P1（独有）** | — | — |
| **自动应答 trust 提示** | **C7（独有）** | — | — |
| 仓库自动更新扫描旧 commit | — | R1 | — |
| 更新覆盖失败保留旧文件 | — | R2 | — |
| 前端 fallback 绕过后端 allowlist | — | — | H2 |
| GitHub URL 无 host 校验 | — | — | H4 |
| scanner 内部无并行 / 冗余 IO | C5（相关站点） | — | M4 / M5 |

**被三方独立命中的只有一条：`uninstall_skill_path`。** 建议以此为第一优先级。三份报告的并集共约 25 条独立发现，去重后可直接作为下一轮的修复 backlog。
