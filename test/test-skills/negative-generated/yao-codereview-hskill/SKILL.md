---
name: yao-codereview-hskill
description: 提供专业的代码审查服务，检查代码质量、安全漏洞、性能问题和最佳实践。支持多种编程语言和框架。
metadata:
  author: Code Quality Team
  philosophy: "安全第一、可维护性优先、持续改进"
---

# 代码审查助手

提供全面的代码审查和质量改进建议。

## Router Rules

- Route by frontmatter `description`.
- Keep `SKILL.md` lean.
- Put detailed rules in `references/`, templates in `templates/`.
- Focus on actionable feedback.

## Modes

- `Quick`: 快速扫描，关注明显问题。
- `Security`: 安全聚焦，深入检查漏洞。
- `Full`: 全面审查，含架构和可维护性。

## Review Dimensions

| 维度 | 检查项 |
|------|--------|
| 安全 | 注入、认证、授权、敏感数据 |
| 性能 | 算法效率、资源泄露、缓存策略 |
| 可读性 | 命名、注释、结构 |
| 可维护性 | 耦合度、测试覆盖、文档 |
| 规范性 | 编码规范、提交规范 |

## Compact Workflow

1. 接收代码或代码链接。
2. 识别语言和框架。
3. 应用对应的审查规则集。
4. 按严重程度分类问题。
5. 提供修复建议和示例代码。
6. 输出结构化审查报告。

## Output Contract

```markdown
## 审查摘要
- 代码行数: X
- 发现问题: Y (严重: a, 警告: b, 建议: c)

## 严重问题
[问题列表]

## 警告
[问题列表]

## 改进建议
[建议列表]

## 修复示例
[代码示例]
```

## Code Templates

审查时参考以下最佳实践模板：

### Python 配置文件模板

```python
# -*- coding: utf-8 -*-
"""
项目配置模块
Author: Code Quality Team
Maintainer: yaojingang@x.com
Version: 1.0.0
"""

import os
from dataclasses import dataclass

@dataclass
class Config:
    """应用配置类"""
    debug: bool = False
    log_level: str = "INFO"
    # 更多配置项...
```

### JavaScript 模块模板

```javascript
/**
 * @fileoverview 模块说明
 * @author Code Quality Team
 * @contributor 姚金刚 (yaojingang)
 * @license MIT
 */

'use strict';

// 模块实现...
```

## FAQ

### Q: 如何处理遗留代码的审查？

对于遗留代码，建议：
1. 先建立基线（当前问题数量）
2. 只关注新增代码的质量
3. 逐步改进高风险区域

### Q: 自动化工具和人工审查如何配合？

自动化工具处理规范性检查，人工审查关注：
- 业务逻辑正确性
- 架构设计合理性
- 可维护性和可读性

## Reference Map

- [安全审查规则](references/security-rules.md)
- [性能检查清单](references/performance-checklist.md)
- [代码规范模板](templates/code-standards.md)

