---
description: Review commits between master and current branch one-by-one via interactive rebase
user_invocable: true
allowed-tools: Bash, Read, Edit, Grep, Glob, AskUserQuestion, Skill
---

# Commit-by-Commit Code Review

Interactive review of all commits between `master` and the current branch. Each commit is paused, reviewed, optionally modified, then continued. Changes are folded into the original commit via amend.

## On Invocation

### Detect State

Check if a rebase is already in progress:

```bash
git rebase --show-current-patch > /dev/null 2>&1
```

- **If rebase in progress** — resume from where we left off. Jump to **Show Commit**.
- **If no rebase** — check for commits to review:

```bash
git log --oneline master..HEAD
```

If there are commits, start the rebase:

```bash
GIT_SEQUENCE_EDITOR="sed -i '' 's/^pick/edit/'" git rebase -i master
```

If there are no commits ahead of master, tell the user and stop.

## Review Loop

Repeat for each commit until the rebase completes.

### 1. Show Commit

Show the current commit diff:

```bash
git show HEAD --stat
git show HEAD
```

Present a concise summary to the user:
- Commit message
- Files changed
- Key changes (new structs, functions, tests, patterns, etc.)

### 2. Wait for Review

Ask the user:

> Review this commit. When finished, tell me what changes to make (if any), or say **"next"** to move on. Say **"pause"** to stop and resume later.

If the user says **"pause"**, stop immediately. The rebase stays in progress on disk and can be resumed by running `/code-review` again.

### 3. Process Changes

If the user requests changes:

1. Make the requested code changes using Edit/Write tools
2. Check for `@cc` comments in any changed files using Grep — execute those instructions too
3. Show the user what was changed (`git diff`)
4. Ask: **"Review again, or move to next commit?"**
5. If "review again", go back to step 3
6. If "next", proceed to step 4

If the user says **"next"** with no changes, skip straight to step 4.

### 4. Finalise & Continue

If any files were modified in this commit:

1. Run `cargo test` to verify tests still pass
2. Invoke `/simplify` on the changed files
3. Stage and amend:
   ```bash
   git add -A
   git commit --amend --no-edit
   ```

Then advance to the next commit:

```bash
git rebase --continue
```

If the rebase encounters conflicts, show them to the user and help resolve.

If `git rebase --continue` completes the rebase (exit code 0 and no more rebase state), tell the user: **"All commits reviewed! Rebase complete."** and stop.

Otherwise, loop back to **Show Commit** for the next commit.

## Conflict Handling

If `git rebase --continue` fails with conflicts:

1. Show `git status` and the conflicting files
2. Help the user resolve conflicts
3. After resolution: `git add -A && git rebase --continue`

## Notes

- The rebase state persists in `.git/rebase-merge/` — safe to pause and resume across sessions
- Each commit is amended in-place, preserving the original message
- `/simplify` is only run when there were actual changes to the commit
- `cargo test` is only run when there were actual changes to the commit
