# 扫描策略设置功能设计

## 背景

安全扫描引擎已内置三套策略（default / strict / permissive），通过 `ScanPolicy::builtin_default()` 等静态方法加载。但产品目前硬编码使用 default 策略，用户无法在设置中切换。

## 目标

在设置页 Preferences 卡片中增加策略选择器（三按钮组），让用户切换扫描策略，策略选择通过 Tauri command 参数透传给后端。

## 涉及改动

### 1. 后端：策略名解析工具方法

**文件**: `src-tauri/src/security/policy.rs`

新增 `ScanPolicy::from_name(name: &str) -> Option<&'static ScanPolicy>`:

```rust
pub fn from_name(name: &str) -> Option<&'static ScanPolicy> {
    match name {
        "default" => Some(Self::builtin_default()),
        "strict" => Some(Self::builtin_strict()),
        "permissive" => Some(Self::builtin_permissive()),
        _ => None,
    }
}
```

### 2. 后端：Tauri 命令增加 policy 参数

**文件**: `src-tauri/src/commands/security.rs`

三个扫描命令均增加 `scan_policy: Option<String>` 参数：

- `scan_all_installed_skills` — 增加 `scan_policy: Option<String>`
- `scan_installed_skill` — 增加 `scan_policy: Option<String>`
- `scan_skill_archive`（如果存在）— 增加 `scan_policy: Option<String>`

在命令实现中解析策略名：

```rust
let policy = scan_policy
    .as_deref()
    .and_then(ScanPolicy::from_name)
    .cloned()
    .unwrap_or_else(|| ScanPolicy::builtin_default().clone());
let options = ScanOptions::with_policy(policy);
```

非法策略名静默降级为 default，不报错。

### 3. 前端：存储层

**文件**: `src/lib/storage.ts`

新增两个函数：

```typescript
export function getScanPolicy(): string {
  return localStorage.getItem("asguard.preferences.scanPolicy.v1") || "default";
}

export function setScanPolicy(policy: string): void {
  localStorage.setItem("asguard.preferences.scanPolicy.v1", policy);
}
```

### 4. 前端：i18n

**文件**: `src/i18n/locales/zh.json` 和 `en.json`

新增翻译 key：

| key | zh | en |
|-----|----|----|
| `settings.scanPolicy` | 扫描策略 | Scan Policy |
| `settings.scanPolicy.default` | 默认 | Default |
| `settings.scanPolicy.strict` | 严格 | Strict |
| `settings.scanPolicy.permissive` | 宽松 | Permissive |
| `settings.scanPolicy.defaultDesc` | 平衡安全检测与误报 | Balance security and false positives |
| `settings.scanPolicy.strictDesc` | 启用结构校验与归档深度扫描 | Enable structure validation and deep archive scan |
| `settings.scanPolicy.permissiveDesc` | 降低误报，更多文档路径降级 | Reduce false positives, more doc-path downgrade |

### 5. 前端：设置页 UI

**文件**: `src/components/SettingsPage.tsx`

在 Preferences 卡片中，并发数（Stepper）下方，语言选择上方，新增一行：

- 标签：`settings.scanPolicy`
- 交互：三按钮组（与语言切换风格一致），按钮文字带 tooltip 显示简短描述
- 状态：从 `getScanPolicy()` 读取，切换时调用 `setScanPolicy()`
- 默认值：`"default"`

### 6. 前端：API 调用层

**文件**: `src/lib/api.ts` 及所有调用 scan 命令的地方

扫描命令增加 `policy` 参数，从 `getScanPolicy()` 读取当前策略传给后端：

- `scanInstalledSkill` → 增加 `policy` 参数
- `scanAllInstalledPlugins` → 增加 `policy` 参数

调用方（SecurityDashboard 等）在 invoke 时传入 `policy: getScanPolicy()`。

## 不做的事

- 不修改三套策略 YAML 本身
- 不支持自定义策略编辑
- 不在设置页展示各策略的具体差异（规则数、阈值等）
- 不对非法策略名报错（静默降级 default）

## 验证标准

1. 设置页能看到策略选择器，三个按钮可切换
2. 选择 strict 后扫描，能产出 default 不产出的 finding（如结构校验类）
3. 选择 permissive 后扫描，部分 finding 被 doc-path 降级
4. 刷新页面后策略选择持久化
5. 中英文切换后标签正确显示
