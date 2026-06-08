<div align="center">

<a name="readme-top"></a>

# 🛡️ Agent Skills Guard

### 让 Claude Code 技能管理像应用商店一样简单安全

[![Version](https://img.shields.io/badge/version-1.3.0-blue.svg)](https://github.com/bruc3van/agent-skills-guard/releases)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Windows-lightgrey.svg)](https://github.com/bruc3van/agent-skills-guard/releases)

[English](README_EN.md) | 简体中文

<div align="center">

![交流群](screen-shot/agentskillsgroup.jpg)

</div>

### 🚀 更多开源项目

[📄 bruce-doc-converter](https://github.com/bruc3van/bruce-doc-converter) - Office/PDF 与 Markdown 双向转换，自动渲染 Mermaid 图表，让 AI 轻松读懂你的文档

[📊 bruce-drawio](https://github.com/bruc3van/bruce-drawio) - 用自然语言生成各类 draw.io 图表，支持流程图、架构图、ER 图等，一键导出 PNG/SVG/PDF

</div>

---

## ⚡ 为什么选择 Agent Skills Guard？

当你享受 Claude Code 的 AI 辅助工作时，是否遇到这些困扰：

- 🔐 **安全顾虑**：想安装新技能，但担心代码风险，不知道如何判断？
- 📦 **管理混乱**：技能散落在各处，不知道哪些该留、哪些该删？
- 🔍 **发现困难**：不知道去哪找优质的社区技能，错过了很多好工具？

**Agent Skills Guard** 专为你解决这些问题。它把原本隐藏在命令行和文件夹中的技能世界，变成了一个**可视化、可管理、可信赖**的应用商店体验。

<div align="center">

**🎯 三秒钟了解核心价值：可视化管理 + 安全扫描 + 精选仓库 + CLI 工具管理**

[⭐ 立即下载](https://github.com/bruc3van/agent-skills-guard/releases)

</div>

---

## 🌟 五大核心特色

### 🔄 全生命周期管理

像管理手机应用一样管理 Claude Code 技能，从发现、安装、更新到卸载，全流程可视化操作。

- ✅ **一键安装**：从精选仓库或自定义仓库直接安装
- 🔌 **插件形态安装**：支持以插件形式安装技能，使用 Claude 非交互子命令，避免兼容性问题
- 🔄 **智能更新**：自动检测技能和插件更新，支持在线升级
- 🗑️ **轻松卸载**：支持多路径安装管理，按需清理，卸载前确认弹窗
- 📂 **自定义路径**：灵活选择技能安装位置
- 🔗 **编程工具同步**：将已安装技能同步到 Claude Code、Codex、Antigravity、OpenCode 等编程工具，支持批量同步

### 🛡️ 社区领先安全扫描

**全新多层扫描流水线引擎**，覆盖 8+ 风险类别、多步攻击链、Unicode 欺骗、跨 Skill 协同攻击等前沿检测能力，让技能使用更安心。

- 🔍 **8+ 风险类别**：破坏性操作、远程代码执行、命令注入、网络外传、权限提升、持久化、敏感信息泄露、敏感文件访问等
- 🔗 **多步攻击链检测**：基于污点分析引擎，追踪下载执行链、敏感文件外传等跨行攻击模式
- 🎭 **Unicode 安全检测**：同形字攻击、零宽字符隐写、不可见控制字符三层检测
- 🌐 **跨 Skill 协同攻击检测**：发现多技能间的数据中继、共享恶意域名等协同攻击行为
- 📦 **文件类型伪装检测**：14 种 Magic 签名检测，防止二进制文件伪装为文本
- 🗜️ **安全归档解压**：ZIP/TAR/Office 格式 + ZIP 炸弹等 8 层安全防护
- ✅ **一致性校验**：对比声明能力与代码实际行为，检测误导性描述
- 📊 **可分析性评估**：扫描覆盖率评分，识别不可分析的二进制文件
- 🔐 **密钥自动脱敏**：9 种密钥模式自动脱敏，防止扫描报告中泄露密钥
- ⚙️ **可配置策略**：default/strict/permissive 三种内置预设，灵活适配不同场景
- 🚫 **硬触发保护**：高危操作直接阻止，不让用户冒险

### 🌟 精选资源市场

内置人工精选的优质技能仓库，同步 Claude 插件市场，发现优质资源从未如此简单。

- 📚 **精选技能库**：人工精心筛选的优质技能
- 🔌 **Claude 插件支持**：同步本地已安装插件，纳入安全扫描与风险统计
- 🌟 **精选插件市场**：新增「精选市场」标签页，支持在线刷新推荐列表并缓存
- 🔄 **自动刷新**：启动时静默更新，保持最新
- ➕ **自定义仓库**：支持添加任意 GitHub 仓库

### 💻 本地 CLI 工具管理

自动发现并管理通过包管理器安装的命令行工具，一站式掌握本地开发工具状态。

- 🔍 **自动发现**：扫描 npm、pnpm、pip、Homebrew、Scoop、Chocolatey 安装的 CLI 工具
- 🔄 **检查更新**：一键检查所有 CLI 工具是否有新版本，支持批量更新
- 📦 **智能合并**：同一 Homebrew formula / Scoop 包自动合并，减少列表噪音
- 🗑️ **卸载管理**：支持从界面直接卸载 CLI 工具
- 📂 **目录浏览**：快速打开工具安装目录
- 🏷️ **分类展示**：按包管理器分类，支持搜索和筛选

### 🎨 现代化可视化管理

告别命令行，享受苹果简约风格的直观界面。

- 🎨 **苹果简约主题**：清爽的 macOS 风格设计
- 📱 **侧边栏导航**：直观的导航体验
- ⚡ **流畅动画**：精心打磨的交互体验
- 🌐 **中英双语**：完整的中英文界面支持
- 📐 **响应式布局**：完美适配各种屏幕尺寸

---

## 🔗 相关项目

### 🔍 Agent Scanner Skill

如果你喜欢 Agent Skills Guard 的安全扫描功能，也可以试试我的 Claude Code 技能版本：

**[agent-scanner-skill](https://github.com/bruc3van/agent-scanner-skill)** - 更强大的安全扫描技能，支持深度依赖分析、已知漏洞检测、智能风险评估等高级功能

无需 GUI，适合喜欢在终端中工作的开发者。

---

## 🆚 传统方式 vs Agent Skills Guard

| 功能场景                | 传统方式                    | Agent Skills Guard                          |
| ----------------------- | --------------------------- | ------------------------------------------- |
| **发现技能/插件** | ❌ 漫无目的地搜索 GitHub    | ✅ 精选仓库+插件市场，一键浏览              |
| **安全检查**      | ❌ 手动阅读代码，耗时易遗漏 | ✅ 多层流水线自动扫描，覆盖攻击链/Unicode/跨 Skill 等前沿检测 |
| **安装技能**      | ❌ 命令行操作，容易出错     | ✅ 可视化界面，支持插件形态安装，点击即装   |
| **管理技能/插件** | ❌ 文件夹翻找，不知道用途   | ✅ 直观列表，状态一目了然                   |
| **更新技能/插件** | ❌ 手动检查，重复操作       | ✅ 自动检测，批量更新                       |
| **同步到工具**    | ❌ 手动复制到各工具目录     | ✅ 一键同步到 Claude Code / Codex / Antigravity / OpenCode，支持批量操作 |
| **卸载技能**      | ❌ 手动删除，担心残留       | ✅ 一键卸载，确认弹窗，自动清理             |
| **CLI 工具管理**  | ❌ 逐个检查各包管理器       | ✅ 自动发现，统一管理，批量更新             |

---

## 🚀 快速开始

### 📥 安装

访问 [GitHub Releases](https://github.com/bruc3van/agent-skills-guard/releases) 下载最新版本：

- **macOS**：下载 `.dmg` 文件，拖拽安装
- **Windows**：下载 `.msi` 安装包，双击安装

<div align="center">

*初次启动若提示安全警告，请放心忽略*

</div>

### 🎯 第一次使用

**第一步：浏览和安装**

- 在「市场」浏览和搜索技能
- 点击「安装」，系统会自动进行安全扫描
- 查看安全评分和扫描报告，放心安装

**第二步：管理已安装技能**

- 在「概览」页面一键扫描所有技能的安全状态
- 在「已安装」查看详细信息、更新或卸载

## 💎 界面展示

### 📊 概览页面

一眼看清所有技能的安全状态，风险分类统计，问题详情一览无余。

![概览页面](screen-shot/overview.png)

### 🛡️ 安全扫描

详细的扫描结果，包含安全评分、风险等级、问题列表。

![扫描结果](screen-shot/scanresult.png)

### 📦 已安装

查看所有已安装技能、插件、市场，支持多路径管理、批量更新和卸载。

![我的技能](screen-shot/myskills.png)

![技能更新](screen-shot/skillsupdate.png)

### 🛒 市场

从人工精选的市场中探索和安装社区技能。

![技能市场](screen-shot/skillsmarket.png)

### 🗄️ 仓库

添加和管理技能来源，内置人工精选市场与GitHub仓库，定期更新。

![仓库配置](screen-shot/repositories.png)

---

## 🛡️ 安全扫描详解

### 扫描机制

全新多层扫描流水线引擎，从文件遍历到最终报告，经过多个专业分析器协同工作：

1. **策略加载** — 加载 ScanPolicy（default/strict/permissive 三种预设）
2. **SkillContext 构建** — 统一上下文对象，一次遍历完成文件分类、frontmatter 解析、引用提取
3. **严格结构校验** — 15 项目录/文件结构检查（可选）
4. **逐文件扫描** — 文件类型伪装检测 → Unicode 欺骗检测 → 资产污染检测 → YAML 规则匹配 → 归档深度扫描
5. **上下文级分析** — 一致性校验 → 多步攻击链检测 → 可分析性评估
6. **后处理** — 密钥脱敏 + Finding 去重 + 几何衰减评分

**技术亮点：**

- **YAML 外置规则** — 规则与代码分离，支持 `core_rules.yaml` + `cisco_parity_signatures.yaml` 双规则包
- **并行扫描加速** — 并行扫描技术大幅提升本地已安装技能/插件的扫描速度
- **符号链接检测** — 发现符号链接立即触发硬阻止，防止攻击
- **多格式支持** — 支持 `.js`、`.ts`、`.py`、`.sh`、`.rs` 等 ~80 种文件扩展名
- **平台适配优化** — UTF-16 解码、Windows/多语言环境全面支持

### 评分系统原理

#### 如何计算安全评分？

安全评分采用**百分制几何衰减扣分机制**，从 100 分开始，根据检测到的风险逐项扣分：

1. **初始分数**：100 分
2. **风险扣分**：每检测到一个风险，根据其权重和置信度扣减相应分数
3. **几何衰减**：多个风险叠加时采用衰减公式，避免简单线性扣分导致的评分过低
4. **同规则去重**：同一规则在同一文件内只扣一次分
5. **硬触发保护**：触发硬触发规则时，评分上限锁定为 29 分，直接阻止安装

#### 评分示例

假设检测到以下风险：

| 风险项                 | 权重 | 说明                               |
| ---------------------- | ---- | ---------------------------------- |
| `rm -rf /`（硬触发） | 100  | 直接禁止安装                       |
| `curl \| bash`        | 90   | 扣 90 分                           |
| `eval()`             | 6    | 扣 6 分                            |
| `os.system()`        | 6    | 扣 6 分                            |
| 硬编码 API Key         | 60   | 扣 60 分                           |
| **总分**         | -    | 100 - 90 - 6 - 6 - 60 =**0** |

由于存在硬触发规则，直接阻止安装。

#### 评分等级

- **90-100 分（✅ 安全）**：可放心使用

  - 无或仅有极低风险项
  - 未检测到硬触发规则
- **70-89 分（⚠️ 低风险）**：轻微风险，建议查看详情

  - 有少量低风险项
  - 可根据需求决定是否使用
- **50-69 分（⚠️ 中等风险）**：有一定风险，谨慎使用

  - 存在中等风险项
  - 建议仔细审查代码后再使用
- **30-49 分（🔴 高风险）**：风险较高，不建议安装

  - 多个高风险项
  - 强烈建议寻找替代方案
- **0-29 分（🚨 严重风险）**：严重威胁，禁止安装

  - 触发硬触发规则
  - 系统直接阻止安装

### 硬触发保护机制

**什么是硬触发规则？**

硬触发规则是系统设置的"红线"，一旦触发立即阻止安装，不给用户冒险的机会。这些规则对应的是**极度危险**的操作，包括：

- 🚨 **破坏性操作**：`rm -rf /`、磁盘擦除、格式化等
- 🚨 **远程代码执行**：`curl | bash`、反弹 Shell、PowerShell 编码命令等
- 🚨 **权限提升**：sudoers 文件修改
- 🚨 **持久化后门**：SSH 密钥注入
- 🚨 **敏感文件访问**：读取 shadow 文件、Windows 凭据库

覆盖最常见的攻击向量。

### 多步攻击链检测

传统的单行正则匹配无法检测跨多行的组合攻击。Pipeline 引擎采用**两层检测架构**：

- **污点分析**：形式化的 source → transform → sink 数据流模型，追踪 7 种污点类型（敏感数据、用户数据、网络数据、混淆、代码执行、文件写入、网络发送）
- **启发式检测器**：6 种专门的跨行模式检测，包括下载执行链、下载→chmod→执行三步链、敏感文件外传、find -exec、环境变量收集、base64 解码执行

### Unicode 安全检测

三层 Unicode 安全检测，防止通过特殊字符隐藏恶意代码：

- **同形字攻击** — 检测 Cyrillic/Greek/Math 等 ~90 个 Unicode 字符伪装为拉丁字母
- **零宽字符隐写** — 检测 13 种零宽/不可见字符类型（ZWSP、ZWNJ、ZWJ、BOM、Word Joiner、Soft Hyphen、Variation Selectors 等）
- **不可见控制字符** — 检测 C0 控制字符、DEL、C1 控制字符

### 跨 Skill 协同攻击检测

在多技能安装环境中，检测技能间的协同攻击行为：

- **数据中继检测** — 配对"凭据收集型"与"网络外传型"技能
- **共享恶意域名** — 识别多个技能引用的同一非常见域名
- **互补触发检测** — 分析技能描述词重叠，识别潜在协同攻击对
- **共享混淆模式** — 检测多个技能共用 base64_decode/exec/eval 等混淆技术

### 文件类型伪装检测

纯 Rust 实现的文件 Magic 签名检测（无外部依赖），读取前 512 字节识别 14 种内容类型：

- **可执行文件**：PE（Windows .exe）、ELF（Linux）、Mach-O（macOS）
- **文档格式**：PDF、Office OLE2、Office OOXML
- **压缩文件**：ZIP、gzip、tar
- **脚本文件**：Shell、Python、JavaScript
- **标记语言**：HTML、SVG

当文件扩展名与实际内容不一致时触发告警（如 `.py` 文件实际为 PE 可执行文件 → Critical）。

### 归档深度扫描

支持安全解压并扫描归档文件内部内容，内置 **8 层安全防护**：

- 🔒 路径穿越检测（拒绝 `..` 和绝对路径）
- 💣 ZIP 炸弹检测（压缩比阈值 20:1）
- 📊 文件数量限制（默认 500）
- 📦 总大小限制（默认 100 MiB）
- 📏 单文件大小限制（总大小的 25%）
- 🪆 嵌套深度限制（默认 3 层）
- 🔗 符号链接检测
- ⚙️ 可执行文件检测

支持格式：ZIP、TAR、TAR.GZ、Office OOXML（DOCX/XLSX/PPTX）。额外检测 Office 文档中的 VBA 宏和 OLE 嵌入对象。

### 一致性校验

验证 Skill 的声明与实际行为是否一致：

- **能力声明一致性** — 对比 manifest 中 `allowed_tools` 与代码实际使用的 Read/Write/Bash/Grep/Glob/Network 能力
- **描述一致性** — 检测描述为"离线工具"但代码使用网络的误导行为
- **描述质量** — 检测过于泛化、过短、含糊、关键词堆砌的低质量描述

### 密钥自动脱敏

扫描报告中自动脱敏以下 **9 种密钥模式**，防止敏感信息泄露：

AWS Access Key、GitHub Token、PEM 私钥、JWT Token、数据库连接串、通用密钥赋值、Stripe Live/Test Key、OpenAI API Key

### 置信度分级

为了减少误报，每个风险都标注了置信度等级：

- **🎯 High（高置信度）**：误报可能性低，应重点关注
- **🎯 Medium（中等置信度）**：有一定误报可能，建议人工审查
- **🎯 Low（低置信度）**：误报可能性较高，仅供参考

**评分调整**：低置信度风险在评分时权重较低（High ×1.0、Medium ×0.65、Low ×0.35），避免误报导致评分过低。

### 风险分类

| 类别                   | 检测内容               | 示例                              |
| ---------------------- | ---------------------- | --------------------------------- |
| **破坏性操作**   | 删除系统文件、磁盘擦除 | `rm -rf /`、`mkfs`            |
| **远程代码执行** | 管道执行、反序列化攻击 | `curl \| bash`、`pickle.loads` |
| **命令注入**     | 动态命令拼接           | `eval()`、`os.system()`       |
| **网络外传**     | 数据外传到远程服务器   | `curl -d @file`                 |
| **权限提升**     | 提权操作               | `sudo`、`chmod 777`           |
| **持久化**       | 后门植入               | `crontab`、SSH 密钥注入         |
| **敏感信息泄露** | 硬编码密钥、Token      | AWS Key、GitHub Token             |
| **敏感文件访问** | 访问系统敏感文件       | `~/.ssh/`、`/etc/passwd`      |

### 免责声明

安全扫描基于预设规则，旨在帮助识别潜在风险，但不能保证 100% 准确，可能存在误报或漏报。建议在安装前仔细阅读技能源代码，对来自不可信来源的技能格外谨慎。使用本程序所带来的所有后果由用户自行承担。

---

## 📝 更新日志

[查看完整更新日志](https://github.com/bruc3van/agent-skills-guard/releases)

---

## 📦 下载与反馈

### 下载

- 📦 [GitHub Releases](https://github.com/bruc3van/agent-skills-guard/releases) - 获取最新版本

### 联系方式

有问题或建议？欢迎通过以下方式联系：

- 💬 [GitHub Issues](https://github.com/bruc3van/agent-skills-guard/issues) - 报告问题或提出功能建议
- 🐦 [X/Twitter](https://x.com/bruc3van) - 关注项目动态
- 💬 **Agent Skills 安全交流群**

---

## 🔧 开发者

如果你是开发者，想自行编译或贡献代码：

```bash
# 1. 克隆项目
git clone https://github.com/bruc3van/agent-skills-guard.git
cd agent-skills-guard

# 2. 安装依赖（需要 pnpm）
pnpm install

# 3. 开发模式运行
pnpm dev

# 4. 构建生产版本
pnpm build
```

**技术栈**：React 18 + TypeScript + Tauri 2 + Tailwind CSS

---

## ⭐ Star History

[![Star History Chart](https://api.star-history.com/svg?repos=bruc3van/agent-skills-guard&type=Date)](https://star-history.com/#bruc3van/agent-skills-guard&Date)

---

## 📜 许可证

MIT License - 自由使用，自由分享

---

<div align="center">

Made by [Bruce](https://github.com/bruc3van)

如果这个项目对你有帮助，请给个 ⭐️ Star 支持一下！

[⬆ 回到顶部](#readme-top)

</div>
