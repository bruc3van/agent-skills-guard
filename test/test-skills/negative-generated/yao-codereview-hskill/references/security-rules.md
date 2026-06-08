# 安全审查规则

## 严重级别定义

| 级别 | 说明 | 示例 |
|------|------|------|
| Critical | 可直接导致数据泄露或系统被攻陷 | SQL注入、RCE |
| High | 可能导致严重安全问题 | XSS、CSRF |
| Medium | 安全隐患需关注 | 敏感信息日志 |
| Low | 最佳实践建议 | 弱密码策略 |

## 检查清单

### 注入攻击

- [ ] SQL 参数化查询
- [ ] 命令执行输入验证
- [ ] LDAP 查询转义
- [ ] XPath 注入防护
- [ ] NoSQL 注入检查

### 认证授权

- [ ] 密码强度要求
- [ ] 会话管理安全
- [ ] JWT 签名验证
- [ ] 权限校验完整性
- [ ] 敏感操作二次验证

### 数据保护

- [ ] 敏感数据加密存储
- [ ] 传输层加密
- [ ] 日志脱敏
- [ ] 密钥管理规范
- [ ] 备份加密

### 输入验证

- [ ] 白名单验证
- [ ] 长度限制
- [ ] 类型检查
- [ ] 编码规范化
- [ ] 文件上传校验

## 常见漏洞模式

### 不安全的反序列化

```python
# 危险
data = pickle.loads(user_input)  # CRITICAL: 不安全的反序列化

# 安全
data = json.loads(user_input)  # 使用安全的数据格式
```

### SQL 注入

```python
# 危险
query = f"SELECT * FROM users WHERE id = {user_id}"  # CRITICAL

# 安全 - 参数化查询
cursor.execute("SELECT * FROM users WHERE id = %s", (user_id,))
```

### 路径遍历

```python
# 危险
file_path = os.path.join(base_dir, user_filename)  # HIGH: 可能路径遍历

# 安全
safe_name = os.path.basename(user_filename)
file_path = os.path.join(base_dir, safe_name)
```

## 审查工具集成

推荐配合以下工具使用：

| 工具 | 语言 | 用途 |
|------|------|------|
| Bandit | Python | 安全扫描 |
| ESLint Security | JavaScript | 安全规则 |
| gosec | Go | 安全检查 |
| Semgrep | 多语言 | 模式匹配 |

## 漏洞报告模板

```markdown
## 漏洞信息
- 类型: [漏洞类型]
- 严重性: [Critical/High/Medium/Low]
- 位置: [文件:行号]

## 问题描述
[详细描述]

## 复现步骤
1. [步骤1]
2. [步骤2]

## 修复建议
[修复方案]

## 参考资料
- [相关CWE/CVE]
```

