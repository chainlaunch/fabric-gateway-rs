# Security Policy

## Supported Versions

This project is pre-1.0; only the latest `0.x` release line receives security
fixes.

| Version | Supported |
|---------|-----------|
| 0.1.x   | ✅        |
| < 0.1   | ❌        |

## Reporting a Vulnerability

Please **do not** open a public GitHub issue for security vulnerabilities.

Instead, report them privately to dviejo@kfs.es (or via GitHub's
[private vulnerability reporting](https://docs.github.com/en/code-security/security-advisories/guidance-on-reporting-and-writing/privately-reporting-a-security-vulnerability),
if enabled on this repo).

Include:

- A description of the vulnerability and its potential impact
- Steps to reproduce (a minimal proof of concept, if possible)
- The affected version(s)

We aim to acknowledge reports within 3 business days and to disclose fixed
vulnerabilities via a security advisory once a patch is released.

## Scope notes

This crate signs Fabric transactions with an ECDSA P-256 private key. Reports
involving signature malleability, key handling, or deviations from Fabric's
required low-S signature encoding are especially in scope.
