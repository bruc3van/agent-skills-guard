# 扫描策略设置 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在设置页增加扫描策略选择器（default/strict/permissive 三按钮组），策略通过 Tauri command 参数透传给后端扫描引擎。

**Architecture:** 前端 localStorage 存策略名，所有扫描 invoke 调用时附带 `scanPolicy` 参数。后端新增 `ScanPolicy::from_name()` 解析方法，各 Tauri command 解析后构建 `ScanOptions` 传入扫描器。服务层（skill_manager/plugin_manager）的 prepare_ 函数签名也增加 `scan_policy` 参数。

**Tech Stack:** React (SettingsPage.tsx), TypeScript (storage.ts, api.ts), Rust/Tauri (policy.rs, security.rs, plugins.rs, skill_manager.rs, plugin_manager.rs), i18next

---

### Task 1: 后端 — 新增 `ScanPolicy::from_name()` 工具方法

**Files:**
- Modify: `src-tauri/src/security/policy.rs` (在现有 `builtin_permissive()` 方法之后)

- [ ] **Step 1: 在 `ScanPolicy` impl 块中新增 `from_name` 方法**

在 `src-tauri/src/security/policy.rs` 中，找到现有的 `pub fn builtin_permissive() -> &'static ScanPolicy` 方法，在其后添加：

```rust
    /// 根据策略名称获取对应的内置策略
    ///
    /// 支持的名称: "default", "strict", "permissive"
    /// 返回 None 表示名称不合法
    pub fn from_name(name: &str) -> Option<&'static ScanPolicy> {
        match name {
            "default" => Some(Self::builtin_default()),
            "strict" => Some(Self::builtin_strict()),
            "permissive" => Some(Self::builtin_permissive()),
            _ => None,
        }
    }
```

- [ ] **Step 2: 在同文件 `tests` 模块（如无则在 `#[cfg(test)] mod tests` 块内）添加测试**

```rust
    #[test]
    fn test_from_name() {
        assert!(ScanPolicy::from_name("default").is_some());
        assert!(ScanPolicy::from_name("strict").is_some());
        assert!(ScanPolicy::from_name("permissive").is_some());
        assert!(ScanPolicy::from_name("unknown").is_none());
        assert!(ScanPolicy::from_name("").is_none());
    }
```

- [ ] **Step 3: 运行测试**

Run: `cd src-tauri && cargo test test_from_name -- --nocapture`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/security/policy.rs
git commit -m "feat(policy): add ScanPolicy::from_name() for string-based policy resolution"
```

---

### Task 2: 后端 — 扩展 `ScanOptions` 支持策略名

**Files:**
- Modify: `src-tauri/src/security/scanner.rs` (ScanOptions 结构体)

**设计决策：** 不在 `ScanOptions` 中加 `policy_name` 字段，而是保持现有 `policy: Option<ScanPolicy>` 不变。策略名解析在 command 层完成，传给 `ScanOptions` 的始终是已解析的 `ScanPolicy` 实例。这样服务层函数不需要改签名——只需在 command 层构建正确的 `ScanOptions` 即可。

但 `skill_manager.rs` 和 `plugin_manager.rs` 中的 prepare_ 函数是服务层直接调用的，它们内部构建了 `ScanOptions`。这些函数需要额外参数来知道用哪个策略。

**最终方案：** 新增一个辅助函数 `resolve_policy`，放在 commands 模块中供所有 command handler 复用：

- [ ] **Step 1: 在 `src-tauri/src/commands/mod.rs` 中新增辅助函数**

找到文件末尾已有的辅助函数区域（如 `clamp_scan_parallelism`），在其附近添加：

```rust
/// 将前端传入的策略名解析为 ScanPolicy，非法名称降级为 default
pub fn resolve_scan_policy(scan_policy: Option<&str>) -> crate::security::policy::ScanPolicy {
    scan_policy
        .and_then(|name| crate::security::policy::ScanPolicy::from_name(name))
        .cloned()
        .unwrap_or_else(|| crate::security::policy::ScanPolicy::builtin_default().clone())
}
```

- [ ] **Step 2: Commit**

```bash
git add src-tauri/src/commands/mod.rs
git commit -m "feat(commands): add resolve_scan_policy helper"
```

---

### Task 3: 后端 — 修改 `commands/security.rs` 中的扫描命令

**Files:**
- Modify: `src-tauri/src/commands/security.rs`

需要修改 3 个命令：`scan_all_installed_skills`、`scan_installed_skill`、`scan_skill_archive`。

- [ ] **Step 1: 修改 `scan_all_installed_skills` 签名和实现**

函数签名从：
```rust
pub async fn scan_all_installed_skills(
    state: State<'_, AppState>,
    locale: String,
    scan_parallelism: Option<usize>,
) -> Result<Vec<SkillScanResult>, String>
```
改为：
```rust
pub async fn scan_all_installed_skills(
    state: State<'_, AppState>,
    locale: String,
    scan_parallelism: Option<usize>,
    scan_policy: Option<String>,
) -> Result<Vec<SkillScanResult>, String>
```

在 `let locale = validate_locale(&locale);` 之后，添加策略解析：
```rust
    let policy = crate::commands::resolve_scan_policy(scan_policy.as_deref());
```

将内部闭包中的：
```rust
ScanOptions {
    skip_readme: true,
    ..Default::default()
},
```
替换为：
```rust
ScanOptions::with_policy(policy.clone()),
```

注意：`policy` 需要在 `pool.install()` 闭包中使用，需在闭包前 `let policy = policy;`（或让闭包捕获）。由于闭包已在 `pool.install(|| ...)` 中，需要把 policy 移入。在 `let mut results = pool.install(|| {` 之前加一行 `let policy = policy;` 即可。

- [ ] **Step 2: 修改 `scan_installed_skill` 签名和实现**

函数签名添加 `scan_policy: Option<String>` 参数：

```rust
pub async fn scan_installed_skill(
    state: State<'_, AppState>,
    app: AppHandle,
    skill_id: String,
    locale: String,
    scan_id: Option<String>,
    scan_policy: Option<String>,
) -> Result<SkillScanResult, String>
```

在 `let locale = validate_locale(&locale);` 之后添加：
```rust
    let policy = crate::commands::resolve_scan_policy(scan_policy.as_deref());
```

将两处 `ScanOptions { skip_readme: true, ..Default::default() }` 替换为 `ScanOptions::with_policy(policy.clone())`。

- [ ] **Step 3: 修改 `scan_skill_archive` 签名和实现**

函数签名添加 `scan_policy: Option<String>` 参数：

```rust
pub async fn scan_skill_archive(
    archive_path: String,
    locale: String,
    scan_policy: Option<String>,
) -> Result<SecurityReport, String>
```

将原来的：
```rust
let policy = crate::security::policy::ScanPolicy::builtin_default().clone();
```
替换为：
```rust
let policy = crate::commands::resolve_scan_policy(scan_policy.as_deref());
```

- [ ] **Step 4: 运行编译检查**

Run: `cd src-tauri && cargo check`
Expected: 编译通过，无错误

- [ ] **Step 5: 运行既有测试**

Run: `cd src-tauri && cargo test -- test_skill_archive_recalculates_score_after_archive_findings`
Expected: PASS（既有测试仍用 `scan_skill_archive` 直接调用，不传 `scan_policy`，应默认 `None` 降级为 default）

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands/security.rs
git commit -m "feat(security-commands): add scan_policy parameter to scan commands"
```

---

### Task 4: 后端 — 修改 `commands/plugins.rs` 中的扫描命令

**Files:**
- Modify: `src-tauri/src/commands/plugins.rs`

- [ ] **Step 1: 找到 `scan_all_installed_plugins` 命令，添加 `scan_policy: Option<String>` 参数**

在签名中增加参数，在函数体开头解析策略：
```rust
let policy = crate::commands::resolve_scan_policy(scan_policy.as_deref());
```
将内部的 `ScanOptions { skip_readme: true, ..Default::default() }` 替换为 `ScanOptions::with_policy(policy.clone())`。

- [ ] **Step 2: 找到 `scan_installed_plugin` 命令，同样添加 `scan_policy: Option<String>` 参数和策略解析**

将两处 `ScanOptions { skip_readme: true, ..Default::default() }` 替换为 `ScanOptions::with_policy(policy.clone())`。

- [ ] **Step 3: 运行编译检查**

Run: `cd src-tauri && cargo check`
Expected: 编译通过

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/plugins.rs
git commit -m "feat(plugin-commands): add scan_policy parameter to plugin scan commands"
```

---

### Task 5: 后端 — 修改安装/更新流程中的策略传递

**Files:**
- Modify: `src-tauri/src/commands/mod.rs`（`prepare_skill_installation` 和 `prepare_skill_update` 命令）
- Modify: `src-tauri/src/services/skill_manager.rs`（`prepare_skill_installation` 和 `prepare_skill_update` 服务方法）
- Modify: `src-tauri/src/commands/plugins.rs`（`prepare_plugin_installation` 命令）
- Modify: `src-tauri/src/services/plugin_manager.rs`（`prepare_plugin_installation` 服务方法）

这些 prepare_ 函数内部调用了扫描器，用户选择的策略也应该影响安装前检查。

- [ ] **Step 1: 修改 `commands/mod.rs` 中的 `prepare_skill_installation` 命令**

找到：
```rust
pub async fn prepare_skill_installation(
    state: State<'_, AppState>,
    skill_id: String,
    locale: String,
    allow_partial_scan: Option<bool>,
) -> Result<SecurityReport, String> {
```
添加参数 `scan_policy: Option<String>`。

在函数体内找到调用 `state.skill_manager.lock().await.prepare_skill_installation(...)` 的位置，传入策略：

```rust
let policy = crate::commands::resolve_scan_policy(scan_policy.as_deref());
```

然后需要看 `prepare_skill_installation` 在 `skill_manager.rs` 中的签名，给它也加上 `policy: ScanPolicy` 参数。

- [ ] **Step 2: 修改 `services/skill_manager.rs` 中的 `prepare_skill_installation` 方法签名**

找到 `pub async fn prepare_skill_installation` 方法，添加 `policy: crate::security::policy::ScanPolicy` 参数。将方法内部所有的 `ScanOptions { skip_readme: ..., ..Default::default() }` 替换为 `ScanOptions::with_policy(policy.clone())`。

（该方法内部可能有多处 ScanOptions，需要逐一替换。）

- [ ] **Step 3: 同样修改 `prepare_skill_update` 命令和服务方法**

在 `commands/mod.rs` 中给 `prepare_skill_update` 添加 `scan_policy: Option<String>` 参数。在 `skill_manager.rs` 中给对应的服务方法添加 `policy: ScanPolicy` 参数。

- [ ] **Step 4: 同样修改 `prepare_plugin_installation` 命令和服务方法**

在 `commands/plugins.rs` 中给 `prepare_plugin_installation` 添加 `scan_policy: Option<String>` 参数。在 `plugin_manager.rs` 中给对应的服务方法添加 `policy: ScanPolicy` 参数。

- [ ] **Step 5: 检查 skill_manager.rs 中其他使用 ScanOptions 的地方**

`skill_manager.rs` 有 7 处 `ScanOptions`。除了 `prepare_skill_installation` 和 `prepare_skill_update` 中的，其余可能位于批量安装等内部流程中。对于这些内部流程，使用 `ScanPolicy::builtin_default()` 作为默认策略（因为它们不直接受用户控制）。

- [ ] **Step 6: 运行编译检查**

Run: `cd src-tauri && cargo check`
Expected: 编译通过

- [ ] **Step 7: 运行全部后端测试**

Run: `cd src-tauri && cargo test`
Expected: 所有测试通过

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/commands/mod.rs src-tauri/src/services/skill_manager.rs src-tauri/src/commands/plugins.rs src-tauri/src/services/plugin_manager.rs
git commit -m "feat: thread scan_policy through prepare_ installation/update commands"
```

---

### Task 6: 前端 — 存储层和 i18n

**Files:**
- Modify: `src/lib/storage.ts`
- Modify: `src/i18n/locales/zh.json`
- Modify: `src/i18n/locales/en.json`

- [ ] **Step 1: 在 `src/lib/storage.ts` 末尾添加策略存取函数**

```typescript
const SCAN_POLICY_KEY = "asguard.preferences.scanPolicy.v1";
const DEFAULT_SCAN_POLICY = "default";

export function getScanPolicy(): string {
  try {
    const stored = localStorage.getItem(SCAN_POLICY_KEY);
    if (stored === "default" || stored === "strict" || stored === "permissive") {
      return stored;
    }
    return DEFAULT_SCAN_POLICY;
  } catch (error) {
    console.warn("Failed to read scan policy preference:", error);
    return DEFAULT_SCAN_POLICY;
  }
}

export function setScanPolicy(policy: string): void {
  try {
    localStorage.setItem(SCAN_POLICY_KEY, policy);
  } catch (error) {
    console.warn("Failed to save scan policy preference:", error);
  }
}
```

- [ ] **Step 2: 在 `src/i18n/locales/zh.json` 中添加翻译**

在 `settings.preferences.scanConcurrency` 对象之后、`"comingSoon"` 之前，添加：

```json
"scanPolicy": {
  "title": "扫描策略",
  "description": "控制安全扫描的严格程度。",
  "default": "默认",
  "strict": "严格",
  "permissive": "宽松"
}
```

- [ ] **Step 3: 在 `src/i18n/locales/en.json` 中添加翻译**

同样位置添加：

```json
"scanPolicy": {
  "title": "Scan Policy",
  "description": "Control the strictness of security scanning.",
  "default": "Default",
  "strict": "Strict",
  "permissive": "Permissive"
}
```

- [ ] **Step 4: Commit**

```bash
git add src/lib/storage.ts src/i18n/locales/zh.json src/i18n/locales/en.json
git commit -m "feat(frontend): add scan policy storage and i18n"
```

---

### Task 7: 前端 — 设置页 UI

**Files:**
- Modify: `src/components/SettingsPage.tsx`

- [ ] **Step 1: 添加 import**

在文件顶部的 storage import 中添加 `getScanPolicy` 和 `setScanPolicy`：

```typescript
import {
  getDefaultScanConcurrency,
  getMaxScanConcurrency,
  getPluginScanPromptEnabled,
  getScanConcurrency,
  setPluginScanPromptEnabled,
  setScanConcurrency,
  getScanPolicy,
  setScanPolicy,
} from "@/lib/storage";
```

在 lucide-react import 中添加 `Shield` 图标：

```typescript
import {
  // ...existing imports...
  Shield,
} from "lucide-react";
```

- [ ] **Step 2: 添加 state**

在 `SettingsPage` 组件内，在 `const [scanConcurrency, setScanConcurrencyState]` 之后添加：

```typescript
const [scanPolicy, setScanPolicyState] = useState(() => getScanPolicy());
```

在 `handleScanConcurrencyStep` 函数之后添加处理函数：

```typescript
const handleScanPolicyChange = (policy: string) => {
  setScanPolicy(policy);
  setScanPolicyState(policy);
};
```

- [ ] **Step 3: 添加策略选择器 UI**

在并发数 Stepper 的 `</GroupCardItem>` 之后、语言选择器的 `<GroupCardItem>` 之前，插入新的 GroupCardItem：

```tsx
<GroupCardItem>
  <div className="flex items-center justify-between">
    <div className="flex items-center gap-3">
      <div className="w-8 h-8 rounded-lg bg-amber-500 flex items-center justify-center">
        <Shield className="w-4 h-4 text-white" />
      </div>
      <div className="space-y-1">
        <div className="text-sm font-medium">{t("settings.preferences.scanPolicy.title")}</div>
        <div className="text-xs text-muted-foreground">
          {t("settings.preferences.scanPolicy.description")}
        </div>
      </div>
    </div>
    <div className="flex items-center gap-2">
      {(["default", "strict", "permissive"] as const).map((policy) => (
        <button
          key={policy}
          onClick={() => handleScanPolicyChange(policy)}
          className={`h-8 px-3 text-xs font-medium rounded-lg transition-all ${
            scanPolicy === policy
              ? "bg-blue-500 text-white"
              : "bg-secondary text-muted-foreground hover:bg-secondary/80"
          }`}
        >
          {t(`settings.preferences.scanPolicy.${policy}`)}
        </button>
      ))}
    </div>
  </div>
</GroupCardItem>
```

- [ ] **Step 4: 验证 UI**

Run: `npm run dev` 或相应开发命令，打开设置页。
Expected: 在并发数和语言选择之间看到策略选择器，三个按钮可切换，刷新后保持选择。

- [ ] **Step 5: Commit**

```bash
git add src/components/SettingsPage.tsx
git commit -m "feat(settings): add scan policy selector UI"
```

---

### Task 8: 前端 — API 调用层传递策略参数

**Files:**
- Modify: `src/lib/api.ts`
- Modify: `src/components/SecurityDashboard.tsx`
- Modify: `src/components/OverviewPage.tsx`

- [ ] **Step 1: 修改 `api.ts` 中的扫描方法，添加 `scanPolicy` 参数**

`scanInstalledSkill` 方法：
```typescript
async scanInstalledSkill(
  skillId: string,
  locale: string,
  scanId?: string,
  scanPolicy?: string
): Promise<SkillScanResult> {
  return invoke("scan_installed_skill", {
    skillId,
    locale,
    scanId: scanId || null,
    scanPolicy: scanPolicy || null,
  });
},
```

`scanAllInstalledPlugins` 方法：
```typescript
async scanAllInstalledPlugins(
  locale: string,
  claudeCommand?: string,
  scanParallelism?: number,
  scanPolicy?: string
): Promise<string[]> {
  return invoke("scan_all_installed_plugins", {
    locale,
    claudeCommand: claudeCommand || null,
    scanParallelism: scanParallelism ?? null,
    scanPolicy: scanPolicy || null,
  });
},
```

`prepareSkillInstallation` 方法：
```typescript
async prepareSkillInstallation(
  skillId: string,
  locale: string,
  allowPartialScan = false,
  scanPolicy?: string
): Promise<SecurityReport> {
  return invoke("prepare_skill_installation", {
    skillId,
    locale,
    allowPartialScan,
    scanPolicy: scanPolicy || null,
  });
},
```

`prepareSkillUpdate` 方法：
```typescript
async prepareSkillUpdate(
  skillId: string,
  locale: string,
  scanPolicy?: string
): Promise<[SecurityReport, string[]]> {
  return invoke("prepare_skill_update", { skillId, locale, scanPolicy: scanPolicy || null });
},
```

`preparePluginInstallation` 方法：
```typescript
async preparePluginInstallation(
  pluginId: string,
  locale: string,
  scanPolicy?: string
): Promise<SecurityReport> {
  return invoke("prepare_plugin_installation", {
    pluginId,
    locale,
    scanPolicy: scanPolicy || null,
  });
},
```

- [ ] **Step 2: 修改 `SecurityDashboard.tsx` 中的 `handleScan`**

在文件顶部添加 import：
```typescript
import { getScanPolicy } from "@/lib/storage";
```

修改 `handleScan` 函数中的 invoke 调用：
```typescript
const scanConcurrency = getScanConcurrency();
const scanPolicy = getScanPolicy();
const results = await invoke<SkillScanResult[]>("scan_all_installed_skills", {
  locale: i18n.language,
  scanParallelism: scanConcurrency,
  scanPolicy,
});
```

- [ ] **Step 3: 修改 `OverviewPage.tsx` 中的扫描调用**

找到 `api.scanInstalledSkill(skill.id, i18n.language)` 调用，改为：
```typescript
const result = await api.scanInstalledSkill(skill.id, i18n.language, undefined, getScanPolicy());
```

在文件顶部确保 import 了 `getScanPolicy`（可能已有 `getScanConcurrency` import，在同一个模块里）。

- [ ] **Step 4: 搜索其他调用 `prepareSkillInstallation`、`prepareSkillUpdate`、`preparePluginInstallation` 的前端代码，补传 `scanPolicy`**

使用 grep 搜索：
```
grep -r "prepareSkillInstallation\|prepareSkillUpdate\|preparePluginInstallation" src/
```

对每个调用点，传入 `getScanPolicy()` 作为最后一个参数。

- [ ] **Step 5: 编译检查**

Run: `npm run build` 或 `npx tsc --noEmit`
Expected: 无类型错误

- [ ] **Step 6: Commit**

```bash
git add src/lib/api.ts src/components/SecurityDashboard.tsx src/components/OverviewPage.tsx
git commit -m "feat(frontend): pass scan policy to all scan API calls"
```

---

### Task 9: 端到端验证

- [ ] **Step 1: 启动开发服务器**

Run: `cargo tauri dev`

- [ ] **Step 2: 验证设置页**

1. 打开设置页
2. 确认在「并发数」和「语言」之间有「扫描策略」选择器
3. 切换到「严格」，刷新页面，确认保持选择
4. 切换到「默认」，刷新页面，确认恢复

- [ ] **Step 3: 验证策略生效**

1. 设置策略为「严格」
2. 扫描一个已安装的 skill
3. 确认扫描结果中出现 default 策略不会产生的结构校验类 finding（如 `STRICT_STRUCTURE_*`）
4. 设置策略为「默认」重新扫描同一 skill
5. 确认结构校验类 finding 不再出现

- [ ] **Step 4: 验证中英文切换**

1. 切换语言为 English，确认策略选择器文字正确
2. 切换回中文，确认文字正确

- [ ] **Step 5: Final commit (if any fixes needed)**

```bash
git add -A
git commit -m "fix: address e2e verification issues"
```
