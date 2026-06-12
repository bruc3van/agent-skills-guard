# Agent Skills Guard 代码库全面审查综合报告

> 审查日期: 2026-06-12
> 审查范围: 9 个维度的全面审查
> 审查方法: 子代理驱动开发

---

## 执行摘要

本次对 Agent Skills Guard 项目进行了全面、系统性的代码审查，覆盖架构设计、安全扫描引擎、数据库层、前端代码、错误处理、国际化、测试覆盖、性能优化、构建部署等 9 个维度。

### 整体评价

项目整体质量**良好**，体现了成熟的 Tauri + React 桌面应用开发经验。安全扫描引擎是项目的亮点，多层检测架构设计精巧。主要改进方向集中在 CI/CD 流程完善、前端测试覆盖提升、以及部分安全和性能优化。

### 问题统计

| 严重程度 | 数量 | 说明 |
|---------|------|------|
| Critical | 1 | 需要立即修复的安全或功能问题 |
| High | 23 | 重要的架构或功能问题 |
| Medium | 49 | 代码质量和可维护性问题 |
| Low | 30 | 改进建议和优化空间 |
| **总计** | **103** | |

---

## Critical 级别问题（1 个）

### 1. Memory Leak in LocalCliPage
- **位置**: `src/components/LocalCliPage.tsx:73-124`
- **影响**: useEffect 中的递归调用可能导致内存泄漏
- **建议**: 使用 AbortController 或更健壮的取消机制

---

## High 级别问题（23 个）

### 安全相关（4 个）
1. Pipeline 分析器的已知安装器域名匹配过于宽松
2. Pipeline 多步攻击检测的行间距窗口有限
3. 跨行正则存在潜在 ReDoS 风险
4. subprocess_call_uses_shell_true 仅检查 12 行窗口

### 架构相关（1 个）
5. Hook 的 onSuccess 中重复 invalidate 相同的 query keys

### 数据库相关（2 个）
6. reset_all_data 中外键关闭后未在异常路径恢复
7. initialize_schema 中迁移方法无事务包装

### 前端相关（6 个）
8. InstalledSkillsPage (2586 行) 完全没有测试
9. MarketplacePage (1266 行) 完全没有测试
10. Excessive Re-renders in OverviewPage
11. Missing Error Boundaries in Critical Paths
12. Type Safety Issues in api.ts
13. Hardcoded Chinese Strings

### 错误处理（4 个）
14. No error boundaries around individual page components
15. ErrorBoundary default fallback is not user-friendly
16. Backend errors lack structured error codes
17. Inconsistent error handling in React Query hooks

### 国际化（4 个）
18. ToolSyncDialog 组件大量硬编码中文字符串
19. InstalledSkillsPage 多处硬编码中文字符串
20. ToolIcons 组件大量硬编码中文字符串
21. InstallPathSelector 硬编码中文字符串

### 性能相关（2 个）
22. 数据库连接使用 std::Mutex 串行化所有操作
23. scan_local_skills 中对每个技能目录执行完整安全扫描

---

## Medium 级别问题（49 个）

### 安全相关（6 个）
1. Unicode 隐写检测的密度阈值可能不够敏感
2. Homoglyph 检测仅覆盖西里尔和希腊字母
3. File Magic 检测仅检查前 512 字节
4. is_doc_path 的路径段匹配可能存在边缘误匹配
5. 策略配置缺少完整性校验
6. check_magic 在无扩展名文件上返回 None

### 架构相关（3 个）
7. useLocalCli hooks 中的错误处理直接在 hook 内显示 toast
8. featured_repositories_cache_path 直接使用 std::fs
9. 后端错误处理使用 String 类型

### 数据库相关（3 个）
10. delete_skill 不在一个事务中执行两条 DELETE
11. 日期时间存储为 TEXT 格式
12. get_skills() 全表扫描无 LIMIT

### 前端相关（6 个）
13. Query Client Created Outside Provider
14. LocalStorage Access Without try-catch
15. Inconsistent Query Key Design
16. Missing Debounce on Search Inputs
17. Complex State Management in InstalledSkillsPage
18. Inline Function Definitions in JSX

### 错误处理（7 个）
19. Hardcoded Chinese strings bypassing i18n
20. Error boundary does not report errors externally
21. No global error handler for unhandled promise rejections
22. translateError only handles one error code per message
23. getPlugins silently swallows errors
24. Error toast duration is fixed at 3000ms
25. ErrorBoundary reset does not re-trigger failed operations

### 国际化（4 个）
26. common.cancel 和 common.confirm 翻译键缺失
27. utils.ts 硬编码中文字符串
28. 后端 Rust 代码大量硬编码中文错误消息
29. LanguageSwitcher title 属性硬编码

### 测试相关（7 个）
30. SettingsPage (615 行) 没有测试
31. plugins.rs 命令层没有测试
32. security.rs 命令层没有测试
33. security-dashboard 和 overview 页面没有测试
34. usePlugins hook 没有测试
35. database.rs 仅 6 个测试
36. plugin_manager.rs 仅 9 个测试

### 性能相关（6 个）
37. 网络请求无重试机制
38. get_skills 每次调用全量反序列化安全报告 JSON
39. std::fs::canonicalize 在多处同步阻塞调用
40. SkillManager 锁持有时间过长
41. auto_scan_unscanned_repositories 串行扫描
42. 下载文件无大小预检查时的双重检查

### 构建部署（7 个）
43. Vite build sourcemap 被禁用
44. CSS 未做 Code Split
45. CI 使用 tauri-apps/tauri-action@v0
46. CI 中 pnpm 版本未显式指定
47. Rust unsafe 代码多处使用 set_var
48. Windows 签名配置为空
49. CSS 使用 unsafe-inline

---

## Low 级别问题（30 个）

包括但不限于：
- SkillInstallation struct 定义了但未见使用
- 前端 api.ts 缺少错误类型标注
- 安全扫描的 FindingKind 分类映射函数过于冗长
- SecurityScanner 是无状态结构体
- count_scan_files 和 scan_directory_with_options 中的二进制检测逻辑重复
- lazy_static 正则表达式编译失败时会 panic
- build_scan_lines 的字符串拼接续行可能产生意外匹配
- 前端 useInstalledSkills 的 staleTime 为 0
- LocalCliPage 通过 display:none 常驻挂载
- 未配置 .npmrc 或 .rustfmt.toml
- 前端依赖版本范围过宽
- Rust 内部函数缺少文档
- 错误码文档缺失
- 等等

---

## 项目优势

1. **安全扫描引擎设计精巧** - 多层检测架构、taint-based 分析、策略驱动配置
2. **TypeScript 类型安全** - 前后端都有完善的类型定义
3. **国际化支持完善** - 中英双语，翻译文件结构对称
4. **测试覆盖后端优秀** - Rust 后端 451 个测试函数，安全模块覆盖率达 93%
5. **Apple 风格 UI 设计** - 一致的组件库和设计系统
6. **原子性操作保证** - 数据库事务、文件原子写入
7. **优雅的错误降级** - 多处 graceful degradation 设计

---

## 总结

Agent Skills Guard 是一个设计良好的桌面应用项目，核心安全扫描功能达到了专业水准。主要改进方向集中在：

1. **安全性**: 收紧权限配置，修复扫描引擎绕过风险
2. **质量保障**: 完善 CI/CD 流程，补充测试覆盖
3. **用户体验**: 统一国际化，改善错误处理
4. **性能优化**: 流式下载，数据库 WAL，并发扫描

---
