# Skills 生态整合实施方案（npx skills 兼容 + 市场自动化）

- **日期**：2026-08-08
- **修订**：rev4（经三轮外部评审；rev1→rev2 见附录 A，rev2→rev3 见附录 B，rev3→rev4 见附录 C）
- **状态**：**M0 已放行**；M1 在本轮 tree SHA 闭环修正后放行；T3 / T8 保持门禁
- **基线版本**：`54a964a`「尝试修复更新后软链状态丢失的问题」（v1.3.6 之后一个未发布提交）；本文档为未跟踪新增文件
- **范围**：`src-tauri/src/services`、`src-tauri/src/security`、`src-tauri/src/commands`、`src/components`、CI
- **性质**：实施方案，本文档不含代码改动
- **调研对象**：[vercel-labs/skills](https://github.com/vercel-labs/skills)（npm 包 `skills` v1.5.22）、[skills.sh](https://www.skills.sh)（含 [API 文档](https://www.skills.sh/docs/api)）

---

## 结论摘要

1. **兼容性（不变）**：我们与 `npx skills` 选中了**同一套磁盘布局**（`~/.agents/skills/<name>` 规范目录 + 软链分发），物理层零冲突。但两边账本互不可见，开发机上已出现实际失联。这是必须先修的。

2. **市场（结论已变）**：skills.sh 已上线正式 `/api/v1`，提供分页目录、搜索、详情、curated 与**多厂商安全审计聚合**，全部标注需 Vercel OIDC。其中 audit 端点**实测匿名返回 200，但官方文档同样标注 `Authentication Required.`**——属实现状态而非契约。因此**不能把任何功能建立在其持续匿名开放的假设上**，skills.sh 的一切数据接入均置于 feature flag 之后。

3. **差异化（必须重新定位）**：「全网唯一有安全数据」已不成立——skills.sh 已聚合 5 家厂商。新定位见 §二。

4. **节奏（已调整）**：新增 **M0 技术验证**；共享 lock 写回从 M2 推迟到 M4 并置于 feature flag 之后；不再「先做 T3」。

| 编号 | 工作项 | 等级 | 里程碑 |
|---|---|---|---|
| T0 | API 契约、授权与 GitHub 配额验证 | 🔴 P0 | M0 |
| T1 | 只读 lock 识别 + 多信号匹配 + `update_provider` | 🔴 P0 | M1 |
| T2 | 整目录扫描指纹 + 未扫描告警 | 🔴 P0 | M1 |
| T7 | 安全扫描器抽为 headless CLI | 🔴 P0 | M2 |
| T8 | CI 索引管线（带内容哈希） | 🔴 P0 | M2 |
| T3 | skills.sh 数据接入（audit + 搜索，**均 flag 默认关闭**） | 🟠 P1 | M3 |
| T9 | 市场页面重构 + 装前扫描闭环 | 🟠 P1 | M3 |
| T10 | `featured-marketplace.yaml` 瘦身 | 🟡 P2 | M3 |
| T4 | 账本写回（feature flag） | 🟠 P1 | M4 |
| T5 | 完整隔离（含 `--copy` 副本） | 🟠 P1 | M4 |
| T6 | Agent 注册表扩展 | 🟡 P2 | M4 |
| T11 | 委托 npx 安装 | 🟡 P2 | M4 |
| T12 | 项目作用域支持 | 🟡 P2 | M4 |

---

## 一、现状盘点

### 1.1 双方架构对照

| 维度 | agent-skills-guard | vercel `skills` CLI | 结论 |
|---|---|---|---|
| 规范目录 | `~/.agents/skills/<name>` | 同左 | ✅ 一致 |
| 分发方式 | 规范目录 → 各工具目录建链 | 同左，`--copy` 可降级 | ✅ 一致 |
| Windows 链接 | NTFS Junction | `symlink(..., 'junction')` | ✅ 一致 |
| POSIX 链接 | 绝对路径 symlink | **相对路径** symlink | ✅ 我方已能解析 |
| 全局账本 | SQLite（app data） | `~/.agents/.skill-lock.json` v3 | ❌ 互不可见 |
| 项目账本 | 无 | `./skills-lock.json` v1（入 git） | ❌ 缺失 |
| Agent 覆盖 | 5 个 | 75+ | ❌ 差距大 |
| 来源类型 | GitHub | GitHub/GitLab/SSH/raw/本地/node_modules/Mintlify/HuggingFace/well-known | ❌ 差距大 |
| 安全信息 | 本地实时扫描，行级 | 聚合 5 家厂商，装后生成 | ⚠️ 见 §二 |
| 运行依赖 | 无 | Node ≥ 22.20.0 | ✅ 我方更轻 |

已验证的兼容点（代码级）：

- [agent_tools.rs:47](../src-tauri/src/services/agent_tools.rs#L47) 的 `~/.agents/skills` 与上游 `AGENTS_DIR` 常量相同
- [skill_manager.rs:476](../src-tauri/src/services/skill_manager.rs#L476) `resolve_update_target_install_dir` 已正确处理**相对**链接目标
- [scan_local_skills](../src-tauri/src/services/skill_manager.rs#L1841) 用 `canonicalize` 去重并回填 `linked_tools`
- [link_fs.rs](../src-tauri/src/services/link_fs.rs) `is_dir_link` 同时识别 junction 与 symlink

### 1.2 现场失联证据（开发机实测）

```
~/.agents/.skill-lock.json  →  记录 ui-ux-pro-max、baoyu-design
~/.agents/skills/           →  这两个目录已不存在          ← 幽灵条目
~/.claude/skills/officecli  →  真实目录而非链接（--copy）   ← 账本未记录，且 T5 必须处理
```

### 1.3 skills.sh 接口现状（2026-08-08 实测）

| 端点 | 匿名访问 | 说明 |
|---|---|---|
| `GET /api/v1/skills/audit/{source}/{skill}` | ⚠️ 实测 200，**但文档标注需认证** | 多厂商审计聚合，见下方警示 |
| `GET /api/v1/skills` | ❌ 401 | 分页排行榜（all-time / trending / hot） |
| `GET /api/v1/skills/search` | ❌ 401 | |
| `GET /api/v1/skills/curated` | ❌ 401 | 官方精选集 |
| `GET /api/v1/skills/{source}/{skill}` | ❌ 401 | 详情 + 文件快照 |
| `GET /api/search?q=&limit=&owner=`（旧） | ✅ 200 | CLI 仍在用，**无版本、无契约、无文档** |
| `sitemap-skills-{1,2}.xml` | ✅ 各 10,000 条 | |
| `sitemap-owners.xml` | ✅ 17,054 条 | |
| `robots.txt` | — | `Disallow: /api/`、`/search`、`/internal/`、`/debug-security/` |

认证方式：Vercel OIDC（`Authorization: Bearer $VERCEL_OIDC_TOKEN`），需应用**部署在 Vercel** 并开启 OIDC Federation，token 为请求级短期 JWT。限流 600 req/min per (team, project)。

> **对我们的硬约束**：桌面 Tauri 应用与 GitHub Actions **都不是 Vercel 运行时**，无法天然获得 OIDC token。除非取得其他授权方式，否则 `/api/v1` 的四个受保护端点在本项目中**不可用**。
>
> **audit 端点的特别警示**：虽然实测匿名返回 200，但官方文档在该端点下明确写着 **"Authentication Required. Authenticated requests are rate-limited to 600/min per (team, project)."**。因此**匿名可用只是当前实现状态，不是公开契约**——随时可能收紧。叠加 `robots.txt` 的 `Disallow: /api/`，本方案不得把任何功能建立在「audit 端点将持续匿名开放」的假设上。T3-3a 因此与旧搜索接口同级：**feature flag 默认关闭，标注实验性**，待 T0-1 书面确认后再评估是否转正。

### 1.4 audit 端点实测样本（`anthropics/skills/pdf`）

```json
{"id":"anthropics/skills/pdf","source":"anthropics/skills","slug":"pdf","audits":[
 {"provider":"Gen Agent Trust Hub","status":"pass","riskLevel":"SAFE","auditedAt":"2026-02-17T18:51:16Z",
  "categories":["PROMPT_INJECTION","EXTERNAL_DOWNLOADS"]},
 {"provider":"Socket","status":"pass","summary":"No alerts","auditedAt":"2026-03-18T16:47:53Z"},
 {"provider":"Snyk","status":"fail","riskLevel":"HIGH","summary":"Risk: HIGH · No issues","auditedAt":"2026-02-17T22:15:27Z"},
 {"provider":"Runlayer","status":"pass","riskLevel":"LOW","summary":"1/12 files flagged","auditedAt":"2026-02-26T16:19:47Z"},
 {"provider":"ZeroLeaks","status":"pass","riskLevel":"NONE","summary":"Score: 93/100","auditedAt":"2026-04-16T08:57:19Z"}]}
```

可观察到的四个特征（构成 §二 的定位依据，也是 UI 展示时必须如实呈现的事实）：

1. **陈旧**：最新一条已 4 个月（今日 2026-08-08）
2. **矛盾**：Snyk `fail`/`HIGH` 与其余四家 `pass` 直接冲突，且其 summary 自相矛盾（"Risk: HIGH · No issues"）
3. **粗粒度**：一句话摘要，无文件、无行号、无可复现依据
4. **装后生成**：文档明载 audit「在技能首次被安装后才自动生成」，长尾技能**装前无数据**；无审计时返回 404

---

## 二、差异化定位（rev1 已废弃，此为重写）

**废弃的表述**：~~「全网唯一能给市场列表打安全标」~~、~~「上游短期不会做安全层」~~。事实相反：上游已聚合 5 家厂商并提供公开 audit API 与 `/audits` 排行榜页。

**新定位——四条可验证的差异**：

| 维度 | skills.sh audit | agent-skills-guard |
|---|---|---|
| 时机 | 装**后**自动生成，长尾装前无数据（404） | **装前强制扫描**，无数据即无法安装 |
| 新鲜度 | 实测最新 4 个月前 | 本机实时，针对**实际将要落盘的那份内容** |
| 可解释性 | 一句话摘要 | 文件 + 行号 + CWE + 修复建议（现有 `SecurityIssue` 模型已具备） |
| 结论一致性 | 5 家打架，用户无从判断 | **统一、可解释、可复现**的本地策略结论（strict/default 可配） |
| 覆盖面 | 仅 skills.sh 收录的技能 | **任意** GitHub 仓库、本地目录、任意来源 |
| 执行位置 | 云端第三方 | 本机，代码不出网 |

产品话术改为：**「skills 生态的本地安全网关」**——不与云端审计竞争覆盖面，而是提供装前的、可解释的、针对真实落盘内容的最后一道闸。上游的多厂商结论作为**补充参考信号**引入（T3），并如实标注其提供方与审计时间。

### 能力边界（必须在 UI 与文档中同步声明）

我方是**静态扫描**，存在固有的漏报与误报。以下表述为对外口径，不得省略：

> **Safe 的准确含义**：在指定扫描策略（`default` / `strict`）、扫描范围与扫描器版本下，未发现已覆盖的常见危险模式。**不构成该技能绝对安全的保证。**

必须同时明确的三点：

1. **不覆盖运行时行为**：技能在实际执行中通过网络拉取并执行的内容、由 LLM 动态生成的命令，静态扫描无法预知
2. **不构成对第三方结论的否定**：我方 `Safe` 而某厂商 `fail` 时，UI 并列展示两者，不做裁决，也不暗示我方更权威
3. **"单一结论"不等于"更准确"**：它的价值在于口径统一、依据可查、结果可复现，而非命中率更高

`scan_completeness != "full"`（截断/部分扫描）时，**一律不得显示 Safe**。

---

## 三、目标与非目标

### 目标

1. 消除与 `npx skills` 的账本失联，做到**只读方向**完全可见；写入方向作为受控可选能力
2. 把 npx 安装通道纳入安全扫描覆盖范围
3. 市场从「人工精选 Claude 插件」切换为「自有自动化索引 + 装前扫描闭环」
4. **保留 GitHub 直装作为默认安装路径**，不引入 Node 强依赖

### 非目标

- 不重写 vercel 的 provider 体系（走 T11 委托，不自研）
- 不做 skills.sh 的批量抓取（robots 禁 `/api/`；未获授权前 CI 不批量调用其任何 API）
- 不承诺与上游账本的**严格双向一致**，只承诺「尽力同步 + 绝不破坏」
- 不删除 Claude Code plugin 能力，降级为市场的一个分类

---

## 四、总体架构

```
                        ┌──────────────── 发现层 ────────────────┐
   A 编辑精选(YAML,人工)   B 自有CI索引(权威)   C skills.sh 补充信号(可降级)   D Claude插件(YAML)
                        └───────────────┬───────────────────────┘
                                        │  统一 MarketplaceItem
                        ┌───────────────▼───────────────────────┐
                        │  安装层（三出口，均强制装前扫描）          │
                        │  ① Guard 直装(GitHub, 默认, 无 Node)     │
                        │  ② 复制 npx 命令                        │
                        │  ③ 委托 npx → staging → 扫描 → 提升(可选)│
                        └───────────────┬───────────────────────┘
                        ┌───────────────▼───────────────────────┐
                        │  ~/.agents/skills/<name>  (规范目录)     │
                        │  + 各 agent 目录软链/副本                │
                        │  SQLite(权威) ← 只读 → .skill-lock.json  │
                        │             ⇢ 写回仅在 flag 开启时        │
                        └───────────────────────────────────────┘
```

**关键原则**：C 层永远是**补充**，任何时刻不可用都不得影响 A/B/D 与安装流程。

---

## 五、工作项详解

### M0 — 技术验证与契约冻结（目标：1 周，不产出用户可见功能）

#### T0 API 契约验证与授权确认

必须在 M1 开工前拿到明确结论的五件事：

| 项 | 内容 | 判定 |
|---|---|---|
| T0-1 | 联系 skills.sh / vercel-labs，确认非 Vercel 运行时的授权路径（API key？公开只读配额？）以及 **audit 端点的匿名使用许可**（文档标 Required，实测 200，需澄清哪个是准） | 有书面授权 → T3-3a 可转正；无 → 永久保持 flag 关闭的实验态 |
| T0-2 | 明确旧 `/api/search` 的定位（是否将下线） | 无承诺 → 只能作为 feature flag 后的实验功能 |
| T0-3 | 冻结上游 CLI 版本与 schema：记录 `skills@1.5.22` 的 commit、`skill-lock.ts` 的 `CURRENT_VERSION=3`、`agents.ts` 的 commit hash 到 `docs/upstream-pin.md` | 后续所有生成脚本以此 pin 为准 |
| T0-4 | 定义索引内容哈希算法与威胁模型（见 T8 附注） | 产出一页 spec |
| T0-5 | `asg-scan` PoC（见下方验收清单） | 确认无 tauri/SQLite 依赖，且不影响现有构建 |
| T0-6 | **GitHub API 配额 dry-run** | 决定 T8 是否必须改 GraphQL 批处理 |
| T0-7 | **整目录指纹性能基准** | 决定 T2 的缓存策略是否必要 |

#### T0-5 `asg-scan` PoC 验收清单

1. 独立编译运行：`cargo build --bin asg-scan` 成功，且 `cargo tauri build` 行为无变化（不新增 feature、不改默认 target）
2. stdout 仅有 JSON，可直接 `| jq`；所有日志与诊断进 stderr
3. 对同一 fixture，等级、分数、issue 集合与 GUI 内扫描**完全一致**
4. 输出排序稳定（issue 按 `file_path` + `line_number` 排序），且**不含动态时间字段**（`scanned_at` 由调用方注入，不由 CLI 生成），保证同输入两次运行字节级一致
5. i18n 在无 tauri 上下文下可初始化，默认 `en`

#### T0-6 GitHub API 配额 dry-run（新增，直接决定 T8 可行性）

sitemap 实测：**19,968 个技能 URL、2,458 个唯一仓库、2,049 个唯一 owner**。

朴素估算「每仓库 1 次元数据 + 1 次 Trees」= **4,916 次请求**，已贴近 GitHub token 的 5,000 次/小时上限，且尚未计入：

- 默认分支 / commit sha 补查
- Trees API 返回 `truncated: true` 后的递归补查（大仓库必现）
- 失败重试
- 与其他 workflow 共享同一 token 额度

**dry-run 内容**：取 100–200 个真实仓库跑完整流程，统计每仓库实际请求数与 `truncated` 命中率，线性外推到 2,458。

**据此决定**：
- REST 是否必须换成 **GraphQL 批处理**（单次查询多仓库的 `stargazers`/`defaultBranchRef`/`object(expression:)`）
- 跨运行缓存的键设计（`repo → (etag, pushed_at, tree_sha)`，未变更仓库零请求）
- **rate-limit 续跑**：命中 429/剩余额度低于阈值时保存进度并在下次 workflow 续跑，而非失败重来

**产出**：`docs/upstream-pin.md` + `docs/index-hash-spec.md` + `docs/t0-quota-dryrun.md` + T0-1 的书面答复（或明确「无答复」的结论）。

#### 门禁矩阵（rev3 的 §M0 与 §路线图 表述冲突，此处统一）

> rev3 一处说「T0 全部完成才能进 M1」，另一处又说「等外部答复时可并行开工 T1/T2」。两者矛盾。实际依赖是**逐项**的，不是整体的：

| 工作项 | 依赖的 T0 子项 | 是否依赖外部答复 | 可否立即开工 |
|---|---|---|---|
| T1 | T0-3（上游 pin）、T0-4（哈希 spec） | ❌ 否 | ✅ 是 |
| T2 | T0-7（性能基准） | ❌ 否 | ✅ 是 |
| T7 | T0-5（PoC） | ❌ 否 | ✅ 是 |
| T3 | **T0-1（授权）、T0-2（旧接口定位）** | ✅ **是** | ❌ 阻塞 |
| T8 | T0-4、**T0-6（配额）** | ❌ 否（T0-6 是内部验证） | ⏸ 待 T0-6 结论 |
| T4 / T5 / T6 / T11 | T0-3 | ❌ 否 | ✅ 是（但排期在 M4） |

**结论**：只有 **T3** 真正被外部答复阻塞。T0-5 / T0-6 / T0-7 均为内部验证，可立刻并行执行；M1（T1+T2）在 T0-3/T0-4/T0-7 完成后即可开工，无需等待 T0-1/T0-2。

---

### M1 — 只读兼容与扫描新鲜度（目标版本 v1.4.0）

#### T1 只读 lock 识别 + 多信号匹配 + `update_provider`

**新增** `src-tauri/src/services/skill_lock.rs`

```rust
pub const SUPPORTED_LOCK_VERSIONS: &[u32] = &[3];   // 与上游 CURRENT_VERSION 对齐

/// 强类型字段 + 未知字段原样保留。序列化时 flatten 回写，杜绝字段丢失。
#[derive(Serialize, Deserialize, Clone)]
pub struct SkillLockEntry {
    pub source: String,
    #[serde(rename = "sourceType")]    pub source_type: String,
    #[serde(rename = "sourceUrl")]     pub source_url: String,
    #[serde(default)] pub r#ref: Option<String>,
    #[serde(default, rename = "skillPath")]        pub skill_path: Option<String>,
    #[serde(default, rename = "skillFolderHash")]  pub skill_folder_hash: Option<String>,
    #[serde(rename = "installedAt")]   pub installed_at: String,
    #[serde(rename = "updatedAt")]     pub updated_at: String,
    #[serde(default, rename = "pluginName")]       pub plugin_name: Option<String>,
    #[serde(default, rename = "sourceBaseUrl")]    pub source_base_url: Option<String>,
    #[serde(default, rename = "wellKnownDigest")]  pub well_known_digest: Option<String>,
    /// 上游未来新增的任何字段
    #[serde(flatten)] pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize)]
pub struct SkillLockFile {
    pub version: u32,
    pub skills: BTreeMap<String, SkillLockEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub dismissed: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "lastSelectedAgents")]
    pub last_selected_agents: Option<Vec<String>>,
    #[serde(flatten)] pub extra: serde_json::Map<String, serde_json::Value>,   // 顶层未知字段
}

pub enum LockState {
    Absent,                       // 文件不存在
    Ok(SkillLockFile),            // 版本受支持、结构完整
    UnsupportedVersion(u32),      // 只读
    Corrupt(String),              // 只读，且**永不写入**
}

pub fn lock_path() -> Option<PathBuf>;   // $XDG_STATE_HOME/skills/.skill-lock.json 优先，否则 ~/.agents/.skill-lock.json
pub fn read_lock() -> LockState;
```

> **rev1 缺陷修正**：原方案的结构体遗漏了 `dismissed`、`lastSelectedAgents`、`sourceBaseUrl`、`wellKnownDigest`，强类型往返会静默删除它们。现以 `#[serde(flatten)] extra` 在顶层与条目两级兜底。

**匹配规则（rev1 仅按目录名匹配，会把同名手工副本误判为 CLI 技能）**

采用加权多信号，**全部满足才判定为 CLI 管理**：

1. 技能位于规范目录 `~/.agents/skills/<name>`（必要条件）
2. 目录名 == lock key
3. `entry.skill_path` 的 basename 与磁盘上的 `SKILL.md` 相对位置自洽
4. `SKILL.md` frontmatter 的 `name` 与 lock key 一致（允许 `sanitizeName` 归一化后比较）

任一不满足 → 降级为 `Manual`，并在 UI 标注「疑似手工副本，来源未确认」。

**新增字段**（[models/skill.rs](../src-tauri/src/models/skill.rs)，均 `#[serde(default)]`）：

```rust
#[serde(default)] pub source_type: Option<String>,       // github | local | node_modules | mintlify | ...
#[serde(default)] pub managed_by: SkillOrigin,           // Guard | SkillsCli | Manual
#[serde(default)] pub update_provider: UpdateProvider,   // GuardGithub | SkillsCli | None
#[serde(default)] pub content_fingerprint: Option<String>, // 见 T2
```

**`update_provider` 判定表**（rev1 的「回填后即可更新」不成立，此为修正）

当前更新链的三个硬性前置：`installed_commit_sha` 存在（否则 [github.rs:1010](../src-tauri/src/services/github.rs#L1010) 直接返回 `Unknown`）、`repositories` 表存在匹配行（否则 [skill_manager.rs:2537](../src-tauri/src/services/skill_manager.rs#L2537) 报 `REPOSITORY_NOT_FOUND`）、`repository_url` 可被 `Repository::from_github_url` 解析（[commands/mod.rs:1359](../src-tauri/src/commands/mod.rs#L1359)）。

| lock `sourceType` | 能否解析成 GitHub URL | `update_provider` | UI 行为 |
|---|---|---|---|
| `github` | ✅ | `GuardGithub` | 显示"检查更新"，需先补齐 sha 与 repo 行（见下） |
| `github` | ❌（私有/SSH/异常） | `SkillsCli` | 显示"由 skills CLI 管理"+ 复制 `npx skills update <name>` |
| `local` / `node_modules` / `mintlify` / `well-known` / 其他 | — | `SkillsCli` | 同上 |
| 无 lock 记录 | — | `None` | 仅显示来源未知，不提供更新入口 |

**`GuardGithub` 的补齐流程**（作为 T1 的子任务）—— 改为**目录级 tree SHA 模型**

> **rev2 缺陷修正**：rev2 写的是「由 `source_url` + `skill_path` 调 `fetch_latest_commit_sha_for_path`」。但 lock 的 `entry.skill_path` 是**文件**路径（开发机实测值：`".claude/skills/ui-ux-pro-max/SKILL.md"`），直接传入会把查询退化为**单文件级**——只改 `scripts/*.py` 而 `SKILL.md` 不变的提交会被漏掉。
>
> **同时澄清一个本仓库的事实**：现有远程更新检测**实测是目录级的**。`check_skill_update` 收到的 `skill.file_path` 存的是技能**目录**相对路径（[github.rs:164](../src-tauri/src/services/github.rs#L164) 取目录项 `item.path`；[github.rs:895](../src-tauri/src/services/github.rs#L895) 取 `skill_dir.strip_prefix(repo_root)`），而 GitHub commits API 的 `path` 参数传目录时**当前会**追踪整个子树。因此这不是既有功能的 correctness bug，而是 T1 新代码的输入转换 bug。
>
> ⚠️ **但目录递归只是实测行为，不是契约**：GitHub 官方文档对 `path` 参数的表述仅为 "Only commits containing this file path will be returned"（[List commits](https://docs.github.com/en/rest/commits/commits)），**未承诺目录递归语义**。这正是本方案改用 tree SHA 的独立理由之一——不能把检测正确性建立在未文档化的实现行为上。

**修正后的流程**：

1. **路径归一化**：`dir_path = dirname(entry.skill_path)`，若为空则 `"."`。**禁止**把 lock 的原始 `skill_path` 直接传给任何按路径查询的 API
2. **遵守 ref**：解析 `entry.ref`（分支/tag），缺省时用仓库默认分支。rev2 完全忽略了该字段
3. **取目标 ref 当前 commit sha**
4. **提取该 commit 下 `dir_path` 的 tree SHA**（Trees API）
5. **与 `entry.skillFolderHash` 比对**：
   - 一致 → 内容即最新。写入 `installed_commit_sha`（供下载与溯源）+ `installed_tree_sha = skillFolderHash`，标记 UpToDate
   - 不一致 → 上游内容已变，但**安装时点未知**。写入 `installed_tree_sha = skillFolderHash`（这是唯一可信的锚点），`installed_commit_sha` 留空，标记 UpdateAvailable
6. 在 `repositories` 表 upsert 对应仓库行（`enabled=false`，仅供更新链解析，不进入仓库列表 UI）

**DB 新增** `installed_tree_sha: Option<String>`，职责划分明确：

| 字段 | 职责 |
|---|---|
| `installed_tree_sha` | **目录内容变更检测的唯一依据** |
| `installed_commit_sha` | 仅用于下载指定版本与溯源展示，**不再承担变更检测** |

**既有更新链改造（T1 的必做项，不可推迟到 M2）**

> **rev3 缺陷修正**：rev3 把这项列为「M2 搭车项、非 T1 阻塞」，与 M1 的设计直接冲突：T1 在 tree 不一致时会写入 `installed_tree_sha` 而把 `installed_commit_sha` 留空，但现有 `check_skill_update` 遇到空 `installed_commit_sha` **直接返回 `Unknown`**（[github.rs:1010](../src-tauri/src/services/github.rs#L1010)）；而 DB 中**没有任何持久化的 `UpdateAvailable` 状态**可供兜底——`SkillStatus` 枚举（[skill.rs:90](../src-tauri/src/models/skill.rs#L90)）自定义以来全仓库零引用，skills 表也无 `status` 列。结果就是「标着 `GuardGithub` 却永远查不出更新」的死角。
>
> 已选方案：**把 tree SHA 优先分支移入 T1**，使 `installed_tree_sha` 在 M1 内真正生效。（另一可选方案是缩减 M1——T1 只识别来源、所有 CLI 技能暂设 `SkillsCli`，到 M2 再切换。不采用，因为那会让 M1 失去「可更新」这一最主要的用户可感价值。）

`check_skill_update` 改为双分支，**tree SHA 优先**：

```
if installed_tree_sha.is_some():
    取目标 ref 当前 commit → 取该 commit 下 dir_path 的 tree SHA
    与 installed_tree_sha 比对 → UpToDate | UpdateAvailable
elif installed_commit_sha.is_some():
    走现有 commits-API 路径过滤逻辑（保留，作为无 tree SHA 的老数据的兼容路径）
else:
    Unknown
```

tree SHA 分支不受**目录改名、force push、历史重写**影响，而 commits API 的路径过滤在这些场景下会退化为 `Unknown`。两条分支长期并存，老数据在首次成功比对后回填 `installed_tree_sha` 完成自愈（复用现有 `UpToDate { canonical_sha }` 的自愈模式）。

**绝不做**：写入伪造的 `installed_commit_sha` 或 `installed_tree_sha`。宁可显示「由 skills CLI 管理」。

**幽灵条目**：lock 中存在但磁盘不存在 → 列入「账本异常」区，提供说明与一键清理（清理动作属 T4，M1 只展示不执行）。

**验收**：
- npx 安装的技能显示正确来源与 `skills CLI` 徽章
- `ui-ux-pro-max` / `baoyu-design` 出现在「账本异常」区
- lock 缺失 / JSON 损坏 / version=4 / 条目结构不全 → 四种情况均静默降级为纯本地扫描，不崩溃、不报错、不写入
- 手工放置的同名目录不会被误认为 CLI 技能

**工作量**：8–9 天（rev1 为 2–3 天；rev2 上调至 5–6 天用于多信号匹配与 sha 补齐；rev3 加 tree SHA 模型与 `installed_tree_sha` 迁移；rev4 再纳入 `check_skill_update` 双分支改造与老数据自愈）

---

#### T2 整目录扫描指纹 + 未扫描告警

> **rev1 缺陷修正**：原方案用 `scanned_at < 目录 mtime` 判定过期。修改目录**内部**文件通常不更新父目录 mtime；而现有 checksum 只覆盖 `SKILL.md`（[skill_manager.rs:2047](../src-tauri/src/services/skill_manager.rs#L2047)），安全扫描却递归整个目录（[scanner.rs:1416 `scan_directory`](../src-tauri/src/security/scanner.rs#L1416)）。因此 npx 更新 `scripts/*.py` 而 `SKILL.md` 不变时，会继续显示旧结论——这是**安全结论失真**，等级 P0。

**新增** `content_fingerprint`：与扫描遍历规则**严格一致**的整目录指纹。

```
fingerprint = sha256( 逐条拼接 sorted_by_relpath[ relpath \0 size \0 sha256(content) \n ] )
```

约束：
- 遍历规则必须复用 `scan_directory` 的同一份 walk 逻辑与忽略规则（抽成共享函数，**禁止两处各写一遍**）
- 符号链接不跟随，记录为 `relpath \0 LINK \0 sha256(target_string)`
- 超大文件按扫描器既有截断策略处理，并在指纹中记录截断标志

**关于 mtime——rev2 的写法自相矛盾，此处推翻重写**

> rev2 一边说 mtime「不作为判定依据」，一边又说用它「跳过明显未变的目录」。只要根目录 mtime 未变就跳过哈希计算，mtime 事实上**就是**判定依据，原漏洞（改子目录内文件不更新父目录 mtime）原样保留。

**新规则**：

1. **禁止**以技能根目录 mtime 为条件跳过指纹计算
2. 允许**逐文件**内容哈希缓存，缓存键 = `(相对路径, 文件大小, mtime_ns)`。三者任一变化即重算该文件的哈希；**目录结构（文件增删）永远重新遍历**，不受缓存影响
3. 必须提供**强制全量重算**入口（设置页 + `--no-cache` 等价能力），用于缓存可疑时的兜底
4. 缓存仅为性能优化，**语义上等价于每次全量计算**——单测须锁定这一点（同一目录，有缓存与无缓存的指纹必须相同）

性能兜底由 T0-7 基准决定：若全量哈希在典型技能规模（实测你机器上 11 个技能）下耗时可接受，缓存可延后实现。

**判定**：`scanned_at IS NULL` 或 `content_fingerprint != 当前计算值` → 未扫描/已过期。

**新增命令** `list_unscanned_skills() -> Vec<UnscannedSkill>`，Overview 页告警卡片 + 一键批量补扫。指纹计算走 `rayon` 并在 `spawn_blocking` 中执行（遵循 [perf-stability-review](./perf-stability-review-2026-08-07.md) 的 R2 结论）。

**暂不做**：文件系统监听（`notify`）。Windows 稳定性成本高于收益，M1 用「启动时 + 手动刷新时」比对即可。

**验收**：修改技能目录内任一非 `SKILL.md` 文件（含新增/删除/改内容）后刷新，该技能均标记为需重扫。

**工作量**：4 天

---

### M2 — 扫描器解耦与索引试运行（目标版本 v1.5.0-beta）

#### T7 安全扫描器抽为 headless CLI

> **rev1 错误修正**：rev1 称复用 `pipeline.rs` 的 `SecurityPipeline`——**该类型不存在**。`pipeline.rs` 只导出 `pub fn analyze(ctx: &SkillContext) -> Vec<Finding>`（[pipeline.rs:1195](../src-tauri/src/security/pipeline.rs#L1195)），是众多分析器之一。真实入口是 [scanner.rs:79](../src-tauri/src/security/scanner.rs#L79) 的 `SecurityScanner`，对外方法为 `scan_directory` / `scan_directory_with_options` / `scan_file`。
>
> **好消息**：`SecurityScanner` 本身**不依赖 SQLite**（DB 交互都在 `skill_manager` 层），抽离比 rev1 估计的简单。工作量由 4–5 天下调至 3 天。

`src-tauri/src/bin/` 当前是空目录，正好用于此。

**新增** `src-tauri/src/bin/asg-scan.rs`，Cargo.toml 显式声明 `[[bin]] name = "asg-scan"`。

```
asg-scan --input <dir> [--policy strict|default] [--timeout-secs 60] [--format json]
stdout: { "schema_version": 1, "scanner_version": "1.5.0", "policy": "default",
          "level": "Safe|Low|Medium|High|Critical", "score": 0-100,
          "content_fingerprint": "<与 T2 同算法>",
          "completeness": "full|truncated|partial",
          "issues": [ { severity, description, file_path, line_number, cwe_id, remediation } ] }
exit: 0 扫描完成（无论结论）| 2 超时 | 3 输入无效 | 1 内部错误
```

真正的工作量集中在四处（而非 DB 解耦）：

1. **i18n**：`rust-i18n` 在无 tauri 上下文时的 locale 初始化，CLI 固定输出 `en` 并允许 `--locale` 覆盖
2. **日志**：诊断信息一律走 stderr，stdout 只输出纯 JSON（CI 需可直接 pipe）
3. **稳定 schema**：`schema_version` 独立于应用版本，任何字段变更需 bump
4. **超时与完整性**：超时不得输出半截 JSON；截断时必须置 `completeness`，且下游禁止把 `truncated` 当作 `full` 采信

**验收**：`asg-scan --input ~/.agents/skills/pdf | jq .level` 输出与 GUI 内扫描结论一致；同一输入两次运行输出字节级一致（可重现性）。

**工作量**：3 天

---

#### T8 CI 索引构建管线（带内容哈希）

**新增** `scripts/build-marketplace-index.mjs` + `.github/workflows/build-marketplace-index.yml`（每日 03:00 UTC + 手动触发）

| 步 | 动作 | 要点 |
|---|---|---|
| 1 | 拉 `https://www.skills.sh/sitemap.xml` | **遍历 sitemapindex**，不硬编码分片数（当前 2 片，每片上限 10000） |
| 2 | 解析 `owner/repo/skillName` 全集（约 2 万） | 仅取 URL 结构，不请求详情页 |
| 3 | 按 repo 聚合（约数千） | |
| 4 | GitHub API 补元数据 | stars / license / `pushed_at` / archived / **默认分支当前 commit sha**；**优先 GraphQL 批量查询**（见下），REST 仅作回退；ETag + 跨运行缓存 |
| 5 | Trees API 定位 `SKILL.md`，读 frontmatter `description` | 一次拿全树，**不爬 skills.sh 详情页**；必须处理 `truncated: true` 的递归补查 |
| 6 | **稀疏 checkout 目标技能目录，记录 `commit_sha` 与 `content_hash`** | `content_hash` 用 T2 同算法 |
| 7 | 对 top N（首期 300–500）跑 `asg-scan` | 结果与**第 6 步的 hash 绑定** |
| 8 | 中文描述：仅对新增/变更条目批量机翻，结果落缓存 | 避免每日重译 |
| 9 | 产出分片 JSON + gzip | `marketplace/index.json`（轻摘要）+ `shards/*.json` |
| 10 | 提交回仓库 | 沿用 [FEATURED_MARKETPLACES_REMOTE_URL](../src-tauri/src/commands/featured_marketplaces.rs#L5) 的分发路径 |

**不做**：CI 内批量调用 skills.sh 任何 API（含 audit）。安装量等信号在 T0-1 拿到许可前不入索引；未获许可则该字段永久缺省。

**API 配额设计（T0-6 的直接产物，rev2 完全缺失）**

实测规模：**2,458 个唯一仓库**。朴素 REST 方案约 4,916 次请求即触顶 5,000/小时。三项强制措施：

1. **GraphQL 批处理**：单次查询携带 ~50 个仓库的 `stargazerCount` / `licenseInfo` / `defaultBranchRef.target.oid` / `pushedAt`，把步骤 4 从 2,458 次压到约 50 次（GraphQL 按 node 计点，需按实测点数校准批大小）
2. **跨运行缓存**：`repo → (etag, pushed_at, default_branch_oid, tree_sha, description_map)` 持久化在索引仓库。`pushed_at` 未变的仓库**零请求**跳过，日常增量应只触及数十个仓库
3. **限流续跑**：监控 `X-RateLimit-Remaining`，低于阈值（如 200）时**保存进度快照并正常退出**，下次 workflow 从断点续跑；绝不因限流失败重来。首次全量允许跨多次 workflow 完成

**估算调整**：T8 由 rev2 的 6–7 天上调至 **9–10 天**（新增 GraphQL 层、缓存层、续跑机制）。

> **rev1 缺陷修正（TOCTOU）**：rev1 的 schema 只记 `scanned_at`，用户看到 Safe 时默认分支可能已变，实际安装到另一份内容。

**索引 schema v3**：

```json
{
  "schema_version": 3,
  "generated_at": "2026-08-08T03:00:00Z",
  "skills": [{
    "id": "anthropics/skills/pdf",
    "name": "pdf",
    "repo": "anthropics/skills",
    "skill_path": "document-skills/pdf/SKILL.md",
    "description": { "en": "...", "zh": "..." },
    "stars": 12043, "license": "MIT", "pushed_at": "2026-07-30T00:00:00Z",
    "resolved_ref": "main",
    "commit_sha": "a1b2c3d4...",
    "content_hash": "sha256:...",
    "scan": {
      "level": "Safe", "score": 96,
      "scanned_at": "2026-08-08T03:12:00Z",
      "scanner_version": "1.5.0",
      "scan_policy_version": "default@3",
      "scan_completeness": "full",
      "scanned_content_hash": "sha256:..."
    }
  }]
}
```

**徽章使用铁律**（T9 强制实现）：

1. 市场卡片上的评级一律标注为**「市场快照评级 · 基于 commit `a1b2c3d`」**，绝不显示为当前状态
2. 安装时**必定重新扫描**实际下载内容
3. 若下载内容的 `content_hash` ≠ 索引 `scanned_content_hash` → **丢弃快照评级**，只显示本次扫描结论，并提示「上游内容已变更」
4. `scan_completeness != "full"` 的条目不得显示为 Safe，显示「部分扫描」
5. 索引无 `scan` 字段 → 显示「未扫描」，不得留白或默认安全

> **rev1 缺陷修正（回传）**：rev1 提出「用户点开详情时后台扫描并回传公共索引」。该设计缺少接收后端、身份认证与防伪，任何人可污染公共评级。**已删除**。按需扫描的结果仅存本机 SQLite 缓存，不回传。公共索引的扩容只能靠 CI 提高 top N。

**工作量**：9–10 天（rev2 为 6–7 天，rev3 因 GraphQL 批处理 / 跨运行缓存 / 限流续跑上调）

---

### M3 — 市场重构与装前扫描闭环（目标版本 v1.5.0）

#### T3 skills.sh 数据接入（降级为 P1，形态改变）

**新增** `src-tauri/src/services/skills_registry.rs`，两个能力，分别独立开关：

**3a. audit 补充信号（feature flag，默认关闭）**

> **rev2 缺陷修正**：rev2 设为「默认开启」，与 T0-1「先确认使用许可」自相矛盾。官方文档在该端点标注 `Authentication Required.`，匿名 200 只是当前实现状态。「用户主动单次请求」能控制频率，但**不能替代授权，也不能把非契约行为变成契约**。故降为与 3b 同级的实验功能，默认关闭，设置项文案注明「该端点官方标注需认证，当前匿名可用属实现细节，可能随时失效」。T0-1 取得书面确认后方可转正为默认开启。

- 开启后，用户**点开技能详情时**按需请求 `GET /api/v1/skills/audit/{source}/{skill}`（单条、用户触发、不批量）
- 结果作为「第三方审计参考」区块展示，**必须逐条标注 provider 与 `auditedAt`**
- 必须显示我方结论在先、第三方在后；当多家结论冲突时（如实测的 Snyk `fail` vs 其余 `pass`）**如实并列展示，不做合并裁决**
- 404（未审计）是常态，显示「暂无第三方审计」而非错误
- 结果缓存 24h；失败静默

**3b. 旧搜索接口（feature flag，默认关闭）**
- `GET /api/search?q=&limit=&owner=`，`q.len() < 2` 本地拦截
- 定位为**实验功能**，设置页显式开关 + 说明「该接口无官方契约，可能随时失效」
- 300ms 防抖、10 分钟 LRU 缓存、`User-Agent: agent-skills-guard/<version>`
- 任何异常 → 空结果 + 「搜索服务不可用」，绝不阻塞页面

若 T0-1 取得正式授权，3b 升级为基于 `/api/v1/skills/search` 的正式能力，届时另行评估。

**工作量**：4 天

---

#### T9 市场页面重构 + 装前扫描闭环

**修改** [MarketplacePage.tsx](../src/components/MarketplacePage.tsx)、[FeaturedRepositories.tsx](../src/components/FeaturedRepositories.tsx)

| Tab | 数据源 | 说明 |
|---|---|---|
| 精选 | A（YAML） | 人工背书，默认首屏 |
| 全部技能 | B（自有 CI 索引） | 按 stars / 安全等级 / 更新时间排序筛选 |
| 搜索 | C-3b（flag 后） | 关闭时此 Tab 隐藏，改为本地索引搜索 |
| Claude 插件 | D（YAML plugin 区） | 保留现有 `/plugin install` |

安装流程强制闭环：`点击安装 → 下载到 staging → 计算 content_hash → 本地扫描 → 与索引快照比对（一致/变更/无快照三态）→ 展示结论 → 用户确认 → 提升到规范目录`。High/Critical 需二次确认并默认拒绝。

新增 i18n key 至 [zh.json](../src/i18n/locales/zh.json) / [en.json](../src/i18n/locales/en.json)。

**工作量**：5 天

#### T10 YAML 瘦身

1010 行 → 150 行以内，只留 20–30 条人工精修推荐 + Claude 插件区。新增可选 `featured_skills` 段（`id` + 双语 note）。[models/featured_marketplace.rs](../src-tauri/src/models/featured_marketplace.rs) 新增字段全部 `Option` + `#[serde(default)]`，保证旧缓存可解析。

**工作量**：1–2 天

---

### M4 — 受控写入与生态扩张（目标版本 v1.6.0）

#### T4 账本写回（feature flag，默认关闭）

> **rev1 缺陷修正**：原三条护栏不足以防止「Guard 与 npx 并发 read-modify-write 互相覆盖」，也未排除「损坏文件被覆盖为空账本」。

**七条护栏，缺一不可**：

1. **只读闸门（rev2 写错，此处修正）**：
   - `UnsupportedVersion` / `Corrupt` → **绝对只读，禁止一切写入**。特别地，`Corrupt` 状态**永不**用空账本覆盖
   - `Absent` → **允许创建**。rev2 把 `Absent` 也列入禁写，会导致「用户从未用过 skills CLI 时 Guard 永远建不出第一份 lock」，与「Guard 安装后 upsert」直接冲突。正确做法：用户开启实验开关**并在首次创建时二次确认**后，创建一份 `version: 3` 的新 lock（仅含 `version` + `skills` 两个键，不臆造 `dismissed` / `lastSelectedAgents`）
2. **未知字段保留**：经 T1 的 `#[serde(flatten)] extra` 往返，顶层与条目两级
3. **写入范围限定**：仅允许操作「Guard 自己创建、`sourceType == "github"`、且 `update_provider == GuardGithub`」的条目。CLI 创建的条目**只读**
4. **摘要比对（乐观并发）**：读取时记录整文件 `sha256`；写入前重读并比对 → 不一致说明期间有他方写入 → **中止本次写入**，与最新内容做键级合并后提示用户重试
5. **原子落盘**：同目录 `NamedTempFile` → `persist`，复用 [write_yaml_cache](../src-tauri/src/commands/featured_marketplaces.rs#L50) 模式
6. **滚动备份**：保留最近 5 次写入前快照 `.skill-lock.json.guard-bak.{1..5}`，非仅首次
7. **可回滚**：设置页提供「从备份恢复账本」

**仍存在的残余风险（必须如实告知用户）**：护栏 4 只能把竞态窗口从「秒级」压缩到「毫秒级」，无法根除——上游 CLI 不使用文件锁，我们单方面加锁无效。因此产品文案明确为**「尽力同步」**，不承诺严格一致。设置项文案：「与 skills CLI 同步账本（实验性）」。

写回规则：

| 我方操作 | lock 动作 |
|---|---|
| 安装（GitHub 源，Guard 发起） | upsert |
| 更新（同上） | 更新 `updatedAt` + `skillFolderHash` |
| 完全卸载（Guard 创建的条目） | 删除 |
| 卸载单个链接路径 | 不动 |
| 隔离（T5） | 不动 |
| 任何 CLI 创建的条目 | 不动 |

**`skillFolderHash` 缺失时的处置（rev2 写错，此处修正）**

上游 schema 将 `skillFolderHash` 定义为**必填 string**（[skill-lock.ts](https://raw.githubusercontent.com/vercel-labs/skills/main/src/skill-lock.ts)）。rev2 说「算不出就留空」会产出一条上游无法正常用于更新比对的残缺条目——等于给对方埋雷。

**正确处置（三级降级）**：

1. 正常：由 GitHub Trees API 自算 tree SHA 后写入
2. 暂时失败（限流 / 网络 / 超时）→ **不写该条目**，加入**待写队列**，在下次成功获取时补写。队列持久化在我方 SQLite，不污染 lock
3. 永久失败（源不是 GitHub、无法定位目录）→ 该技能**永不写入 lock**，`update_provider` 保持 `SkillsCli`，UI 说明「该技能未同步至 skills CLI 账本」

**绝不做**：写入残缺条目，或伪造 hash。宁可不同步。

**工作量**：5 天

---

#### T5 完整隔离（含 `--copy` 副本）

> **rev1 缺陷修正**：原流程只移动规范目录并断链。若 agent 目录中是 `--copy` 产生的**独立副本**（开发机上 `~/.claude/skills/officecli` 即是），副本仍会被 Agent 加载，**高危技能实际未被隔离**。

**新流程**：

```
1. 枚举物理实例：遍历全部已知 agent 目录（T6 完成后为 75+，之前为 5+用户自定义）
   对每个同名条目分类：链接(指向规范目录) | 链接(指向他处) | 独立副本 | 不存在
2. 生成恢复清单 quarantine-manifest.json：记录每个实例的路径、类型、原链接目标、内容 hash
3. 执行：链接 → 删除；独立副本 → 移入 ~/.agents/.quarantine/<name>/copies/<agent-id>/
        规范目录 → 移入 ~/.agents/.quarantine/<name>/canonical/
4. 后置校验：重新遍历所有已知 agent 路径，确认无任何可加载副本残留
   校验失败 → 隔离标记为"部分成功"，明确列出未能处理的路径，要求用户手工确认
5. lock 条目保持不动
```

**必须如实告知的限制**：
- 我们只能覆盖**已知**的 agent 目录。用户自定义或未收录的路径无法保证
- 保留 lock 条目只能降低上游重装概率，**不能杜绝**（`skills update` / `skills install` 仍可能重建）。UI 需提示「若使用 skills CLI 重新安装，隔离将被覆盖」

**工作量**：4 天

---

#### T6 Agent 注册表扩展

> **rev1 缺陷修正**：`{id, label, global_skills_dir, detect_paths}` 无法无损表达上游定义——上游含环境变量路径（`CLAUDE_CONFIG_DIR`/`CODEX_HOME`/`XDG_CONFIG_HOME`/`APPDATA`）、OS 特有路径、多历史目录择优（OpenClaw 的 `.openclaw`→`.clawdbot`→`.moltbot`）、`packageJsonHasDependency` 依赖检测、`globalSkillsDir: undefined`、以及自定义检测函数。

**采用「静态定义 + 有限 resolver 类型」**：

```rust
enum PathResolver {
    Fixed(&'static str),                          // ~/.claude/skills
    EnvOr { env: &'static str, fallback: &'static str },
    XdgConfig(&'static str),
    FirstExisting(&'static [&'static str]),       // OpenClaw 多历史目录
    AppData(&'static str),                        // Windows APPDATA
    None,                                         // globalSkillsDir: undefined
}
enum DetectRule { PathExists(PathResolver), MacApp(&'static str), PackageJsonDep(&'static str), Any(&'static [DetectRule]) }
```

resolver 类型是**封闭集合**；上游若出现无法表达的自定义函数，该 agent 标记 `unsupported` 并跳过，不做近似。

**生成方式**：`scripts/gen-agent-registry.mjs` 从 **T0-3 固定的上游 commit** 读取 `agents.ts`，产出 `agent_tools_generated.rs`。**手动运行 + 人工 review diff 后提交**，普通构建流程绝不联网。

UI 只展示已检测到的工具 + 用户手动添加的。数据库中已存的 `linked_tools` id 保持字符串兼容，无需迁移。

**工作量**：5 天

#### T11 委托 npx 安装（默认关闭）+ T12 项目作用域

T11：前置检测 Node ≥ 22.20（复用 [local_cli_scanner.rs](../src-tauri/src/services/local_cli_scanner.rs) 探测能力），不满足则隐藏入口。临时 cwd 内以 `DISABLE_TELEMETRY=1 DO_NOT_TRACK=1` 执行 `npx -y skills@<T0-3 pinned> add <src> -s <skill> -a universal --copy -y`，扫描 staging 通过后才提升。**工作量 5 天**。

T12：读 `./skills-lock.json`（v1，SHA-256 内容哈希）+ `./.agents/skills/`，提供项目技能安全审计报告。**工作量 5 天**。

---

## 六、安装出口设计（贯穿全案）

| 出口 | 路径 | 依赖 | 默认 |
|---|---|---|---|
| ① Guard 直装 | 解析 `owner/repo/skill_path` → GitHub 下载 → **装前扫描** → 规范目录 + 建链 | 无 | ✅ |
| ② 复制命令 | 剪贴板复制 `npx skills add <repo> --skill <name>` | 无 | |
| ③ 委托 npx | T11 流程 | Node ≥ 22.20 | 需手动开启 |

现有 [RepositoriesPage](../src/components/RepositoriesPage.tsx) 的自定义 GitHub 仓库能力**原样保留**。

---

## 七、风险登记

| 编号 | 风险 | 影响 | 应对 |
|---|---|---|---|
| R1 | 上游 lock version 升级或并发写导致数据丢失 | 用户数据损坏 | T4 七条护栏 + feature flag 默认关闭 + 明确「尽力同步」 |
| R2 | `/api/v1` 需 OIDC，我方无法获取 | C 层不可用 | T0-1 先行；未获授权则 C 层仅保留匿名 audit 单条查询 |
| R3 | 旧 `/api/search` 随时下线 | 搜索失效 | 置于 flag 后，标注实验；主搜索走本地索引 |
| R4 | robots 禁 `/api/`，批量调用有合规风险 | 被封禁/纠纷 | CI 不调用其任何 API；客户端仅用户主动触发的单条请求 |
| R5 | 索引徽章与实际内容不符（TOCTOU） | 安全误导 | T8 内容哈希 + T9 安装时必重扫 + 三态展示 |
| R6 | `--copy` 副本导致隔离失效 | 高危技能仍被加载 | T5 全实例枚举 + 后置校验 + 如实告知覆盖边界 |
| R7 | 扫描指纹与扫描遍历规则不一致 | 安全结论失真 | T2 强制复用同一份 walk 逻辑，单测锁定 |
| R8 | 索引体积拖慢启动 | 卡顿（见 [perf-review](./perf-stability-review-2026-08-07.md) R2/R4） | 分片 + gzip；启动只拉轻摘要；解析走 `spawn_blocking` |
| R9 | 上游 CLI 行为随版本漂移 | 兼容失效 | T0-3 pin 版本；T11 pin 执行版本；升级需人工 review |
| R10 | 引入 Node 依赖劝退用户 | 安装成功率下降 | T11 默认关闭，检测不到即隐藏 |

---

## 八、测试策略

| 层级 | 覆盖 |
|---|---|
| Rust 单测 | lock 五态解析（正常/缺失/损坏/未知版本/结构不全）；多信号匹配的正反例；摘要比对中止逻辑；`Absent` 可创建 / `Corrupt` 绝对只读 |
| Rust 单测 | 未知字段往返：断言 **JSON 语义等价（`serde_json::Value` 深度相等）且未知字段零丢失**。**不做字节级比较**——`BTreeMap` 会重排键序，缩进与换行也可能不同，字节级断言不现实且无意义。字节摘要仅用于 T4 护栏 4 的并发冲突检测，不用于往返测试 |
| Rust 单测 | `content_fingerprint` 与 `scan_directory` 遍历一致性（同一 fixture，两者文件集合必须相等）；有缓存与无缓存的指纹必须相同 |
| Rust 单测 | `dirname(entry.skill_path)` 归一化：`"a/b/SKILL.md"→"a/b"`、`"SKILL.md"→"."`、`entry.ref` 存在与缺省两路 |
| Rust 集成 | `tempfile` 沙盒模拟 `~/.agents`：相对软链 / junction / `--copy` 副本三形态的扫描、隔离、卸载 |
| Rust 集成 | 并发写 lock：两线程同时 read-modify-write，断言无字段丢失且冲突被检出 |
| CLI | `asg-scan` 同输入两次运行输出字节级一致；超时不输出半截 JSON；退出码矩阵 |
| 前端 | `msw` mock audit / search（200 / 401 / 404 / 500 / 超时 / 畸形 JSON 六种） |
| CI 脚本 | dry-run 模式 + 固定 sitemap 快照的快照测试 |
| 手工回归 | 装有 Claude Code + Codex + Cursor 的机器上交替执行 `npx skills add` 与 Guard 安装/卸载/隔离，核对两边账本 |

**必测边界**：lock 缺失、损坏、未知版本、条目指向不存在目录、目录存在但无条目、同名手工副本——六种状态均不得崩溃或丢数据。

---

## 九、路线图

| 里程碑 | 版本 | 工作项 | 估算 | 交付价值 |
|---|---|---|---|---|
| M0 | — | T0（含 T0-6 配额、T0-7 性能） | ~7 工作日 | 契约、授权、配额三项确定，避免围绕不稳定前提返工 |
| M1 | v1.4.0 | T1 T2 | ~13 工作日 | 消除失联（只读方向）；安全结论不再失真；目录级 tree SHA 变更检测**闭环** |
| M2 | v1.5.0-beta | T7 T8 | ~13 工作日 | headless 扫描器 + 带哈希与配额控制的索引试运行 |
| M3 | v1.5.0 | T3 T9 T10 | ~11 工作日 | 市场重构 + 装前扫描闭环 |
| M4 | v1.6.0 | T4 T5 T6 T11 T12 | ~24 工作日 | flag 后的写回、完整隔离、Agent 扩展、全来源 |

**执行顺序**（依据 §M0 的门禁矩阵，非整体串行）：

1. **T0-5** `asg-scan` PoC —— 独立、成本最低、不被任何答复阻塞
2. **T0-7** 指纹性能基准 —— 决定 T2 的缓存策略
3. **T0-6** GitHub 配额 dry-run —— 决定 T8 是否必须上 GraphQL
4. 与 1–3 并行：发出 **T0-1 / T0-2** 的授权询问（外部答复周期不可控，越早发越好）
5. T0-3 / T0-4 完成后即可开工 **M1（T1+T2）**，不等 T0-1/T0-2

**批准状态**：M0 放行；**M1 在本轮 tree SHA 闭环修正（见 T1「既有更新链改造」）后放行**；T3 保持门禁（等 T0-1/T0-2），T8 保持门禁（等 T0-6）。

---

## 附录 A — rev1 → rev2 修订清单

| 编号 | rev1 问题 | 等级 | rev2 处置 |
|---|---|---|---|
| A1 | 认为 skills.sh 无安全信息、仅有旧 `/api/search` | P0 | §1.3/§1.4 重写；差异化定位 §二 重写；新增 T0 |
| A2 | lock 结构漏 `dismissed`/`lastSelectedAgents`/`sourceBaseUrl`/`wellKnownDigest`，往返会删字段 | P0 | T1 两级 `#[serde(flatten)] extra` |
| A3 | 写回三护栏不足（损坏覆盖、并发覆盖、单次备份） | P0 | T4 扩为七护栏 + flag + 推迟至 M4 + 明确「尽力同步」 |
| A4 | T2 用目录 mtime 判定扫描过期，会漏内部文件修改 | P1 | T2 改为与扫描遍历一致的整目录指纹 |
| A5 | T1 声称回填后即可更新，实际不闭环 | P1 | 新增 `update_provider` 三态 + sha 补齐流程 + repo 行 upsert |
| A6 | T1 仅按目录名匹配，同名手工副本会被误判 | P1 | 四信号联合判定 |
| A7 | T5 未处理 `--copy` 独立副本 | P1 | T5 重写：全实例枚举 + 恢复清单 + 后置校验 + 边界告知 |
| A8 | T8 索引缺内容哈希，徽章存在 TOCTOU | P1 | schema v3 增 `resolved_ref`/`commit_sha`/`content_hash`/`scan_policy_version`/`scan_completeness`；T9 徽章五条铁律 |
| A9 | R4 提出扫描结果「回传公共索引」，无后端无认证无防伪 | P1 | 删除该设计，改为本机缓存 |
| A10 | T6 静态表无法表达上游 agent 定义 | P2 | 改为「静态定义 + 封闭 resolver 类型」，pin commit，手动生成 + 人工 review |
| A11 | T7 引用不存在的 `SecurityPipeline` | P2 | 更正为 `SecurityScanner`（scanner.rs:79）；工作量下调至 3 天；重列真实难点 |
| A12 | 基线写为 `cd05631` 且工作区干净 | P2 | 更正为 `54a964a`，并注明本文档为未跟踪文件 |
| A13 | 建议「先做 T3」 | P1 | 改为「先做 T0，T1/T2 可并行」 |

**第一轮评审中我方补正的两点**（评审意见未覆盖或有偏差）：

- **B1**：评审称 `/api/v1` 全部需 OIDC。实测 `GET /api/v1/skills/audit/{source}/{skill}` **匿名返回 200**，仅 `/skills`、`/search`、`/curated`、`/{source}/{skill}` 为 401。因此 T3 保留了一条无需授权的 audit 补充信号通道。
- **B2**：上游 audit 数据的实际质量为新差异化提供了依据——实测 `anthropics/skills/pdf` 的 5 家结论中 Snyk 为 `fail`/`HIGH` 而其余 4 家为 `pass`，且 summary 自相矛盾；最新审计距今 4 个月；且 audit 仅在技能首次被安装后生成。这些构成 §二 表格中「新鲜度/一致性/时机」三行的事实基础。

> **B1 的后续更正**：第二轮评审指出，官方文档在 audit 端点下明确标注 `Authentication Required.`。经复核属实。因此 B1 的结论需要收窄——匿名 200 是**当前实现状态而非公开契约**，不足以支撑「默认开启」。见附录 B 的 C1。

---

## 附录 B — rev2 → rev3 修订清单

| 编号 | rev2 问题 | 等级 | rev3 处置 |
|---|---|---|---|
| C1 | 把 audit 匿名可用当作契约并设为「默认开启」，与 T0-1「先确认许可」自相矛盾。官方文档实为 `Authentication Required.` | P0 | §1.3 加警示；T3-3a 改为 **feature flag 默认关闭 + 实验标注**，T0-1 书面确认后方可转正 |
| C2 | T1 补齐流程直接把 lock 的 `entry.skill_path`（文件路径）传给按路径查询的 API，退化为单文件级检测；且完全忽略 `entry.ref` | P1 | 改为**目录级 tree SHA 模型**：`dirname` 归一化 + 遵守 `ref` + 新增 `installed_tree_sha` 承担变更检测，`installed_commit_sha` 降级为下载/溯源用途 |
| C3 | T2 允许用根目录 mtime「预筛跳过」，等于把 mtime 重新变成判定依据，原漏洞回归 | P1 | 明令禁止按根目录 mtime 跳过；改为**逐文件**缓存（键含 relpath+size+mtime_ns）+ 强制全量入口 + 有无缓存指纹必须相同的单测 |
| C4 | T4 把 `Absent` 也列入禁写，导致 Guard 永远建不出第一份 lock | P1 | `Absent` 改为**允许创建**（需开关 + 二次确认）；只有 `Corrupt` / `UnsupportedVersion` 绝对只读 |
| C5 | T4「`skillFolderHash` 算不出就留空」会产出上游 schema 定义为必填的残缺条目 | P1 | 改为**三级降级**：正常写入 / 暂时失败入待写队列重试 / 永久失败则该技能永不写 lock |
| C6 | 测试要求未知字段往返「字节级一致」不现实（BTreeMap 重排、缩进差异） | P1 | 改为断言 **JSON 语义等价 + 未知字段零丢失**；字节摘要仅用于并发冲突检测 |
| C7 | §二只写优势，未声明静态扫描的能力边界 | P2 | 新增「能力边界」小节：明确 Safe 的准确含义、三条限制、`truncated` 不得显示 Safe；「单一结论」改述为「统一、可解释、可复现」 |
| C8 | T8 未做 API 配额可行性评估，6–7 天估算偏乐观 | P2 | 新增 T0-6 配额 dry-run；T8 补 GraphQL 批处理 / 跨运行缓存 / 限流续跑三项设计；估算上调至 9–10 天 |
| C9 | T0-5 PoC 无验收标准 | P2 | 补五条验收清单（独立编译、stdout 纯 JSON、与 GUI 一致、输出确定性、不改构建行为） |
| C10 | 未评估整目录指纹的性能影响 | P2 | 新增 T0-7 性能基准，据此决定缓存是否必要 |

---

## 附录 C — rev3 → rev4 修订清单

| 编号 | rev3 问题 | 等级 | rev4 处置 |
|---|---|---|---|
| D1 | T1 标 `GuardGithub` 但把 tree SHA 检测推到 M2，而现有 `check_skill_update` 遇空 `installed_commit_sha` 直接返回 `Unknown`，DB 又无持久化 `UpdateAvailable`（`SkillStatus` 枚举零引用、无 `status` 列）→ M1 承诺的「目录级 tree SHA 变更检测」无法闭环 | P1 | tree SHA 优先分支**移入 T1/M1**，双分支并存 + 老数据自愈回填。未采用「缩减 M1」的备选，因其会抽掉 M1 最主要的用户可感价值 |
| D2 | §M0「T0 全部完成才进 M1」与 §路线图「可并行开工 T1/T2」自相矛盾 | P2 | 新增**门禁矩阵**，依赖改为逐项；澄清只有 T3 真正被外部答复阻塞 |
| D3 | 把目录递归描述为既成事实 | P2 | 改述为「实测行为，非契约」，并引 GitHub 文档原文 "Only commits containing this file path will be returned"；同时说明这正是改用 tree SHA 的独立理由 |
| D4 | 附录 B 称 commits API 在 shallow history 下退化——**我方错误** | P2 | 删除。该查询在 GitHub 服务端执行，与本地 clone 深度无关。保留「目录改名、force push、历史重写」三项 |

**第二轮评审中我方补正的一点**：

- **B3**：评审称「现有更新检查按 `skill.file_path` 查询，故远程更新检测会漏掉技能目录内的辅助文件变化」。**该前提不成立**——本仓库的 `file_path` 存的是技能**目录**相对路径（[github.rs:164](../src-tauri/src/services/github.rs#L164) 取目录项 `item.path`；[github.rs:895](../src-tauri/src/services/github.rs#L895) 取 `skill_dir.strip_prefix(repo_root)`），而 GitHub commits API 的 `path` 参数传目录时会追踪整个子树，因此既有远程检测**本来就是目录级的**。真正的缺陷在 rev2 的 T1 新代码——它使用的是 lock 的 `entry.skill_path`（文件路径），这才是需要 `dirname` 归一化的地方（已记为 C2）。评审建议的 tree SHA 模型仍予采纳，但定位从「修正确性缺口」调整为「提升健壮性」：commits API 路径过滤在目录改名、force push、历史重写下会退化为 `Unknown`，且其目录递归语义未被官方文档承诺，tree SHA 比对不受这两项影响。（rev3 此处曾误列「shallow history」，rev4 已删除——该查询在 GitHub 服务端执行，与本地 clone 深度无关。）
