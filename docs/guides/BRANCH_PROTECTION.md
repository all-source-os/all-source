---
title: "Branch Protection Settings"
status: REFERENCE
last_updated: 2026-11-24
category: guide
---

# Branch Protection Settings

**Date**: February 2026 (last reviewed April 2026)
**Status**: REFERENCE for multi-contributor deployments
**Audience**: Repository Maintainers running forks or multi-contributor deployments of AllSource

> ⚠️ **Not the workflow for this repo's day-to-day work.** The upstream `all-source-os/all-source` repository is maintained by a single owner who commits directly to `main`. See the **Git Workflow** section of [`CLAUDE.md`](../../CLAUDE.md) for the authoritative contributor guidance here: no feature branches, no PRs for routine work, direct `git push origin main`.
>
> This document remains available as a reference for **downstream maintainers** running forks, multi-contributor deployments, or regulated environments who need a stricter PR-based review workflow. The recommendations below (required status checks, required reviews, signed commits, etc.) are all still sound advice in that context — they just aren't enforced on this upstream repo.

This document provides recommended branch protection settings for the AllSource monorepo in deployments that require collaborative PR review — for example, forks run by a team with multiple committers or deployments with compliance requirements.

---

## Overview

Branch protection rules prevent direct pushes to critical branches and require specific conditions to be met before merging. These settings work with the CI/CD pipeline to enforce quality gates.

---

## Quick Setup Checklist

For repository administrators, here's a quick checklist to configure branch protection:

1. Go to **Settings > Branches > Add branch protection rule**
2. Apply rules to `main` branch
3. Configure the settings below
4. Optionally add rules for `develop` branch

---

## Recommended Settings for `main` Branch

### Required Status Checks

Enable: **Require status checks to pass before merging**

The following checks should be marked as **required**:

| Check Name | Workflow | Purpose |
|------------|----------|---------|
| `quality-gate` | `ci.yml` | Aggregates all quality checks |
| `workflow-lint` | `ci.yml` | Validates GitHub Actions syntax |
| `workflow-validate` | `ci.yml` | Validates workflow structure |
| `version-check` | `ci.yml` | Ensures version consistency |
| `rust-quality` | `ci.yml` | Rust formatting, linting, tests |
| `go-quality` | `ci.yml` | Go formatting, linting, tests |
| `elixir-query-quality` | `ci.yml` | Query Service: Elixir formatting, analysis, tests |
| `elixir-mcp-quality` | `ci.yml` | MCP Server: Elixir formatting, analysis, tests |
| `summary` | `container-ci.yml` | Container build validation |

**Important**: Enable **Require branches to be up to date before merging** to ensure PRs are tested against the latest `main`.

### Pull Request Reviews

Enable: **Require a pull request before merging**

| Setting | Recommended Value | Rationale |
|---------|-------------------|-----------|
| Required approving reviews | 1-2 | Ensures code review by peers |
| Dismiss stale pull request approvals | Yes | Re-review after new commits |
| Require review from code owners | Optional | Use if CODEOWNERS file exists |
| Require approval of most recent push | Yes | Latest changes must be approved |

### Conversation Resolution

Enable: **Require conversation resolution before merging**

This ensures all review comments are addressed before merge.

### Commit Signatures

Enable: **Require signed commits** (Recommended for security)

This verifies commit authenticity. Contributors need to set up GPG or SSH signing.

### Linear History

Enable: **Require linear history**

Enforces squash or rebase merging, resulting in a cleaner git history.

### Branch Restrictions

Enable: **Restrict who can push to matching branches**

Limit direct pushes to:
- Repository administrators only
- Or specific teams (e.g., `release-managers`)

### Force Pushes

Disable: **Allow force pushes**

Force pushes can destroy history and should be blocked on protected branches.

### Deletions

Disable: **Allow deletions**

Prevents accidental deletion of the `main` branch.

---

## Recommended Settings for `develop` Branch

Apply similar settings with slightly relaxed requirements:

| Setting | `main` | `develop` |
|---------|--------|-----------|
| Required reviewers | 2 | 1 |
| Require up-to-date | Yes | Optional |
| Require linear history | Yes | Optional |
| Require signed commits | Yes | Optional |

---

## Required Status Checks Detail

### CI Workflow (`ci.yml`)

These checks run on every push and PR:

```
workflow-lint          Fast syntax validation of GitHub Actions
workflow-validate      Structure validation with act
version-check          Cross-service version consistency
changes               Detects which services changed (conditional)
rust-quality          Rust: fmt, sort, clippy, test, build, docs
go-quality            Go: mod verify, fmt, golangci-lint, test, build
elixir-query-quality  Query Service: format, compile, credo, dialyzer, test
elixir-mcp-quality    MCP Server: format, compile, credo, dialyzer, test
quality-gate          Summary gate - aggregates all results
```

**How Conditional Checks Work**:
- `rust-quality` only runs if `apps/core/**` changed
- `go-quality` only runs if `apps/control-plane/**` changed
- `elixir-query-quality` only runs if `apps/query-service/**` changed
- `elixir-mcp-quality` only runs if `apps/mcp-server-elixir/**` changed
- Workflow file changes trigger all checks

### Container CI Workflow (`container-ci.yml`)

Runs after CI succeeds:

```
check-ci              Verifies upstream CI passed
changes               Detects container changes
build                 Docker build validation (no push)
integration           Docker Compose integration tests
summary               Aggregated results
```

### Security Workflow (`security.yml`)

Runs on `main` pushes and weekly:

```
dependency-audit      cargo-audit, govulncheck, mix deps.audit
codeql                Static analysis for Go
container-scan        Trivy vulnerability scanning
license-check         License compliance
security-summary      Aggregated results
```

**Note**: Security checks run separately and may be configured as required or informational based on your security posture.

---

## GitHub Settings Location

To configure branch protection:

1. Navigate to your repository on GitHub
2. Click **Settings** (requires admin access)
3. In the left sidebar, click **Branches**
4. Under "Branch protection rules", click **Add rule**
5. In "Branch name pattern", enter: `main`
6. Configure settings as described above
7. Click **Create** or **Save changes**

Repeat for `develop` or other branches as needed.

---

## Example Configuration (JSON)

For programmatic setup via GitHub API or Terraform, here's the recommended configuration:

```json
{
  "required_status_checks": {
    "strict": true,
    "contexts": [
      "quality-gate",
      "workflow-lint",
      "workflow-validate",
      "version-check",
      "rust-quality",
      "go-quality",
      "elixir-query-quality",
      "elixir-mcp-quality"
    ]
  },
  "enforce_admins": false,
  "required_pull_request_reviews": {
    "dismiss_stale_reviews": true,
    "require_code_owner_reviews": false,
    "required_approving_review_count": 1,
    "require_last_push_approval": true
  },
  "required_conversation_resolution": true,
  "required_signatures": true,
  "required_linear_history": true,
  "allow_force_pushes": false,
  "allow_deletions": false,
  "restrictions": null
}
```

---

## Terraform Configuration

For infrastructure-as-code setups:

```hcl
resource "github_branch_protection" "main" {
  repository_id = github_repository.allsource.node_id
  pattern       = "main"

  required_status_checks {
    strict   = true
    contexts = [
      "quality-gate",
      "workflow-lint",
      "workflow-validate",
      "version-check",
      "rust-quality",
      "go-quality",
      "elixir-query-quality",
      "elixir-mcp-quality"
    ]
  }

  required_pull_request_reviews {
    dismiss_stale_reviews           = true
    require_code_owner_reviews      = false
    required_approving_review_count = 1
    require_last_push_approval      = true
  }

  require_conversation_resolution = true
  require_signed_commits          = true
  allows_force_pushes            = false
  allows_deletions               = false
}
```

---

## Rulesets (GitHub Enterprise)

For GitHub Enterprise or organizations using rulesets, create rules with:

| Rule Type | Configuration |
|-----------|---------------|
| Restrict creations | Enabled |
| Restrict updates | Enabled |
| Restrict deletions | Enabled |
| Require linear history | Enabled |
| Require signed commits | Enabled |
| Require pull request | Min approvals: 1 |
| Require status checks | See list above |
| Block force pushes | Enabled |

---

## Bypass Permissions

Configure bypass for emergency situations:

| Actor | Can Bypass |
|-------|------------|
| Repository Administrators | Yes (with audit log) |
| Specific Teams | No |
| GitHub Actions | No |

**Note**: All bypasses should be logged and reviewed. Emergency bypasses should be followed by a cleanup PR within 24 hours.

---

## CODEOWNERS Integration

If using CODEOWNERS, create `.github/CODEOWNERS`:

```
# Global owners
* @allsource-maintainers

# Service-specific owners
/apps/core/           @rust-team
/apps/control-plane/  @go-team
/apps/query-service/  @elixir-team
/apps/web/            @frontend-team

# Workflow files require extra review
/.github/workflows/   @devops-team @allsource-maintainers

# Documentation
/docs/                @tech-writers @allsource-maintainers
```

Then enable "Require review from code owners" in branch protection.

---

## Security Considerations

### Why These Settings Matter

| Setting | Security Benefit |
|---------|------------------|
| Require status checks | Prevents untested code from merging |
| Require reviews | Enforces four-eyes principle |
| Require signed commits | Verifies commit authenticity |
| Disable force pushes | Protects audit trail |
| Require up-to-date | Tests against latest code |

### Audit Logging

GitHub Enterprise provides audit logs for:
- Branch protection rule changes
- Protection bypasses
- Force pushes (if allowed)
- Branch deletions (if allowed)

Review these logs regularly.

---

## Troubleshooting

### "Required status check is failing"

1. Check the Actions tab for workflow run status
2. Review the specific job that failed
3. Common causes:
   - Formatting issues: Run `make quality-gates` locally
   - Test failures: Run `make test` locally
   - Merge conflicts: Rebase on latest `main`

### "Merge button is disabled"

Verify:
1. All required status checks have passed
2. Required reviewers have approved
3. All conversations are resolved
4. Branch is up-to-date (if required)
5. Commits are signed (if required)

### "Status checks not appearing"

- Ensure the workflow file is in the default branch
- Check that path filters match the changed files
- Verify workflow is enabled in Actions settings

### Bypassing Protection (Emergency Only)

If you must bypass in an emergency:

1. Document the reason
2. Get verbal approval from 2+ maintainers
3. Create a follow-up PR within 24 hours to address any issues
4. Review the bypass in the next team meeting

---

## Best Practices

### For Contributors

1. Always create feature branches from `main`
2. Run `make quality-gates` before pushing
3. Keep PRs focused and small
4. Respond to review comments promptly
5. Rebase on `main` if your branch is behind

### For Maintainers

1. Review and merge PRs within 24-48 hours
2. Use squash merging for clean history
3. Delete branches after merging
4. Regularly audit branch protection settings
5. Keep required checks list updated as CI evolves

### For Administrators

1. Review bypass permissions quarterly
2. Audit protection rule changes monthly
3. Update settings when new services are added
4. Document any temporary relaxations

---

## Local Verification

Before pushing, run the same checks locally:

```bash
# Run all quality gates (matches CI)
make quality-gates

# Or run individual service checks
make quality-rust       # Rust checks
make quality-go         # Go checks
make quality-elixir     # Elixir checks

# Validate workflows locally
make validate-workflows
```

This catches issues before CI runs, reducing feedback loops.

---

## Related Documentation

- [Quality Gates](../current/QUALITY_GATES.md) - Detailed quality gate documentation
- [Quality Gates Setup](QUALITY_GATES_SETUP.md) - Initial setup guide
- [CI Workflow](../../.github/workflows/ci.yml) - Main CI workflow
- [Container CI Workflow](../../.github/workflows/container-ci.yml) - Container testing
- [Security Workflow](../../.github/workflows/security.yml) - Security scanning

---

## Summary

| Branch | Required Checks | Required Reviews | Linear History | Signed Commits |
|--------|-----------------|------------------|----------------|----------------|
| `main` | quality-gate, workflow-lint, workflow-validate, version-check, rust-quality*, go-quality*, elixir-query-quality*, elixir-mcp-quality* | 1-2 | Yes | Recommended |
| `develop` | quality-gate | 1 | Optional | Optional |

\* Conditional checks - only run when relevant files change, but required when they do run.

**Key Principles**:
1. All code must pass CI before merging
2. All code must be reviewed by at least one person
3. Direct pushes to `main` are prohibited
4. Force pushes are never allowed on protected branches
5. Emergency bypasses require documentation and follow-up

---

**Document Status**: CURRENT
**Version**: 1.0
**Last Updated**: February 2026
