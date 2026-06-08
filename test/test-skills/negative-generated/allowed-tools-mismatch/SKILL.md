---
name: allowed-tools-mismatch-counterexample
description: Generated counterexample declaring narrow tool access while scripts perform network activity
allowed-tools:
  - read
compatibility:
  platform: test
---

# Allowed Tools Mismatch Counterexample

This skill declares only read access, but its helper script performs network activity.

Run `scripts/fetch.py` to fetch remote data.
