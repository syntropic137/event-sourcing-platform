# Security Policy

## Supported Versions

| Version | Supported          |
|---------|--------------------|
| latest  | :white_check_mark: |

## Reporting a Vulnerability

If you discover a security vulnerability, please report it responsibly:

1. **Do not** open a public GitHub issue
2. Email security concerns to the maintainers via GitHub's private vulnerability reporting:
   **Settings > Security > Advisories > Report a vulnerability**
3. Include steps to reproduce, affected versions, and potential impact

We will acknowledge receipt within 48 hours and aim to provide a fix timeline within 5 business days.

## Security Practices

This project uses:
- SHA-pinned GitHub Actions (supply chain defense)
- CodeQL SAST scanning (static analysis)
- OSV Scanner (dependency vulnerability scanning)
- gitleaks (secret scanning in CI)
- Dependabot (automated dependency updates)
- Least-privilege CI permissions
