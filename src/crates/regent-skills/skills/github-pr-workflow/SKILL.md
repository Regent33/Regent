---
name: github-pr-workflow
description: "GitHub PR lifecycle: branch, commit, open, CI, merge."
version: 1.0.0
created_by: bundled
pinned: true
tags: [github, pull-requests, ci, git]
---

The full PR lifecycle via `terminal`. Each step shows the `gh` way first,
then a `git` + `curl` fallback for machines without `gh`.

## Detect what's available
```bash
if command -v gh &>/dev/null && gh auth status &>/dev/null; then
  AUTH=gh
else
  AUTH=git   # needs GITHUB_TOKEN in the environment for API calls
fi
```

## Owner/repo from the remote
```bash
REMOTE_URL=$(git remote get-url origin)
OWNER_REPO=$(echo "$REMOTE_URL" | sed -E 's|.*github\.com[:/]||; s|\.git$||')
OWNER=$(echo "$OWNER_REPO" | cut -d/ -f1)
REPO=$(echo "$OWNER_REPO" | cut -d/ -f2)
```

## 1. Branch
```bash
git fetch origin && git checkout main && git pull origin main
git checkout -b feat/add-user-authentication
```
Prefixes: `feat/`, `fix/`, `refactor/`, `docs/`, `ci/`.

## 2. Commit
Make changes with `file_edit`/`apply_patch`/`write_file`, then:
```bash
git add src/auth.rs tests/auth_test.rs
git commit -m "feat: add JWT-based user authentication

- Add login/register endpoints
- Add auth middleware for protected routes"
```
Conventional Commits format: `type(scope): short description`, body wrapped
at 72 chars. Types: feat, fix, refactor, docs, test, ci, chore, perf.

## 3. Push and open the PR
```bash
git push -u origin HEAD
```
**gh:**
```bash
gh pr create --title "feat: add JWT-based user authentication" --body "## Summary
- Adds login/register endpoints
- JWT token generation and validation

## Test Plan
- [ ] Unit tests pass

Closes #42"
```
Options: `--draft`, `--reviewer user1,user2`, `--label enhancement`,
`--base develop`.

**git + curl:**
```bash
BRANCH=$(git branch --show-current)
curl -s -X POST -H "Authorization: token $GITHUB_TOKEN" \
  -H "Accept: application/vnd.github.v3+json" \
  https://api.github.com/repos/$OWNER/$REPO/pulls \
  -d "{\"title\":\"feat: ...\",\"body\":\"...\",\"head\":\"$BRANCH\",\"base\":\"main\"}"
```
The response `number` field is the PR number — save it. Add `"draft":true`
for a draft PR.

## 4. Monitor CI
**gh:**
```bash
gh pr checks           # one-shot
gh pr checks --watch   # polls until done
```
**git + curl:**
```bash
SHA=$(git rev-parse HEAD)
curl -s -H "Authorization: token $GITHUB_TOKEN" \
  https://api.github.com/repos/$OWNER/$REPO/commits/$SHA/status | jq '.state, .statuses[]'
curl -s -H "Authorization: token $GITHUB_TOKEN" \
  https://api.github.com/repos/$OWNER/$REPO/commits/$SHA/check-runs | jq '.check_runs[] | {name, status, conclusion}'
```
Poll every 30s for up to ~10 minutes if watching without `gh`.

## 5. Auto-fixing CI failures
**gh:**
```bash
gh run list --branch $(git branch --show-current) --limit 5
gh run view <RUN_ID> --log-failed
```
**git + curl:**
```bash
curl -s -H "Authorization: token $GITHUB_TOKEN" \
  "https://api.github.com/repos/$OWNER/$REPO/actions/runs?branch=$(git branch --show-current)&per_page=5" | jq '.workflow_runs[] | {id, name, conclusion}'
curl -s -L -H "Authorization: token $GITHUB_TOKEN" \
  https://api.github.com/repos/$OWNER/$REPO/actions/runs/<RUN_ID>/logs -o ci-logs.zip
unzip -o ci-logs.zip -d ci-logs && cat ci-logs/*.txt
```

**Loop:** check status → read failure logs → `read_file` + `file_edit` the
fix → `git add . && git commit -m "fix: ..." && git push` → re-check.
Up to 3 attempts, then ask the user — use `systematic-debugging` if the
cause isn't obvious.

## 6. Merge
**gh:**
```bash
gh pr merge --squash --delete-branch
gh pr merge --auto --squash --delete-branch   # merges once checks pass
```
**git + curl:**
```bash
curl -s -X PUT -H "Authorization: token $GITHUB_TOKEN" \
  https://api.github.com/repos/$OWNER/$REPO/pulls/$PR_NUMBER/merge \
  -d '{"merge_method":"squash","commit_title":"feat: ... (#'$PR_NUMBER')"}'
git push origin --delete $(git branch --show-current)
git checkout main && git pull origin main
```
Merge methods: `merge`, `squash`, `rebase`.

## Reference

| Action | gh | git + curl |
|---|---|---|
| List my PRs | `gh pr list --author @me` | `GET /repos/$OWNER/$REPO/pulls?state=open` |
| View PR diff | `gh pr diff` | `git diff main...HEAD` |
| Add comment | `gh pr comment N --body "..."` | `POST /issues/N/comments` |
| Request review | `gh pr edit N --add-reviewer user` | `POST /pulls/N/requested_reviewers` |
| Close PR | `gh pr close N` | `PATCH /pulls/N -d '{"state":"closed"}'` |
| Check out someone's PR | `gh pr checkout N` | `git fetch origin pull/N/head:pr-N` |

*Adapted from Hermes Agent (MIT, © 2025 Nous Research).*
