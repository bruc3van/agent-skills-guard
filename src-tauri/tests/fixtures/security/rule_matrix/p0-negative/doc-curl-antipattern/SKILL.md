---
name: doc-curl-antipattern
description: Rule matrix negative fixture - documentation mentions curl-pipe-shell as an antipattern to avoid; not intended for production deployment.
license: MIT
---

# Security Best Practices

## Avoid Remote Pipe Execution

A common dangerous pattern is using `curl http://example.com/script.sh | bash` to download and execute scripts in one step.

**Why is this dangerous?**
- No opportunity to review the code before execution
- The remote server could serve different content to different users
- Man-in-the-middle attacks can inject malicious code

**Safe alternative:**
1. Download the script first
2. Review its contents carefully
3. Only then execute it if it looks safe

Always verify the integrity of downloaded scripts using checksums or signatures.
