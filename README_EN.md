<div align="center">

<a name="readme-top"></a>

# 🛡️ Agent Skills Guard

### Making Claude Code Skills Management as Simple and Secure as an App Store

[![Version](https://img.shields.io/badge/version-1.0.1-blue.svg)](https://github.com/brucevanfdm/agent-skills-guard/releases)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Windows-lightgrey.svg)](https://github.com/brucevanfdm/agent-skills-guard/releases)

English | [简体中文](README.md)

</div>

---

## ⚡ Why Agent Skills Guard?

When enjoying Claude Code's AI-assisted programming, do you face these frustrations:

- 🔐 **Security concerns**: Want to install new skills but worried about code risks, don't know how to judge?
- 📦 **Management chaos**: Skills scattered everywhere, don't know which to keep or delete?
- 🔍 **Discovery difficulties**: Don't know where to find quality community skills, missing many great tools?

**Agent Skills Guard** is designed to solve these problems. It transforms the skills world originally hidden in command lines and folders into a **visible, manageable, trustworthy** app store experience.

<div align="center">

**🎯 Core value in three seconds: Visual management + Security scanning + Featured repositories**

[⭐ Download Now](https://github.com/brucevanfdm/agent-skills-guard/releases)

</div>

---

## 🌟 Four Core Features

### 🔄 Full Lifecycle Management

Manage Claude Code skills like managing mobile apps, from discovery, installation, updates to uninstallation, all with visual operations.

- ✅ **One-click install**: Install directly from featured or custom repositories
- 🔌 **Plugin-style installation**: Support installing skills as plugins using Claude non-interactive commands, avoiding compatibility issues
- 🔄 **Smart updates**: Automatically detect skill and plugin updates, support online upgrades
- 🗑️ **Easy uninstall**: Support multi-path installation management, clean on demand
- 📂 **Custom paths**: Flexibly choose skill installation locations

### 🛡️ Community-Leading Security Scanning

**Covering 8 major risk categories with 22 hard-trigger protections**, making skill use more secure.

- 🔍 **8 risk categories**: Destructive operations, remote code execution, command injection, data exfiltration, privilege escalation, persistence, sensitive information leakage, sensitive file access
- 🚫 **22 hard-trigger rules**: Directly block high-risk operations, no risk taking
- 🔗 **Symbolic link detection**: Prevent symlink attacks
- ⚡ **Parallel scanning acceleration**: Parallel scanning technology greatly improves detection speed
- 📊 **Security scoring system**: 0-100 score intuitive display
- 📝 **Detailed scan reports**: Tell you where the risk is, why it's risky, how to fix it
- 🎯 **Confidence grading**: High/Medium/Low three-level confidence, reduce false positives

### 🌟 Featured Resource Marketplace

Built-in manually curated quality skills repository, syncs with Claude plugin marketplace, discovering quality resources has never been easier.

- 📚 **Featured skills library**: Manually selected quality skills
- 🔌 **Claude plugin support**: Sync local installed plugins, include in security scanning and risk statistics
- 🌟 **Featured plugin marketplace**: New "Featured Marketplace" tab, supports online refresh and caching
- 🔄 **Auto refresh**: Silent update on startup, keep latest
- ➕ **Custom repositories**: Support adding any GitHub repository

### 🎨 Modern Visual Management

Say goodbye to command lines and enjoy the intuitive Apple minimalist interface.

- 🎨 **Apple minimalist theme**: Clean macOS style design
- 📱 **Sidebar navigation**: Intuitive navigation experience
- ⚡ **Smooth animations**: Carefully polished interaction experience
- 🌐 **Bilingual interface**: Complete Chinese and English interface support
- 📐 **Responsive layout**: Perfect adaptation to various screen sizes

---

## 🔗 Related Projects

### 🔍 Agent Scanner Skill

If you like the security scanning feature of Agent Skills Guard, you can also try our Claude Code skill version:

**[agent-scanner-skill](https://github.com/brucevanfdm/agent-scanner-skill)** - More powerful security scanning with deep dependency analysis, known vulnerability detection, and intelligent risk assessment

No GUI required, perfect for developers who prefer working in the terminal.

---

## 🆚 Traditional Way vs Agent Skills Guard

| Feature                           | Traditional Way                       | Agent Skills Guard                                      |
| --------------------------------- | ------------------------------------- | ------------------------------------------------------- |
| **Discover skills/plugins** | ❌ Aimlessly search GitHub            | ✅ Featured repo + plugin marketplace, one-click browse |
| **Security check**          | ❌ Manual code review, time-consuming | ✅ 8-category auto scan, 3-5x faster, instant results   |
| **Install skills**          | ❌ Command line, error-prone          | ✅ Visual UI, plugin-style install, click to install    |
| **Manage skills/plugins**   | ❌ Folder digging, unclear usage      | ✅ Intuitive list, clear status                         |
| **Update skills/plugins**   | ❌ Manual check, repetitive           | ✅ Auto detect, batch update                            |
| **Uninstall skills**        | ❌ Manual delete, worried leftovers   | ✅ One-click uninstall, auto cleanup                    |

---

## 🚀 Quick Start

### 📥 Installation

Visit [GitHub Releases](https://github.com/brucevanfdm/agent-skills-guard/releases) to download the latest version:

- **macOS**: Download `.dmg` file, drag to install
- **Windows**: Download `.msi` installer, double-click to install

<div align="center">

*Security warnings on first launch can be safely ignored*

</div>

### 🎯 First Time Use

**Step 1: Browse and Install**

- Browse and search skills in "Skills Marketplace"
- Click "Install", system will automatically perform security scan
- Check security score and scan report, install with peace of mind

**Step 2: Manage Installed Skills**

- One-click scan all skills' security status in "Overview" page
- View details, update or uninstall in "My Skills"

## 💎 Interface Showcase

### 📊 Overview Page

See all skills' security status at a glance, risk category statistics, and issue details clearly.

![Overview](screen-shot/overview.png)

### 🛡️ Security Scan Report

Detailed scan results, including security score, risk level, problem list.

![Scan result](screen-shot/scanresult.png)

### 📦 My Skills

View all installed skills, support multi-path management, batch update and uninstall.

![My skills](screen-shot/myskills.png)
![Skills update](screen-shot/skillsupdate.png)

### 🛒 Skills Marketplace

Explore and install community skills from featured repositories.

![Skills marketplace](screen-shot/skillsmarket.png)

### 🗄️ Repository Configuration

Add and manage skill sources, built-in featured marketplace and GitHub repositories, updated regularly.

![Repositories](screen-shot/repositories.png)

---

## 🛡️ Security Scanning Details

### Scanning Mechanism

Our security scanning engine analyzes every file of skill code to detect potential risks:

- **Parallel scanning acceleration**: Parallel scanning technology greatly improves scan speed for local installed skills/plugins
- **Symbolic link detection**: Immediately hard-block on symlink discovery, prevent attacks
- **Multi-format support**: Support `.js`, `.ts`, `.py`, `.sh`, `.rs` and other code formats
- **Platform adaptation**: Added UTF-16 decoding and text confidence validation, extended Windows/multi-language support

### Scoring System Principles

#### How is the Security Score Calculated?

The security score uses a **100-point deduction mechanism**, starting from 100 points and deducting points based on detected risks:

1. **Initial Score**: 100 points (full score)
2. **Risk Deduction**: For each risk detected, deduct points based on its weight
3. **Same-Rule Deduplication**: Deduct points only once per rule in the same file (avoiding duplicate deductions)
4. **Score Accumulation**: All risk deductions accumulate, minimum to 0 points

#### Scoring Example

Assume the following risks are detected:

| Risk Item                   | Weight | Description                         |
| --------------------------- | ------ | ----------------------------------- |
| `rm -rf /` (hard trigger) | 100    | Installation prohibited directly    |
| `curl \| bash`             | 90     | Deduct 90 points                    |
| `eval()`                  | 6      | Deduct 6 points                     |
| `os.system()`             | 6      | Deduct 6 points                     |
| Hardcoded API Key           | 60     | Deduct 60 points                    |
| **Total Score**       | -      | 100 - 90 - 6 - 6 - 60 =**-0** |

Due to the presence of hard-trigger rules, installation is directly blocked.

#### Scoring Levels

- **90-100 (✅ Safe)**: Safe to use

  - No or only very low-risk items
  - No hard-trigger rules detected
- **70-89 (⚠️ Low Risk)**: Minor risk, recommend checking details

  - Few low-risk items
  - Decide whether to use based on needs
- **50-69 (⚠️ Medium Risk)**: Certain risk, use with caution

  - Medium-risk items present
  - Recommend carefully reviewing code before use
- **30-49 (🔴 High Risk)**: High risk, not recommended for installation

  - Multiple high-risk items
  - Strongly recommend finding alternatives
- **0-29 (🚨 Critical Risk)**: Serious threat, installation prohibited

  - Hard-trigger rules triggered
  - System directly blocks installation

### Hard-Trigger Protection Mechanism

**What are Hard-Trigger Rules?**

Hard-trigger rules are "red lines" set by the system. Once triggered, installation is immediately blocked without giving users a chance to take risks. These rules correspond to **extremely dangerous** operations, including:

- 🚨 **Destructive Operations** (8 rules): `rm -rf /`, disk wiping, formatting, etc.
- 🚨 **Remote Code Execution** (10 rules): `curl | bash`, reverse shell, PowerShell encoded commands, etc.
- 🚨 **Privilege Escalation** (1 rule): sudoers file modification
- 🚨 **Persistence Backdoor** (1 rule): SSH key injection
- 🚨 **Sensitive File Access** (2 rules): Reading shadow file, Windows credential store

Totaling **22 hard-trigger rules**, covering the most common attack vectors.

### Confidence Grading

To reduce false positives, each risk is marked with a confidence level:

- **🎯 High**: Low possibility of false positives, should focus on
- **🎯 Medium**: Some possibility of false positives, recommend manual review
- **🎯 Low**: High possibility of false positives, for reference only

**Score Adjustment**: Low-confidence risks have lower weights in scoring to avoid false positives causing excessively low scores.

### Risk Classification

| Category                         | Detection Content                   | Examples                          |
| -------------------------------- | ----------------------------------- | --------------------------------- |
| **Destructive Operations** | Delete system files, disk wipe      | `rm -rf /`, `mkfs`            |
| **Remote Code Execution**  | Pipe execution, deserialization     | `curl \| bash`, `pickle.loads` |
| **Command Injection**      | Dynamic command concatenation       | `eval()`, `os.system()`       |
| **Data Exfiltration**      | Data exfiltration to remote servers | `curl -d @file`                 |
| **Privilege Escalation**   | Escalation operations               | `sudo`, `chmod 777`           |
| **Persistence**            | Backdoor implantation               | `crontab`, SSH key injection    |
| **Sensitive Info Leakage** | Hardcoded keys, Tokens              | AWS Key, GitHub Token             |
| **Sensitive File Access**  | Access system sensitive files       | `~/.ssh/`, `/etc/passwd`      |

### Disclaimer

Security scanning is based on preset rules, designed to help identify potential risks, but cannot guarantee 100% accuracy, and false positives or false negatives may exist. It is recommended to carefully read the skill source code before installation and be extra cautious with skills from untrusted sources. Users assume all consequences of using this program.

---

## 📝 Changelog

[View full changelog](https://github.com/brucevanfdm/agent-skills-guard/releases)

---

## 📦 Download & Feedback

### Download

- 📦 [GitHub Releases](https://github.com/brucevanfdm/agent-skills-guard/releases) - Get the latest version

### Contact

Have questions or suggestions? Contact via:

- 💬 [GitHub Issues](https://github.com/brucevanfdm/agent-skills-guard/issues) - Report issues or suggest features
- 🐦 [X/Twitter](https://x.com/brucevanfdm) - Follow project updates
- 💬 **Agent Skills Security Community**

<div align="center">

![Community](screen-shot/agentskillsgroup.jpg)

</div>

---

## 🔧 For Developers

If you're a developer and want to build from source or contribute:

```bash
# 1. Clone the project
git clone https://github.com/brucevanfdm/agent-skills-guard.git
cd agent-skills-guard

# 2. Install dependencies (requires pnpm)
pnpm install

# 3. Run in development mode
pnpm dev

# 4. Build production version
pnpm build
```

**Tech Stack**: React 18 + TypeScript + Tauri 2 + Tailwind CSS

---

## ⭐ Star History

[![Star History Chart](https://api.star-history.com/svg?repos=brucevanfdm/agent-skills-guard&type=Date)](https://star-history.com/#brucevanfdm/agent-skills-guard&Date)

---

## 📜 License

MIT License - Free to use, free to share

---

<div align="center">

Made with ❤️ by [Bruce](https://github.com/brucevanfdm)

If this project helps you, please give it a ⭐️ Star!

[⬆ Back to top](#readme-top)

</div>
