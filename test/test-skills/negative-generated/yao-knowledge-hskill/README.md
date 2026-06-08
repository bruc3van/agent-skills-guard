# yao-knowledge-hskill

知识图谱助手 - 帮助团队构建结构化的领域知识体系。

## 功能

- 概念定义和术语管理
- 关系网络建立
- 知识查询和导航
- 知识体系演进

## 使用场景

- 技术文档体系构建
- 产品知识库整理
- 领域术语标准化
- 新人 onboarding 材料

## 快速开始

```
帮我定义 "微服务架构" 这个概念，包括：
- 精确定义
- 与单体架构的关系
- 核心组成部分
- 适用场景
```

## 目录结构

```
yao-knowledge-hskill/
├── SKILL.md
├── agents/
│   └── interface.yaml
├── concepts/
│   ├── ai-agent.yaml
│   └── template.md
├── references/
│   └── modeling-methodology.md
└── manifest.json
```

## 概念模板

```yaml
concept:
  name: "概念名称"
  aliases: ["别名"]
  definition: "定义"
  domain: "领域"
  relations:
    is_a: "父概念"
    related_to: ["相关概念"]
```

## License

MIT

