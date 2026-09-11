# AGENTS.md

Guidance for AI coding agents working in this repo.

## Commit messages: Conventional Commits are REQUIRED

Releases are automated with **release-please**, which derives the next version
and the changelog **solely from commit messages**. Non-conforming commits are
silently ignored for versioning, so a release will simply not happen. Always
use the [Conventional Commits](https://www.conventionalcommits.org) format:

```
<type>[optional scope]: <description>

[optional body]
[optional footer(s)]
```

Common types and their release effect:

| Type | Use for | Version bump |
|------|---------|--------------|
| `feat:` | a new user-facing capability | **minor** (0.X.0) |
| `fix:` | a bug fix | **patch** (0.0.X) |
| `docs:`, `chore:`, `refactor:`, `test:`, `ci:`, `perf:`, `style:`, `build:` | everything else | none |
| `feat!:` / `fix!:` or a `BREAKING CHANGE:` footer | incompatible change | **major** (X.0.0) (pre-1.0: minor) |

Rules:

- The description must start **immediately** after `type: ` (a bare
  `Add thing` line is *not* parseable and will be dropped from the changelog).
- Use a scope when it helps, e.g. `feat(serve):`, `fix(sync):`,
  `chore(sprites):`.
- Only `feat:` and `fix:` (and breaking changes) cut a release. If a batch of
  work contains no `feat:`/`fix:`, no release PR is opened — add at least one,
  or force a specific version with a `Release-As: X.Y.Z` footer on an
  (optionally empty) commit.
- One logical change per commit; keep the subject ≤ ~72 chars.

Examples:

```
feat(categorise): add category + labels pipeline stage via Google Places
fix(sync): stop advancing the watermark when a page fetch fails
chore(ci): make the release build Cargo.toml-driven
```

## Pushing from the sandboxed environment

Plain `git push` (and `git push -u origin …`) **fails in this sandbox**:
`origin` is an SSH remote (`git@github.com:nelson/pocketsmith-rust-api.git`)
and git tunnels SSH through a SOCKS proxy via an `nc` ProxyCommand that the
sandbox refuses to execute (`nc … : Operation not permitted` → broken pipe).
`ssh -T` sometimes works, but the push transport flakes — don't rely on it.

**Working method: push over HTTPS using `gh`'s git credential helper.**
The corporate environment injects a CA that git's default bundle doesn't
trust, so `GIT_SSL_CAINFO=/etc/ssl/cert.pem` is still required.

```bash
GIT_SSL_CAINFO=/etc/ssl/cert.pem \
git -c credential.helper='!gh auth git-credential' \
  push -u "https://github.com/nelson/pocketsmith-rust-api.git" <branch>
```

Notes:
- **The old inline-token form no longer works.** Passing the github.com
  token as the URL password
  (`https://nelson:${GH_TOKEN}@github.com/...`) now fails with
  `remote: Invalid username or token. Password authentication is not
  supported for Git operations.` — GitHub rejects the `gh` OAuth token as a
  raw git password. Use the `gh auth git-credential` helper instead (it
  supplies the right credential for the transport).
- `gh` must be logged in to github.com (`gh auth status -h github.com`).
- For force pushes, prefer a lease and tag the old remote tip first:
  ```bash
  git tag -f backup/<name> <old-remote-sha>
  GIT_SSL_CAINFO=/etc/ssl/cert.pem \
  git -c credential.helper='!gh auth git-credential' \
    push --force-with-lease=<branch>:<old-remote-sha> \
    "https://github.com/nelson/pocketsmith-rust-api.git" <branch>
  ```
- `gh pr create` / `gh pr list` / `git ls-remote` over HTTPS still work with
  the github.com token; only the **git push password** path changed:
  ```bash
  GH_TOKEN=$(gh auth token -h github.com 2>/dev/null) \
  GIT_SSL_CAINFO=/etc/ssl/cert.pem \
  gh pr create --repo nelson/pocketsmith-rust-api --base master --head <branch> ...
  ```

## Cargo location

`cargo` may not be on `PATH`. On this machine it can live at e.g.
`/run/current-system/sw/bin/cargo`, `/etc/profiles/per-user/nelson/bin/cargo`,
or `/nix/store/*-cargo-*/bin/cargo`. If missing, locate it with
`find /nix/store -maxdepth 3 -name cargo -type f`.

## Tests

- Library + most logic: `cargo test`
- Web UI (`serve` binary): `cargo test --features web`
- Real-DB fidelity/coverage checks (needs a local `pocketsmith.db`):
  `cargo test --features fidelity`
