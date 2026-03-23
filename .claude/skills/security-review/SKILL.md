---
name: security-review
description: Use when completing development work, before creating PRs, or on-demand to audit code changes for security vulnerabilities across any tech stack
---

# Security Review

Audit code changes for common vulnerability patterns. Detects the tech stack and applies language-specific checks.

**Core principle:** Security is verified systematically, not caught ad-hoc.

**Announce at start:** "I'm using the security-review skill to audit these changes."

## When to Use

- Before PR creation (auto-triggered via finishing-a-development-branch Step 1.5)
- On-demand via `/security-review`
- After adding new dependencies
- After fixing bugs in security-sensitive code (subprocess calls, file I/O, user input handling)

## Procedure

### Step 1: Determine Scope

**Pre-PR review (default):** Changed files only.

```bash
BASE=$(git merge-base HEAD main)
git diff --name-only $BASE..HEAD
```

**On-demand full review:** All source files. State scope explicitly in output.

### Step 2: Detect Tech Stack

Check for indicator files and note which language-specific checks apply:

| File | Stack | Audit Tool |
|------|-------|------------|
| `Cargo.toml` | Rust | `cargo audit` |
| `package.json` | Node.js | `npm audit` |
| `pyproject.toml`, `requirements.txt` | Python | `pip-audit` |
| `go.mod` | Go | `govulncheck` |

Multiple stacks may apply.

### Step 3: Run Dependency Audit

Execute the appropriate audit tool. Capture and report output.

If the tool is not installed, report as an Important finding — do not skip silently.

### Step 4: Run the 8-Point Checklist

Check every category against the scoped files. Do not skip categories — report "Pass" for clean categories rather than omitting them.

**1. Command Execution** — All subprocess calls pass arguments safely?

| Safe | Unsafe |
|------|--------|
| Rust: `Command::new().arg()` | `sh -c` with interpolated strings |
| Python: `subprocess.run([...])` | `shell=True`, `os.system()` |
| Node: `execFile()` | `exec()` with string interpolation |
| Go: `exec.Command()` with separate args | String concatenation in args |

**2. Dependency Vulnerabilities** — Audit tool output from Step 3.

**3. Input Validation** — User-controlled data validated before sensitive operations?

Trace from source (file dialogs, text fields, CLI args, config files) to sink (file operations, commands, output). Check for type validation, bounds checking, encoding validation at boundaries.

**4. Path Handling** — Typed path APIs used, not string concatenation?

| Safe | Unsafe |
|------|--------|
| Rust: `Path`, `PathBuf`, `.join()` | `format!("{}/{}", dir, file)` |
| Python: `pathlib.Path` | f-string path construction |
| Node: `path.join()` | String concatenation |

Check for `../` traversal vectors in user-controlled paths.

**5. Secrets/Credentials** — No hardcoded secrets in source?

Grep for: `password`, `secret`, `token`, `api_key`, `private_key`, `Bearer`, connection strings, high-entropy strings. Check `.gitignore` covers `.env`, `*.pem`, `*.key`.

**6. Unsafe Code** — All unsafe blocks justified and documented?

Rust: `unsafe {`, `unsafe fn`. Go: `unsafe.Pointer`. Python: `ctypes`, `cffi`.

Each block needs a `// SAFETY:` comment explaining invariants.

**7. Error Information Disclosure** — User-facing errors free of system internals?

Check for: absolute paths, stack traces, hostnames, connection strings, software versions in error output.

**8. Temp File Handling** — Secure creation with automatic cleanup?

| Safe | Unsafe |
|------|--------|
| Rust: `tempfile::TempDir` | Manual `/tmp/myapp_` paths |
| Python: `tempfile.mkstemp()` | Predictable temp names |
| Go: `os.MkdirTemp()` | Missing cleanup |

### Step 5: Report

Use this exact format:

```
### Security Review Results

**Scope:** [X files changed since BASE_SHA / full project scan]
**Tech stack:** [detected]

#### 1. Command Execution: [PASS/FAIL]
[findings or "No issues found"]

#### 2. Dependency Vulnerabilities: [PASS/FAIL/SKIPPED]
[audit tool output summary]

#### 3. Input Validation: [PASS/FAIL]
#### 4. Path Handling: [PASS/FAIL]
#### 5. Secrets/Credentials: [PASS/FAIL]
#### 6. Unsafe Code: [PASS/FAIL]
#### 7. Error Information Disclosure: [PASS/FAIL]
#### 8. Temp File Handling: [PASS/FAIL]

### Findings by Severity

#### Critical (Must Fix Before Merge)
[file:line — category — what/why/how]

#### Important (Should Fix Before Merge)
[file:line — category — what/why/how]

#### Minor (Informational)
[observations]

### Verdict
**Passed?** [Yes / No / With fixes]
**Reasoning:** [1-2 sentences]
```

## Red Flags

**Never:**
- Skip a checklist category (report Pass, not silence)
- Skip dependency audit because tool isn't installed (report it)
- Mark everything as Critical (reserve for exploitable vulnerabilities)
- Include process recommendations (CI changes, tooling) — stay in your lane as a code auditor

**Always:**
- Run the actual audit tool, don't just note its absence
- Use file:line references for every finding
- State the attack scenario for each finding (why it matters)
- Report scope and tech stack at the top

## Integration

**Called by:**
- **finishing-a-development-branch** (Step 1.5) — Before push and PR creation

**Pairs with:**
- **requesting-code-review** — Security review first, then code quality review

**Invocable on-demand:** `/security-review`
