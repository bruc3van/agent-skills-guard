# 卡顿与闪退 深度 Review（合并版）

- **日期**：2026-08-07
- **版本**：v1.3.4（`2b83ae8`，工作区干净）
- **范围**：`src/`（React 前端）+ `src-tauri/src/`（Rust 后端），约 50k 行 / 135 个源文件
- **性质**：仅诊断，本轮未修改任何代码
- **来源**：两份独立 review 的合并结果，交叉验证后去重、纠偏、补全

---

## 结论摘要

定位到 **2 个高风险根因** + **5 个放大因素**，可以完整解释「卡顿」与「偶发闪退」两个症状。

其中 **R1（SQLite 连接并发不安全）单独一条即可同时解释两个症状**：竞态检测命中时 panic → tokio 吞掉 task → `invoke` 的 Promise 永不 settle → 界面永久转圈（表现为"卡死"）；竞态未被检测到时产生真实数据竞争 → 内存破坏 → 进程退出（表现为"闪退"）。

| 编号 | 问题 | 等级 | 主要症状 | 位置 |
|---|---|---|---|---|
| R1 | SQLite `Connection` 被并发读访问（UB） | 🔴 P0 | 闪退 + 卡死 | `database.rs:129-193` |
| R2 | 同步重活跑在 async 命令 / tokio 工作线程上 | 🔴 P0 | 卡顿 | `security.rs`、`plugins.rs`、`skill_manager.rs` |
| R3 | `get_installed_skills` 每次调用做全盘磁盘 reconcile | 🟠 P1 | 切页卡顿 | `skill_manager.rs:1679-1718` |
| R4 | 冷启动任务风暴 | 🟠 P1 | 启动数秒无响应 | `App.tsx`、`app-update-refresh.ts`、`LocalCliPage.tsx` |
| R5 | 启动期 `expect()` panic + 无持久化日志 | 🟠 P1 | 打开即闪退 / 无现场 | `lib.rs:301, 316-326` |
| R6 | 全局 Mutex 队头阻塞 | 🟠 P1 | 卡顿 | `commands/mod.rs`、`commands/plugins.rs` |
| R7 | 列表接口重复搬运完整安全报告 | 🟡 P2 | 主线程压力 | `database.rs:670`、`models/security.rs:159` |

**建议修复顺序**：R1 → R2 → R3/R6 → R4 → R5 → R7

---

## R1 🔴 SQLite 连接的并发读是未定义行为

### 证据

`src-tauri/src/services/database.rs:129-138` 将内部含 `RefCell` 的 `rusqlite::Connection` 强制声明为 `Sync`，随后用允许**多读者并发**的 `RwLock` 保护：

```rust
/// Safety: `Connection` is `Send` but not `Sync` due to internal `RefCell`.
/// We rely on `RwLock` to enforce read/write exclusion at runtime.
struct SyncConnection(Connection);
unsafe impl Sync for SyncConnection {}   // ← 安全性论证不成立
```

注释的前提是错的：`RwLock::read()` **允许多个线程同时持有** `&Connection`。

已在本机 crate 源码确认 rusqlite 的实际结构：

```
~/.cargo/registry/src/index.crates.io-*/rusqlite-0.32.1/src/lib.rs:377
    pub struct Connection { db: RefCell<InnerConnection>, cache: StatementCache, ... }
~/.cargo/registry/src/index.crates.io-*/rusqlite-0.32.1/src/lib.rs:383
    unsafe impl Send for Connection {}      // 只有 Send，没有 Sync
~/.cargo/registry/src/index.crates.io-*/rusqlite-0.32.1/src/lib.rs:782
    pub fn prepare_with_flags(&self, ...) { self.db.borrow_mut().prepare(...) }
                                                     ^^^^^^^^^^^ 只读查询也会 borrow_mut
```

**关键点：每一次"只读"查询都会 `borrow_mut()`。** `query_row()` 内部同样先调 `prepare()`（`lib.rs:701`）。

`lock_conn_read()`（`database.rs:188-193`）被 9 个方法使用，全部紧跟 `prepare` / `query_row`：

| 方法 | 行号 |
|---|---|
| `get_repositories` | `:494` |
| `repository_url_exists` | `:531` |
| `is_app_migration_completed` | `:650` |
| `get_skills` | `:670` |
| `get_plugins` | `:740` |
| `get_local_cli_tool` | `:1238` |
| `get_all_local_cli_tools` | `:1266` |
| `get_repository` | `:1294` |
| `get_unscanned_repositories` | `:1333` |

### 并发确实可达

Tauri v2 的 `#[tauri::command] async fn` 各自 spawn 到多线程 tokio runtime。冷启动瞬间 React Query 并发打出：

- `get_installed_skills` → `db.get_skills()`
- `get_repositories`
- `get_plugins_cached` → `db.get_plugins()`
- `get_scan_results` → `db.get_skills()`
- `list_local_cli_tools` → `db.get_all_local_cli_tools()`

全部落在不同 worker 线程上，同时对同一个 `RefCell` 执行 `borrow_mut()`。

> 注：rusqlite 的 `bundled` feature 使用 `SQLITE_THREADSAFE=1`（serialized），SQLite **C 层句柄**是有内部互斥保护的。本问题出在 **Rust 层的 `RefCell` / `InnerConnection` / `StatementCache`**，C 层的线程安全不覆盖这里。

### 两种后果，对应两个症状

1. **卡死**：`RefCell` 的借用检查在 release 构建下**同样生效**（不是 debug-only）。竞态被检测到时 panic `already mutably borrowed` → tokio 在 task 边界吞掉 panic → 该 `invoke` 的 Promise **永不 settle** → 前端永久 loading。
2. **闪退**：`RefCell` 的借用标志是 `Cell<isize>`，**非原子递增**。竞态下递增可能丢失，两个线程都拿到 `RefMut` → 两个 `&mut InnerConnection` → 真实数据竞争 → 内存破坏 / 访问违例。

### 修复（最小改动，无需改任何调用方）

```rust
// database.rs
use std::sync::Mutex;

pub struct Database {
    conn: Mutex<Connection>,   // 删除 SyncConnection 与 unsafe impl Sync
}

impl Database {
    fn lock_conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|poisoned| {
            log::error!("Database mutex poisoned, rolling back");
            let guard = poisoned.into_inner();
            let _ = guard.execute_batch("ROLLBACK");
            guard
        })
    }
    // 保留旧名，避免改动 9 个调用点
    fn lock_conn_read(&self) -> std::sync::MutexGuard<'_, Connection> { self.lock_conn() }
}
```

`Connection` 本身是 `Send`，`Mutex<Connection>` 自动获得 `Sync`，**`unsafe` 可以完全删除**。

串行化带来的吞吐损失由 R3 / R7 的优化抵消有余（当前一次 `get_installed_skills` 就发起 3 次全表读）。

> 若后续确实需要并行查询，正确做法是**多个独立 Connection 或连接池**（如 `r2d2_sqlite`）+ 开启 WAL，而不是共享单个 Connection。当前 schema 也未设置 `PRAGMA journal_mode=WAL` 与 `busy_timeout`。

---

## R2 🔴 同步重活跑在 async 命令上，饿死 tokio 运行时

### 全量扫描：在 async 里同步等待 Rayon

`src-tauri/src/commands/security.rs:15-108`：

```rust
#[tauri::command]
pub async fn scan_all_installed_skills(...) -> Result<Vec<SkillScanResult>, String> {
    let pool = ThreadPoolBuilder::new().num_threads(parallelism).build()?;  // 每次调用新建线程池
    let mut results = pool.install(|| { ... par_iter() ... });              // 阻塞当前 tokio worker
```

`pool.install()` 会**阻塞调用线程直到整个扫描结束**。同时每次调用都新建再销毁一个 Rayon 线程池，存在线程 churn。

### 单项扫描：声明 async，实为同步

- `security.rs:174` `scan_installed_skill` — 直接同步执行目录遍历 + 文件读取 + 正则分析
- `plugins.rs:318` `scan_installed_plugin` — 同上
- `security.rs:274` `count_scan_files` — 同上

### 前端并发放大

`src/components/OverviewPage.tsx:123` 读取 `getScanConcurrency()`，`src/lib/storage.ts:5-6` 定义：

```ts
const DEFAULT_SCAN_CONCURRENCY = 3;
const MAX_SCAN_CONCURRENCY = 8;
```

即前端最高可同时发起 **8 个** `scan_installed_skill`。每项上限 2000 文件 / 单文件 2 MiB（`security/policy.rs:242-250`）。

**嵌套并发**：前端 8 路 × 后端 Rayon 池（`commands/mod.rs:28-30`，`DEFAULT_SCAN_PARALLELISM=3`、`MAX_SCAN_PARALLELISM=8`）→ 无统一预算。

### 后果

- tokio worker 被同步任务长期占用，其他 IPC 请求排队；
- 多路磁盘读取 + 复杂正则争抢 CPU；
- 扫描期间页面操作、查询、关闭窗口均明显变慢。

> **纠偏**：另一份 review 提到"极端输入下内存或 CPU 压力可能进一步诱发进程退出"。此推测**证据不足，应降级**——扫描器有明确硬上限（2000 文件 / 单文件 2 MiB / 深度 20，见 `security/policy.rs:242-250`），OOM 路径不成立。R2 的实际影响是**卡顿**，不是闪退。

### 正确示范就在同一份代码里

`src-tauri/src/commands/mod.rs:629` 的 `confirm_skill_installation` 已经正确使用了 `spawn_blocking`：

```rust
tokio::task::spawn_blocking(move || {
    let manager = skill_manager.blocking_lock();
    manager.confirm_skill_installation(...)
}).await
```

### 建议

1. 上述所有扫描命令统一包进 `tokio::task::spawn_blocking`；
2. Rayon 线程池提为全局 `LazyLock`，不再每次新建；
3. 建立**后端统一扫描调度器 + 全局并发预算**，前端不再自行并发，避免嵌套；
4. 扫描任务支持取消。

---

## R3 🟠 `get_installed_skills` 每次调用做全盘磁盘 reconcile

这是「切到已安装页面就卡」的直接原因。

### 证据

`src-tauri/src/services/skill_manager.rs:1679-1718`，一次「读取已安装列表」实际执行：

| 步骤 | 行为 | 行号 |
|---|---|---|
| 1 | `cleanup_stale_prepare_paths_once()` | `:1684` |
| 2 | `refresh_installed_tool_links_from_dirs()` → **全表 `db.get_skills()`** + 逐 skill 逐工具目录符号链接比对 | `:1691` → `:1733` |
| 3 | `adopt_existing_skills_for_scan()` → 遍历所有工具 skill 目录 + **第二次全表 `get_skills()`** | `:1693` → `migration.rs:74-75` |
| 4 | **第三次全表 `db.get_skills()`** | `:1706` |
| 5 | 逐 skill **重复执行第 2 步已做过的** `refresh_existing_tool_links_for_skill` | `:1710` |

而 `refresh_existing_tool_links_for_skill`（`:294-354`）对每个候选路径调用 `tool_skill_path_is_compatible_with_source`（`:179-197`），后者会**读取 SKILL.md 并计算 SHA256**，还夹杂 `canonicalize()` 系统调用。

### 量级

设 N 个技能、T 个工具目录，**单次调用**约等于：

- 3× 全表读（每条含 `security_report` JSON 反序列化）
- 2×N×T 次文件读取 + SHA256

100 技能 / 5 工具目录 ≈ **1000+ 次文件哈希**。

### 调用频率极高

`src/hooks/useSkills.ts:13-21`：

```ts
export function useInstalledSkills() {
  return useQuery({
    queryKey: ["skills", "installed"],
    queryFn: () => api.getInstalledSkills(),
    staleTime: 0,
    refetchOnMount: "always",   // ← 每次挂载都重跑上面全部工作
    refetchOnWindowFocus: false,
  });
}
```

`OverviewPage` 与 `InstalledSkillsPage` 都订阅此 key，**来回切标签页 = 反复全盘扫描**。

### 建议

1. 拆分职责：纯读 `get_installed_skills()`（只查 DB）vs 显式 `refresh_skill_links()`（reconcile）；前端只在启动与手动刷新时调后者；
2. 消除 `:1691` 与 `:1710` 的重复 reconcile，三次 `get_skills()` 合并为一次；
3. `staleTime` 调整到 30s 量级，`refetchOnMount` 恢复默认；
4. 为 `skill_md_checksum` 增加 `(path, mtime, size) → checksum` 内存缓存。

---

## R4 🟠 冷启动任务风暴

启动后数秒内叠加执行：

| 时机 | 任务 | 位置 |
|---|---|---|
| 立即 | `reconcileSkillStateOnAppStartup` → `refreshSkillLinks()` + `scanLocalSkills()` + 4 个 query refetch | `App.tsx:106` → `app-update-refresh.ts:42-62` |
| +1.5s | 刷新精选仓库 / 精选 marketplace + 同步插件 | `App.tsx:190` |
| +2.5s | `autoScanUnscannedRepositories()` | `App.tsx:207` |
| +4s | 自动检查应用更新 | `UpdateContext.tsx:206` |
| 立即 | `LocalCliPage` **即使从未打开也永久挂载** | `App.tsx:474-482` |

### `reconcileSkillStateOnAppStartup` 无条件全量扫描

`src/lib/app-update-refresh.ts:73` 的注释明确写着「此处不再按版本号跳过扫描」，即**每次冷启动都全量扫**，且它触发的是 R3 中那条最重的路径。

### `LocalCliPage` 常驻挂载的代价

用户还停留在总览页时，它已经：

1. 触发 `list_local_cli_tools` → `discover_local_cli_tools()`（`local_cli_scanner.rs:991-999`）**串行** spawn `npm root -g`、`npm ls -g --json`、`npm bin -g`、`pnpm root -g`、`pnpm ls -g`、`pip show pip`、`brew`、`scoop`、`choco`……每个超时 **15 秒**（`local_cli_scanner.rs:472`）；
2. 结果返回后 `LocalCliPage.tsx:73-125` 的 effect **逐个工具串行**调 `fetchLocalCliDescriptions`，每次 1 个 IPC + 1 个子进程（5s 超时，`commands/local_cli.rs:1001`），且每轮 2 次 `setState` → **784 行的整页组件全量重渲染 2N 次**；
3. `merge_and_cache_tools`（`commands/local_cli.rs:609-637`）在 `spawn_blocking` **之外**同步执行 N 次 DB 读 + N 次 DB 写，直接抢 R1 那把锁。

> 正面确认：`configure_background_command`（`local_cli_scanner.rs:478-481`）已正确设置 `CREATE_NO_WINDOW`，Windows 下不会闪黑框。

### 建议

1. `LocalCliPage` 改为首次进入时才加载；描述查询仅在页面可见时执行；
2. 描述抓取批量化（一次 IPC 传全部路径），后端并发处理，前端进度更新加节流；
3. 恢复 `reconcileSkillStateOnAppStartup` 的版本 / 时间戳节流；
4. 启动任务串行分级，合并重复扫描，并支持取消。

---

## R5 🟠 启动期 `expect()` panic + 无持久化诊断

### 证据

`src-tauri/src/lib.rs:316-326`：

```rust
let app_dir = app.path().app_data_dir().expect("Failed to get app data directory");
std::fs::create_dir_all(&app_dir).expect("Failed to create app data directory");
let db_path = app_dir.join("agent-skills.db");
let db = Database::new(db_path).expect("Failed to initialize database");
```

数据库损坏、文件被锁定、权限异常、磁盘错误 → 进程**立即退出**，完全符合"打开即闪退"。

### 无现场证据

`lib.rs:301` 仅初始化了控制台 `env_logger`：

```rust
env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
```

**没有滚动文件日志，没有 panic hook。** 发布版闪退后几乎不留任何可分析的现场。

### 建议

1. 启动失败改为返回可恢复的错误页，而非 panic；
2. 对损坏数据库提供**自动备份 + 重建**路径；
3. 增加滚动文件日志（如 `tauri-plugin-log`）与 `std::panic::set_hook`，将 panic 落盘。

---

## R6 🟠 全局 Mutex 队头阻塞

`src-tauri/src/commands/mod.rs:569-579` 等处，所有 skill 命令先抢同一把锁：

```rust
pub async fn get_skills(state: State<'_, AppState>) -> Result<Vec<Skill>, String> {
    let manager = state.skill_manager.lock().await;   // 全局串行点
    manager.get_all_skills().map_err(|e| e.to_string())
}
```

plugin 命令同理抢 `state.plugin_manager`（`commands/plugins.rs:154`、`:168`、`:330`）。

启动期 `reconcileSkillStateOnAppStartup` 的 `refreshSkillLinks()` + `scanLocalSkills()` **持锁数秒**（R3 路径），期间 UI 发出的每一个 `get_skills` / `get_installed_skills` 全部排队 —— 表现为「启动后前几秒点什么都没反应」。

**建议**：R2 引入 `spawn_blocking` 后，进一步缩小临界区 —— 让 Manager 只保护真正需要互斥的可变状态，只读路径直接走 DB。

---

## R7 🟡 列表接口重复搬运完整安全报告

### 证据

`src-tauri/src/models/security.rs:151-167`：

```rust
pub struct SecurityReport {
    ...
    pub scanned_files: Vec<String>,   // 上限 2000 条
    pub skipped_files: Vec<String>,
}
```

`database.rs:670` `get_skills()` 每次都读取并反序列化完整 `security_report`；`commands/security.rs:296` `get_scan_results()` 又通过 `get_skills()` 重复读取一遍。

本机数据库实测：

```
C:\Users\Bruce\AppData\Roaming\com.agent-skills-guard.app\agent-skills.db
Length : 14,237,696 字节（约 13.6 MiB）
```

### 补充发现：`scanned_files` 前端从未使用

对 `src/` 全量检索确认：**`scanned_files` 没有任何读取点**。前端只用到 `skipped_files` 的 count 与前 3 条预览：

- `SecurityDetailDialog.tsx:74-75` — `skipped_files.length` / `.slice(0, 3)`
- `ui/SkillSecurityDialog.tsx:48` — `skipped_files`
- `InstalledSkillsPage.tsx:1739`、`MarketplacePage.tsx:649`、`RepositoriesPage.tsx:1119` — 均只读 `skipped_files`

即 2000 条/技能的 `scanned_files` 是**纯粹的载荷浪费**：SQLite 读取 → JSON 反序列化 → IPC 序列化 → 前端 `JSON.parse`，全程压在主线程上，而结果无人使用。

### 建议

1. `scanned_files` 加 `#[serde(skip_serializing)]`，或不再入库；
2. 列表接口只返回摘要字段（score / level / issue 计数），完整报告在打开详情时按 ID 单独加载。

---

## P3 次要项

| 位置 | 问题 |
|---|---|
| `commands/security.rs:222`、`commands/plugins.rs:370` | `emit("scan-progress")` **每扫描一个文件发一次**，但前端全局检索**无任何 `listen()` 订阅** —— 纯浪费的序列化 + IPC |
| `src-tauri/tauri.conf.json` | Windows 下 `"transparent": true` + `"decorations": false` 会让 WebView2 走非硬件加速合成路径，建议实测对比开关差异 |
| `src/contexts/UpdateContext.tsx:217` | `value` 对象每次渲染重建，所有消费者跟着重渲染 —— 应 `useMemo` |
| `src/main.tsx` | 无 `window.onerror` / `unhandledrejection` 全局兜底，React 树外的异步异常静默丢失 |
| `src/components/ErrorBoundary.tsx` | 无 `key` reset 机制，切换页面后错误状态不会自动清除 |
| `src/components/InstalledSkillsPage.tsx:1616` | `renderSkillCard` 等在组件体内定义，列表无虚拟化；条目多时每次 `setState` 全量重渲染 |
| `database.rs` | 未设置 `PRAGMA journal_mode=WAL` 与 `busy_timeout`（当前单连接影响有限，改连接池时必须补上）|

---

## 现场取证与验证基线

### 崩溃日志（两份 review 独立取证，结论一致）

```
近 30 天 Windows Application Error / Application Hang / .NET Runtime 事件：401 条
  其中匹配 agent-skills / WebView2 的：0 条
WebView2 Crashpad 目录（%LOCALAPPDATA%\Microsoft\Edge WebView2\Crashpad\reports）：不存在
```

**含义**：本机暂无法把某一次真实闪退**精确归因**到上述任一条路径。这不推翻 R1 的分析（该问题由静态代码 + crate 源码论证得出，不依赖崩溃日志），但说明**R5 建议的持久化日志与 panic hook 应尽快落地**，否则后续闪退仍将无据可查。

### 测试基线（本轮实跑，全部通过）

| 项目 | 结果 |
|---|---|
| TypeScript `tsc --noEmit` | ✅ 通过，无错误 |
| 前端单测 `vitest run` | ✅ 25 个文件 / 76 项全部通过（8.28s）|
| Rust 库测试 `cargo test --lib` | ✅ 457 项通过 / 0 失败（2.87s）|

**重要**：现有测试**不覆盖**以下场景，因此测试全绿**不能推翻**上述发现：

- 共享 SQLite `Connection` 的真实多线程并发（R1）
- tokio 工作线程饥饿 / 运行时阻塞（R2）
- 损坏数据库的启动恢复（R5）
- 大数据量下的 IPC 载荷与主线程压力（R7）

建议在修复 R1 时补一个**并发读压测**（多线程同时调用 `get_skills` / `get_repositories` / `get_plugins`），作为回归防线。

---

## 两份 Review 的差异说明

| 项 | 说明 |
|---|---|
| R1、R2、R4、R7 | 两份 review 独立得出，结论一致，已合并并补充 crate 源码级证据 |
| R3、R6 | 仅第一份发现（`get_installed_skills` 三次全表读 + N×T 次 SHA256；全局 Mutex 队头阻塞）—— 这是**切页卡顿的最大单一来源** |
| R5 | 仅第二份发现（启动 `expect()` panic + 无持久化日志）—— 对**定位闪退现场**价值很高 |
| `scan-progress` 无监听 | 仅第一份发现 |
| `scanned_files` 前端零引用 | 仅第一份验证到具体证据，第二份只笼统建议"返回摘要" |
| 崩溃日志取证 | 仅第二份执行，本轮已复现验证 |
| ⚠️ 已纠偏 | 第二份称扫描"极端输入下可能诱发进程退出"——证据不足，扫描器有硬上限（2000 文件 / 2 MiB / 深度 20），OOM 路径不成立，已降级为纯卡顿问题 |

---

## 实施状态（2026-08-07 已完成，未提交）

三个阶段均已实现。全量验证：`tsc --noEmit` ✅ / `vitest` 25 文件 76 项 ✅ / `cargo test --lib` **464** 项 ✅（新增 7 项）/ `vite build` ✅。

| 编号 | 实施内容 | 主要改动 |
|---|---|---|
| R1 | `RwLock<SyncConnection>` → `Mutex<Connection>`，**删除 `unsafe impl Sync`**；补 `PRAGMA journal_mode=WAL` + `busy_timeout=5000` | `database.rs` |
| R1 | 新增 2 项并发回归测试：8 线程 × 50 轮并发只读、4 读者 + 1 写者 | `database.rs` |
| R5 | 新增 `logging` 模块：stderr + 滚动文件（2 MiB，保留一代）双写、`panic` 钩子落盘 backtrace；启动 `expect()` 全部改为可恢复错误 + 原生错误对话框；数据库损坏时自动备份为 `.corrupt-<时间戳>` 并重建 | `logging.rs`、`lib.rs` |
| R3 | 新增 `SKILL_MD_CHECKSUM_CACHE`（按 `(mtime, size)` 失效），消除 O(N×T) 次 SKILL.md 读取 + SHA256；安装/更新/卸载路径显式失效 | `skill_manager.rs` |
| R3 | `useInstalledSkills` 由 `staleTime: 0` + `refetchOnMount: "always"` 改为 30s staleTime | `useSkills.ts` |
| R2 | 全部扫描命令移入 `spawn_blocking`；Rayon 线程池按并行度缓存复用（最多 8 个，各创建一次） | `security.rs`、`plugins.rs`、`commands/mod.rs` |
| R6 | 新增 `with_skill_manager_blocking` helper，`get_skills` / `get_installed_skills` / `scan_local_skills` / `refresh_skill_links` / `uninstall_skill(_path)` 全部移出 async 线程 | `commands/mod.rs` |
| R4 | `LocalCliPage` 改为首次进入才挂载（之后常驻保状态）；描述抓取由「逐个串行 IPC + 2N 次全页重渲染」改为一次批量 IPC + 后端并发（信号量限 4）+ 单次 setState | `App.tsx`、`LocalCliPage.tsx`、`local_cli.rs` |
| R7 | 新增 `SecurityReport::without_scanned_file_list()`，在全部 7 处持久化/出站边界剥离 `scanned_files` | `models/security.rs` 等 |
| P3 | 删除 `ScanProgressEvent` 与 `scan-progress` 发射（前端零监听）及连带的 `scan_id` 参数；新增全局 `error` / `unhandledrejection` 兜底；`UpdateContext` 的 `value` 加 `useMemo` | 多处 |

### 两处有意偏离原计划

**1. R3「拆分读/reconcile」未按原样实施 —— 测试推翻了原判断。**

原计划把 `get_installed_skills` 改为纯 DB 读。实施后 `installed_skills_reuse_existing_record_when_source_is_missing` 失败（期望 1 条记录，实得 2 条）。原因：`get_installed_skills_from_dirs` 里那次「看似冗余」的前置 `refresh_installed_tool_links_from_dirs` 并不冗余 —— 它先把 `source_path` / `local_paths` 重新指向磁盘上真实存在的副本，adoption 才能把发现的磁盘分组匹配到既有记录，否则会新建重复记录。

已恢复该调用，改为用 checksum 缓存消除其**成本**而非删除其**语义**：第二轮比对现在几乎零 IO。代码中已补注释说明为何不可省略。

**2. R4「恢复启动 reconcile 的版本/时间戳节流」未实施 —— 会回归一个刚修的功能。**

`reconcileSkillStateOnAppStartup` 的注释明确写着「此处不再按版本号跳过扫描」，且提交 `514b589`「添加强制刷新已安装技能工具链接的功能，确保应用内更新后软链图标正确点亮」正是为此。加时间节流会导致「用户在外部创建软链后重启，图标不点亮」。

该路径的成本已由 R6（移出 async 线程，不再阻塞其他 IPC）+ R3（checksum 缓存）显著降低，因此保留每次冷启动执行一次的语义。

### 未处理项

`tauri.conf.json` 的 Windows `transparent: true` 保持原样 —— 这需要在真机上实测开关前后的渲染差异才能判断，不宜盲改。

### 复审轮（两份外部 review 的核实与处理）

第二轮由两个外部 AI 对上述改动做 review，逐条核实后处理如下。验证基线：`tsc` ✅ / `vitest` **26 文件 82 项** ✅ / `cargo test --lib` **472 项** ✅ / `clippy` 0 error / `vite build` ✅。

| 编号 | 问题 | 核实 | 处理 |
|---|---|---|---|
| F1 | `open_database_with_recovery` 对**任意**失败都重建空库，BUSY / 权限 / 磁盘满会导致真实数据丢失 | ✅ 属实，且是本轮最严重的一条 | 新增 `is_database_corrupted()`，只认 `SQLITE_CORRUPT` / `SQLITE_NOTADB`；其余错误保留原库并返回可读错误。补 6 项测试（含 `anyhow::Context` 包装后仍能识别错误码、垃圾文件重建、健康库不备份） |
| F2 | `OverviewPage` 用裸 `useQuery(["skills","installed"])`（默认 `staleTime: 0` + `refetchOnMount`），把 R3 的缓存优化整个抵消 | ✅ 属实 | 改用 `useInstalledSkills()`，统一缓存策略 |
| F3 | 数据库重建成功只写 `log::warn`，用户不知道数据被清空 | ✅ 属实 | 新增 `show_database_rebuilt_dialog()`，非致命警告弹窗 + 备份路径 |
| F4 | 描述抓取在开始时标记 `attempted`，取消后不回滚，这些工具永不重试 | ✅ 属实，但**建议的修法有副作用** | 见下方说明 |
| F5 | `scan_all_installed_plugins` 仍在 async 线程同步 `get_plugins()`，与 skills 路径不对称 | ✅ 属实 | 并入 `spawn_blocking` |
| F6 | sidecar 路径用 `format!("{}", path.display())` 拼接，非 UTF-8 路径有损 | ✅ 属实 | 新增 `sidecar_path()`，改用 `OsString` 拼接 + 测试 |
| F7 | 批量化后进度一直停在 1/N | ✅ 属实 | 进度状态简化为 `{ total }`，文案改为「正在获取 N 个工具的说明」，同步更新 zh/en 两份 i18n |
| F8 | `global-error-handlers` 的 `installed` guard 是死代码，建议删除 | ⚠️ **部分属实** | 见下方说明 |
| F9 | `scanned_files` 老数据仍在库里，需等重新扫描才有收益 | ✅ 属实 | 加 `#[serde(skip_serializing_if)]`（出站 JSON 完全省略该字段）+ 在 `deserialize_security_report` 读取侧剥离，老数据立即受益。补 2 项测试 |
| F10 | 线程池常驻线程上限未在注释中说明 | ✅ 合理 | 注释补充：最坏 1+2+…+8 = **36 个**常驻线程及其取舍理由 |
| — | 「建议补 `toHaveBeenCalledTimes(1)` 断言」 | ❌ **事实有误** | 该断言本就存在（`LocalCliPage.test.tsx`），无需改动 |

#### F4：采纳问题，但没有照搬修法

原建议是「cleanup 时删除本批 attempted 标记」。照做后 `工具列表变化后继续为新工具请求说明` 测试失败 —— 因为 cleanup 在**每次** effect 重跑时都会触发，包括批次已成功完成的情况，于是已抓取过的工具被反复重新请求。

改为引入 `settled` 标志，**只回滚「尚未拿到结论就被中断」的批次**：已完成的保留标记（不重复请求），被中断的撤销标记（可重试）。两种语义各有一条测试锁定。

#### F8：保留 guard，只修注释

原建议「直接删掉 `installed` guard，因为 `addEventListener` 对相同引用幂等」。这个前提不成立：**每次调用都会创建新的闭包**，浏览器无法据此去重，删掉 guard 反而会引入重复注册（同一个错误被记录多次）。

另外其描述的 StrictMode 失效序列也不成立 —— React 的顺序是 setup → cleanup → setup，被拦截的调用拿到的是空 cleanup，不会误解除他人注册的监听器。

确实错的是**注释**：它声称防的是「StrictMode 下 effect 执行两次」，但唯一调用点是 `main.tsx` 模块顶层，本就只执行一次。已改写注释说明真实用途，并补 5 项测试锁定「重复注册被拦截」「被拦截的调用返回空 cleanup」「cleanup 后可重新注册」等行为。

### 需要真机验证的点

- **数据库损坏恢复**：把 `%APPDATA%\com.agent-skills-guard.app\agent-skills.db` 内容替换为任意文本后启动，应弹出「数据库已重建」警告并给出备份路径（已有单测覆盖，仍建议真机确认弹窗表现）
- **数据库被占用**：应用运行时用另一个进程独占该文件再启动，应提示「数据库文件已保留」而**不是**清空数据
- 日志文件生成位置：`%APPDATA%\com.agent-skills-guard.app\logs\agent-skills-guard.log`
- 切换标签页与扫描期间的实际流畅度

---

## 落地路线图

### 第一阶段 —— 止血（消除闪退与卡死）

1. **R1**：`RwLock<SyncConnection>` → `Mutex<Connection>`，删除 `unsafe impl Sync`（约 10 行）
2. **R5**：启动 `expect()` → 可恢复错误页；接入滚动文件日志 + panic hook

### 第二阶段 —— 消除卡顿主因

3. **R3**：拆分 `get_installed_skills` 读/reconcile 职责；消除重复扫描；放宽 `staleTime`
4. **R2**：扫描命令统一 `spawn_blocking`；Rayon 池全局化；建立统一并发预算
5. **R6**：缩小 Manager 临界区

### 第三阶段 —— 体验优化

6. **R4**：`LocalCliPage` 懒加载；描述批量抓取；启动任务分级节流
7. **R7**：剥离 `scanned_files`；列表接口返回摘要
8. **P3**：删除无监听的 `scan-progress`；补全局错误兜底；列表虚拟化

### 回归防线

- 补并发读压测（覆盖 R1）
- 补启动期损坏数据库恢复测试（覆盖 R5）
