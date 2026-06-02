//! 引用文件提取模块
//!
//! 从 Skill 内容（SKILL.md 或其他文件）中提取被引用的文件路径。
//! 支持 6 种提取模式：Markdown 链接、自然语言引用、执行型引用、
//! @reference 指令、include/import/load 指令、Python import。

use std::collections::BTreeSet;
use std::path::Path;

use lazy_static::lazy_static;
use regex::Regex;

// ── 正则表达式 ──

lazy_static! {
    /// 模式 1: Markdown 链接 [text](path)
    static ref MD_LINK_RE: Regex =
        Regex::new(r#"\[.*?\]\(([^)]+)\)"#).unwrap();

    /// 模式 2: 自然语言引用 (see|refer to|check|read) `path.ext`
    static ref NATURAL_LANG_RE: Regex =
        Regex::new(r#"(?i)(?:see|refer to|check|read)\s+[`"']?(\S+\.\w+)[`"']?"#).unwrap();

    /// 模式 3: 执行型引用 (run|execute|invoke) scripts/...
    static ref EXEC_REF_RE: Regex =
        Regex::new(r#"(?i)(?:run|execute|invoke)\s+(scripts/\S+)"#).unwrap();

    /// 模式 4: @reference: 指令
    static ref AT_REFERENCE_RE: Regex =
        Regex::new(r#"@reference:\s*(.+)"#).unwrap();

    /// 模式 5: include/import/load: 指令
    static ref INCLUDE_RE: Regex =
        Regex::new(r#"(?i)(?:include|import|load):\s*(.+)"#).unwrap();

    /// 模式 6: Python import (from X import Y / import Y)
    static ref PYTHON_IMPORT_RE: Regex =
        Regex::new(r#"(?m)^(?:from\s+(\S+)\s+)?import\s+(\S+)"#).unwrap();
}

// ── 常量 ──

/// Python 标准库模块列表（约 200 个）
/// 来源：Python 3.12 标准库模块索引
#[allow(clippy::too_many_lines)]
const STDLIB_MODULES: &[&str] = &[
    // 基础内置
    "__future__", "_thread", "abc", "aifc", "argparse", "array",
    // a
    "ast", "asynchat", "asyncio", "asyncore", "atexit",
    // b
    "base64", "bdb", "binascii", "binhex", "bisect", "builtins",
    // c
    "calendar", "cgi", "cgitb", "chunk", "cmath", "cmd", "code",
    "codecs", "codeop", "collections", "colorsys", "compileall",
    "concurrent", "configparser", "contextlib", "contextvars", "copy",
    "copyreg", "cProfile", "crypt", "csv", "ctypes", "curses",
    // d
    "dataclasses", "datetime", "dbm", "decimal", "difflib", "dis",
    "distutils", "doctest",
    // e
    "email", "encodings", "enum", "errno", "faulthandler", "fcntl",
    "filecmp", "fileinput", "fnmatch", "fractions", "ftplib",
    // f
    "functools",
    // g
    "gc", "getopt", "getpass", "gettext", "glob", "grp", "gzip",
    // h
    "hashlib", "heapq", "hmac", "html", "http", "idlelib",
    // i
    "imaplib", "imghdr", "imp", "importlib", "inspect", "io",
    "ipaddress", "itertools",
    // j
    "json",
    // k
    "keyword",
    // l
    "lib2to3", "linecache", "locale", "logging", "lzma",
    // m
    "mailbox", "mailcap", "marshal", "math", "mimetypes", "mmap",
    "modulefinder", "multiprocessing",
    // n
    "netrc", "nis", "nntplib", "numbers",
    // o
    "operator", "optparse", "os", "ossaudiodev",
    // p
    "pathlib", "pdb", "pickle", "pickletools", "pipes", "pkgutil",
    "platform", "plistlib", "poplib", "posix", "posixpath", "pprint",
    "profile", "pstats", "pty", "pwd", "py_compile", "pyclbr", "pydoc",
    // q
    "queue", "quopri",
    // r
    "random", "re", "readline", "reprlib", "resource", "rlcompleter", "runpy",
    // s
    "sched", "secrets", "select", "selectors", "shelve", "shlex",
    "shutil", "signal", "site", "smtpd", "smtplib", "sndhdr",
    "socket", "socketserver", "sqlite3", "ssl", "stat", "statistics",
    "string", "stringprep", "struct", "subprocess", "sunau", "symtable",
    "sys", "sysconfig", "syslog",
    // t
    "tabnanny", "tarfile", "telnetlib", "tempfile", "termios", "test",
    "textwrap", "threading", "time", "timeit", "tkinter", "token",
    "tokenize", "tomllib", "trace", "traceback", "tracemalloc", "tty",
    "turtle", "turtledemo", "types", "typing",
    // u
    "unicodedata", "unittest", "urllib",
    // v
    "venv",
    // w
    "warnings", "wave", "weakref", "webbrowser", "winreg", "winsound", "wsgiref",
    // x
    "xdrlib", "xml", "xmlrpc",
    // z
    "zipapp", "zipfile", "zipimport", "zlib",
    // 子模块（只在 from X import Y 中出现时匹配顶层包名即可，
    // 但部分场景下直接 import 子模块名，也一并排除）
    "typing_extensions",
];

/// 常见第三方 Python 包列表（约 100 个）
/// PyPI 下载量排名靠前的包
#[allow(clippy::too_many_lines)]
const KNOWN_THIRD_PARTY: &[&str] = &[
    // 核心工具链
    "pip", "setuptools", "wheel", "virtualenv", "tox", "nox",
    // Web 框架
    "flask", "django", "fastapi", "starlette", "bottle", "tornado",
    "sanic", "quart", "aiohttp", "falcon", "pyramid",
    // HTTP / 网络
    "requests", "httpx", "urllib3", "httplib2", "pycurl",
    "websocket", "paramiko", "fabric",
    // 数据科学
    "numpy", "pandas", "scipy", "polars", "dask", "xarray",
    "statsmodels", "sympy",
    // 可视化
    "matplotlib", "seaborn", "plotly", "bokeh", "altair",
    // 机器学习 / AI
    "torch", "tensorflow", "keras", "scikit_learn", "sklearn",
    "transformers", "huggingface_hub", "openai", "anthropic",
    "langchain", "llamaindex", "tiktoken",
    // 数据库
    "sqlalchemy", "psycopg2", "pymysql", "aiomysql", "asyncpg",
    "motor", "pymongo", "redis", "aioredis", "peewee",
    "databases", "tortoise", "dynamodb",
    // ORM / 迁移
    "alembic", "pony", "sqlmodel",
    // 测试
    "pytest", "nose", "hypothesis", "faker",
    "factory_boy", "responses", "vcrpy",
    // 代码质量
    "flake8", "pylint", "mypy", "black", "isort", "ruff",
    "bandit", "safety", "pyflakes", "pycodestyle",
    // 序列化
    "pydantic", "marshmallow", "cerberus", "voluptuous",
    "dataclasses_json", "attrs", "cattrs",
    // CLI
    "click", "typer", "argcomplete",
    // 日志 / 监控
    "structlog", "loguru", "sentry_sdk", "datadog",
    // 环境 / 配置
    "python_dotenv", "pydantic_settings", "dynaconf",
    // 文件 / 系统
    "watchdog", "pyinotify", "psutil",
    "pathlib2", "pyfakefs",
    // 加密 / 安全
    "cryptography", "pyjwt", "jose", "passlib", "bcrypt",
    "cffi", "pyopenssl",
    // 邮件
    "email_validator", "yagmail",
    // 图像处理
    "pillow", "opencv_python", "imageio",
    // 任务队列
    "celery", "rq", "dramatiq", "huey", "arq",
    // 其他
    "rich", "colorama", "tqdm", "tabulate", "prettytable",
    "jinja2", "mako", "chardet", "python_dateutil", "pytz",
    "dateutil", "deprecated",
];

// ── 路径校验 ──

/// URL 前缀（这些路径应被排除）
const URL_PREFIXES: &[&str] = &["http://", "https://", "ftp://"];

/// 判断提取到的路径是否为有效的本地文件引用
///
/// 排除条件：
/// - URL（http/https/ftp）
/// - 锚点链接（#）
/// - 绝对路径（/）
/// - 路径穿越（..）
/// - 空字符串
pub fn is_valid_file_ref(path: &str) -> bool {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return false;
    }
    // 排除 URL
    for prefix in URL_PREFIXES {
        if trimmed.starts_with(prefix) {
            return false;
        }
    }
    // 排除锚点
    if trimmed.starts_with('#') {
        return false;
    }
    // 排除绝对路径
    if trimmed.starts_with('/') {
        return false;
    }
    // 排除路径穿越
    if trimmed.contains("..") {
        return false;
    }
    true
}

// ── 主提取函数 ──

/// 从 Skill 内容中提取所有引用的文件路径
///
/// 按 6 种模式提取，合并、排序、去重后返回。
///
/// # 参数
/// - `content`: 文件内容文本
/// - `skill_dir`: Skill 根目录（用于 Python import 本地模块检测），None 则跳过本地模块检测
///
/// # 返回
/// 去重排序后的相对路径列表
pub fn extract_references(content: &str, skill_dir: Option<&Path>) -> Vec<String> {
    let mut results = BTreeSet::new();

    // 模式 1: Markdown 链接
    for cap in MD_LINK_RE.captures_iter(content) {
        let path = cap[1].trim();
        if is_valid_file_ref(path) {
            results.insert(path.to_string());
        }
    }

    // 模式 2: 自然语言引用
    for cap in NATURAL_LANG_RE.captures_iter(content) {
        let path = cap[1].trim();
        if is_valid_file_ref(path) {
            results.insert(path.to_string());
        }
    }

    // 模式 3: 执行型引用
    for cap in EXEC_REF_RE.captures_iter(content) {
        let path = cap[1].trim();
        if is_valid_file_ref(path) {
            results.insert(path.to_string());
        }
    }

    // 模式 4: @reference: 指令
    for cap in AT_REFERENCE_RE.captures_iter(content) {
        let path = cap[1].trim();
        if is_valid_file_ref(path) {
            results.insert(path.to_string());
        }
    }

    // 模式 5: include/import/load: 指令
    for cap in INCLUDE_RE.captures_iter(content) {
        let path = cap[1].trim();
        if is_valid_file_ref(path) {
            results.insert(path.to_string());
        }
    }

    // 模式 6: Python import
    for cap in PYTHON_IMPORT_RE.captures_iter(content) {
        let module_name = if let Some(pkg) = cap.get(1) {
            // from X import Y -> 取 X（顶层包名）
            pkg.as_str().split('.').next().unwrap_or(pkg.as_str())
        } else {
            // import Y -> 取 Y（顶层包名）
            cap[2].split('.').next().unwrap_or(&cap[2])
        };

        // 排除标准库
        if is_stdlib_module(module_name) {
            continue;
        }

        // 排除已知第三方包
        if is_known_third_party(module_name) {
            continue;
        }

        // 本地模块检测：检查 skill_dir 中是否存在同名 .py 文件
        if let Some(dir) = skill_dir {
            let py_file = dir.join(format!("{module_name}.py"));
            if py_file.exists() {
                results.insert(format!("{module_name}.py"));
            }
            // 也检查子目录形式的模块（如 mymodule/__init__.py）
            let pkg_init = dir.join(module_name).join("__init__.py");
            if pkg_init.exists() {
                results.insert(format!("{module_name}/__init__.py"));
            }
        }
    }

    results.into_iter().collect()
}

// ── 辅助函数 ──

/// 判断模块是否为 Python 标准库模块
fn is_stdlib_module(name: &str) -> bool {
    STDLIB_MODULES.iter().any(|&m| m == name)
}

/// 判断模块是否为已知第三方包
fn is_known_third_party(name: &str) -> bool {
    KNOWN_THIRD_PARTY.iter().any(|&p| p == name)
}

// ── 测试 ──

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_markdown_links() {
        let content = r#"
# Skill Instructions

Please see [config.json](config.json) and [run script](scripts/run.sh).
Also check [docs](references/guide.md).
"#;
        let refs = extract_references(content, None);
        assert!(refs.contains(&"config.json".to_string()));
        assert!(refs.contains(&"scripts/run.sh".to_string()));
        assert!(refs.contains(&"references/guide.md".to_string()));
    }

    #[test]
    fn test_markdown_links_exclude_urls() {
        let content = r#"
# Links

[GitHub](https://github.com/example/repo)
[Docs](http://docs.example.com)
[FTP](ftp://files.example.com/data)
[Anchor](#section)
[Absolute](/absolute/path/to/file)
"#;
        let refs = extract_references(content, None);
        assert!(
            refs.is_empty(),
            "URLs, anchors, and absolute paths should be excluded, got: {:?}",
            refs
        );
    }

    #[test]
    fn test_natural_language_reference() {
        let content = r#"
# Instructions

See config.yaml for details.
Refer to scripts/helper.py for the helper function.
Check `data.json` for the data format.
Read 'references/api.md' for API docs.
"#;
        let refs = extract_references(content, None);
        assert!(refs.contains(&"config.yaml".to_string()));
        assert!(refs.contains(&"scripts/helper.py".to_string()));
        assert!(refs.contains(&"data.json".to_string()));
        assert!(refs.contains(&"references/api.md".to_string()));
    }

    #[test]
    fn test_execution_reference() {
        let content = r#"
# Steps

Run scripts/build.sh to build the project.
Execute scripts/deploy.py --prod for deployment.
Invoke scripts/test_all.sh to run tests.
"#;
        let refs = extract_references(content, None);
        assert!(refs.contains(&"scripts/build.sh".to_string()));
        assert!(refs.contains(&"scripts/deploy.py".to_string()));
        assert!(refs.contains(&"scripts/test_all.sh".to_string()));
    }

    #[test]
    fn test_at_reference() {
        let content = r#"
# Skill Body

@reference: helpers/utils.py
@reference: data/defaults.json
@reference: templates/main.html
"#;
        let refs = extract_references(content, None);
        assert!(refs.contains(&"helpers/utils.py".to_string()));
        assert!(refs.contains(&"data/defaults.json".to_string()));
        assert!(refs.contains(&"templates/main.html".to_string()));
    }

    #[test]
    fn test_include_directive() {
        let content = r#"
# Directives

include: config/defaults.yaml
import: schemas/user.json
load: templates/base.html
"#;
        let refs = extract_references(content, None);
        assert!(refs.contains(&"config/defaults.yaml".to_string()));
        assert!(refs.contains(&"schemas/user.json".to_string()));
        assert!(refs.contains(&"templates/base.html".to_string()));
    }

    #[test]
    fn test_python_import_stdlib_excluded() {
        let content = "\
```python
import os
import sys
import json
from pathlib import Path
from collections import defaultdict
```
";
        let refs = extract_references(content, None);
        assert!(
            refs.is_empty(),
            "stdlib modules should be excluded, got: {:?}",
            refs
        );
    }

    #[test]
    fn test_python_import_local_module() {
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path();

        // 创建本地模块文件
        fs::write(dir_path.join("my_helper.py"), "# helper code").unwrap();
        fs::create_dir(dir_path.join("my_package")).unwrap();
        fs::write(dir_path.join("my_package").join("__init__.py"), "# package init").unwrap();

        let content = "\
```python
import my_helper
from my_package import something
import os
```
";
        let refs = extract_references(content, Some(dir_path));
        assert!(refs.contains(&"my_helper.py".to_string()));
        assert!(refs.contains(&"my_package/__init__.py".to_string()));
        // os 是 stdlib，不应出现
        assert!(!refs.iter().any(|r| r == "os"));
    }

    #[test]
    fn test_path_traversal_excluded() {
        let content = r#"
[bad](../etc/passwd)
[also bad](../../secret.json)
@reference: ../../../outside/skill/file.txt
"#;
        let refs = extract_references(content, None);
        assert!(
            refs.is_empty(),
            "path traversal should be excluded, got: {:?}",
            refs
        );
    }

    #[test]
    fn test_is_valid_file_ref_edge_cases() {
        assert!(!is_valid_file_ref(""));
        assert!(!is_valid_file_ref("   "));
        assert!(!is_valid_file_ref("http://example.com/file.txt"));
        assert!(!is_valid_file_ref("https://example.com/file.txt"));
        assert!(!is_valid_file_ref("ftp://example.com/file.txt"));
        assert!(!is_valid_file_ref("#anchor"));
        assert!(!is_valid_file_ref("/absolute/path"));
        assert!(!is_valid_file_ref("../secret"));
        assert!(!is_valid_file_ref("a/../b"));
        assert!(is_valid_file_ref("config.json"));
        assert!(is_valid_file_ref("scripts/run.sh"));
        assert!(is_valid_file_ref("path/to/file.py"));
    }

    #[test]
    fn test_results_are_sorted_and_deduplicated() {
        let content = r#"
[second](scripts/build.sh)
[third](config.json)
[first](config.json)
"#;
        let refs = extract_references(content, None);
        assert_eq!(refs.len(), 2);
        // BTreeSet 保证排序
        assert_eq!(refs[0], "config.json");
        assert_eq!(refs[1], "scripts/build.sh");
    }

    #[test]
    fn test_mixed_patterns() {
        let content = r#"
# Multi-pattern Skill

[helper](helpers/utils.py)
See scripts/build.sh for build instructions.
@reference: config/defaults.yaml
include: data/schema.json
Run scripts/deploy.sh to deploy.
import os
import my_local_module
"#;
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("my_local_module.py"), "# module").unwrap();

        let refs = extract_references(content, Some(dir.path()));
        assert!(refs.contains(&"helpers/utils.py".to_string()));
        assert!(refs.contains(&"scripts/build.sh".to_string()));
        assert!(refs.contains(&"config/defaults.yaml".to_string()));
        assert!(refs.contains(&"data/schema.json".to_string()));
        assert!(refs.contains(&"scripts/deploy.sh".to_string()));
        assert!(refs.contains(&"my_local_module.py".to_string()));
        // os 是 stdlib
        assert!(!refs.iter().any(|r| r == "os"));
    }

    #[test]
    fn test_third_party_excluded() {
        let content = "\
```python
import numpy
import pandas
import requests
from flask import Flask
```
";
        let refs = extract_references(content, None);
        assert!(
            refs.is_empty(),
            "third-party modules should be excluded, got: {:?}",
            refs
        );
    }

    #[test]
    fn test_empty_content() {
        let refs = extract_references("", None);
        assert!(refs.is_empty());
    }

    #[test]
    fn test_no_skill_dir_skips_local_detection() {
        let content = "import unknown_module";
        let refs = extract_references(content, None);
        // 没有 skill_dir，无法检测本地模块，所以不提取
        assert!(refs.is_empty());
    }

    #[test]
    fn test_unknown_module_without_dir_not_extracted() {
        let content = "import unknown_module";
        let refs = extract_references(content, None);
        assert!(
            refs.is_empty(),
            "unknown module without skill_dir should not be extracted"
        );
    }
}
