[English](README_EN.md) | 中文

# Draw.io 图表生成技能

跨平台的 draw.io (diagrams.net) 图表生成技能，适用于 AI 编程 Agent（Claude Code、OpenClaw 等）。Agent 直接生成 drawio XML，经过自检后通过 CLI 导出图片。

## 安装

向你的 AI 编程 Agent 发送以下提示词：

```
帮我安装这个skill：https://github.com/bruc3van/bruce-drawio
```

Agent 会自动克隆仓库并完成配置。

## 支持的图表类型

| 类型       | 说明                         | 触发词示例         |
| ---------- | ---------------------------- | ------------------ |
| 流程图     | 业务流程、审批流程、算法逻辑 | "画一个流程图"     |
| 架构图     | 系统架构、微服务、部署架构   | "画一个架构图"     |
| UML 时序图 | 组件之间的交互时序           | "画一个时序图"     |
| UML 类图   | 类关系、继承结构             | "画一个类图"       |
| ER 图      | 数据库设计、实体关系         | "画一个 ER 图"     |
| 思维导图   | 头脑风暴、知识梳理           | "画一个思维导图"   |
| 网络拓扑图 | 网络架构、设备连接           | "画一个网络拓扑图" |

## 平台支持

| 平台    | 安装命令                       | 包管理器            |
| ------- | ------------------------------ | ------------------- |
| macOS   | `brew install --cask drawio` | Homebrew            |
| Windows | `winget install JGraph.Draw` | winget / Chocolatey |
| Linux   | `snap install drawio`        | snap / 手动安装     |

所有平台也支持从 [draw.io releases](https://github.com/jgraph/drawio-desktop/releases) 手动下载安装。

## 工作流程

1. 用户描述想要的图表
2. Agent 判断图表类型和关键元素
3. Agent 直接生成完整的 drawio XML
4. 自检清单验证正确性和布局
5. 保存 `.drawio` 文件
6. 通过 CLI 导出为 PNG/SVG/PDF
7. 向用户展示图片并提供可编辑的源文件

## 导出格式

| 格式 | 参数       | 适用场景           |
| ---- | ---------- | ------------------ |
| PNG  | `-f png` | 默认格式，通用性强 |
| SVG  | `-f svg` | 可缩放矢量图       |
| PDF  | `-f pdf` | 打印 / 嵌入文档    |

使用 `--scale 2` 可导出高清 PNG。

## 项目结构

```
bruce-drawio/
  SKILL.md                      # 主技能文档（工作流程 + 规则）
  skill.json                    # 技能元数据
  references/
    best-practices.md           # XML 模板、样式、布局规则
    examples.md                 # 完整的可用 XML 示例
  evals/
    evals.json                  # 测试用例
```

## 架构图风格

架构图默认采用**分层块状布局**风格：

- 灰色背景底板
- 左侧标签列标注每一层（如"场景层"、"应用层"）
- 蓝色半透明层容器，内含子分组
- 白色叶子节点，灰色边框
- 可选的右侧跨层侧边栏（如安全、监控等横切关注点）
- 纯块状图，不使用箭头连线，通过空间嵌套表达层次关系

## 依赖检测

无需额外脚本。Agent 按照 SKILL.md 中的步骤通过 shell 命令（`which drawio`、检查默认安装路径）自动检测 draw.io。如果未安装，会引导用户安装。

## 使用示例

安装完成后，用自然语言描述你想要的图表即可，例如：

- "画一个电商下单流程图"
- "画一个微服务架构图"
- "画一个用户注册的时序图"
- "画一个博客系统的 ER 图"
- "画一个 AI Agent 的思维导图"
