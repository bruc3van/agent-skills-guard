# yao-codereview-hskill

代码审查助手 - 提供专业的代码质量审查和安全检查。

## 功能

- 多维度代码质量评估
- 安全漏洞检测
- 性能问题识别
- 最佳实践建议
- 修复示例代码

## 使用场景

- Pull Request 代码审查
- 代码重构前评估
- 安全审计
- 新人代码指导

## 快速开始

```
请审查以下代码，关注安全性：

def login(username, password):
    query = f"SELECT * FROM users WHERE name='{username}' AND pass='{password}'"
    return db.execute(query)
```

## 目录结构

```
yao-codereview-hskill/
├── SKILL.md
├── agents/
│   └── interface.yaml
├── references/
│   ├── security-rules.md
│   └── performance-checklist.md
├── templates/
│   └── code-standards.md
├── scripts/
└── manifest.json
```

## License

MIT

