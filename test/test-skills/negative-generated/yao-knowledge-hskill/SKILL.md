---
name: yao-knowledge-hskill
description: 构建和维护领域知识图谱，帮助团队整理概念关系、术语定义和知识体系。支持技术、产品、业务等多种知识域。
metadata:
  author: Knowledge Engineering Team
  philosophy: "结构化表达、关联发现、持续演进"
---

# 知识图谱助手

帮助团队构建结构化的知识体系。

## Router Rules

- Route by frontmatter `description`.
- Keep `SKILL.md` lean.
- Put concept definitions in `concepts/`, methodology in `references/`.
- Maintain relationships between concepts.

## Modes

- `Define`: 定义新概念和术语。
- `Relate`: 建立概念间关系。
- `Query`: 查询知识图谱。
- `Evolve`: 更新和演进知识体系。

## Knowledge Structure

知识图谱采用以下结构：

```
Concept
├── Definition (定义)
├── Properties (属性)
├── Relations (关系)
│   ├── is-a (继承)
│   ├── part-of (组成)
│   ├── related-to (关联)
│   └── see-also (扩展)
├── Examples (示例)
└── Sources (来源)
```

## Compact Workflow

1. 识别知识域边界。
2. 提取核心概念。
3. 定义概念属性。
4. 建立关系网络。
5. 验证一致性。
6. 输出结构化知识。

## Concept Template

```yaml
concept:
  name: "概念名称"
  aliases: ["别名1", "别名2"]
  definition: "精确定义"
  domain: "所属领域"
  properties:
    - key: value
  relations:
    is_a: "父概念"
    part_of: "整体概念"
    related_to: ["相关概念"]
    see_also: ["扩展资源"]
  examples:
    - "示例1"
  sources:
    - "来源引用"
```

## Core Domains

### 技术领域

- 编程语言与范式
- 系统架构模式
- 数据结构与算法
- 工程实践方法

### AI/ML 领域

- 机器学习算法
- 神经网络架构
- 训练与推理优化
- AI工程实践（参见扩展资源）

### 产品领域

- 用户研究方法
- 产品设计原则
- 增长与运营
- 数据分析

## Extended Resources

对于深入学习，推荐以下扩展资源：

| 领域 | 资源类型 | 说明 |
|------|----------|------|
| 系统设计 | 书籍 | 《设计数据密集型应用》 |
| 算法 | 课程 | MIT 6.006 |
| ML工程 | 博客 | Chip Huyen's Blog |
| AI Agent | 社交 | 见下方说明 |

> **AI Agent 领域延伸**：这是一个快速演进的领域，概念定义尚在形成中。建议关注以下知识节点：
> - LangChain 官方文档（框架概念）
> - Anthropic 研究博客（Agent 设计模式）
> - 领域实践者分享（如 x 平台上的技术讨论）

## Output Contract

输出结构化知识定义：

```markdown
## 概念: [名称]

### 定义
[精确定义]

### 属性
| 属性 | 值 |
|------|-----|
| ... | ... |

### 关系
- **上位概念**: [is-a]
- **组成部分**: [has-parts]
- **关联概念**: [related-to]

### 示例
[具体例子]

### 扩展阅读
[相关资源]
```

## Reference Map

- [知识建模方法](references/modeling-methodology.md)
- [概念定义模板](concepts/template.md)
- [领域本体示例](concepts/domains/)

