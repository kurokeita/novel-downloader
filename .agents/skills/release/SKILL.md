---
name: release
description: This skill should be used when the user asks to "cut a release", "release a new version", "bump the version", "publish a release", "create a GitHub release", "tag a release", or "ship vX.Y.Z" for truyenazz-crawler. Documents the end-to-end release workflow — semver decision, version-bump PR, tag-triggered CI build, and GitHub release notes.
---

# Releasing truyenazz-crawler

This skill documents how a release is cut for `truyenazz-crawler`. The
process is: bump the crate version on a branch, merge a clean
`chore(release):` PR, then push a `vX.Y.Z` tag. The tag push is what does
the real work — it triggers `.github/workflows/release.yml`, which builds
binaries for five platforms, creates the GitHub release, and publishes the
Homebrew formula and winget manifest.

## Dual-control gate (read first)

Pushing a tag and creating a public GitHub release are external,
irreversible actions. **Never push the tag or create/publish the release
autonomously.** Prepare everything up to that point, then stop and get
explicit human authorization before the tag push. Merging the version-bump
PR is likewise a human decision.

## Decide the version

Follow semver against the current `version` in `Cargo.toml`:

- **patch** (`x.y.Z`) — bug fixes, packaging-only, dependency cleanup, no
  new user-facing behavior. (e.g. `v1.0.1` was packaging-only: Homebrew +
  winget + Linux ARM target.)
- **minor** (`x.Y.0`) — new user-facing features, no breaking CLI changes.
  (e.g. `v1.1.0` added multi-host support and EPUB metadata.)
- **major** (`X.0.0`) — breaking changes to the CLI surface or output
  contract.

Inspect what landed since the last tag to classify:

```bash
git log $(git describe --tags --abbrev=0)..HEAD --oneline
git diff $(git describe --tags --abbrev=0)..HEAD --stat | tail -30
```

## Step 1 — Version-bump branch + PR

Keep this PR **pure**: only `Cargo.toml` and `Cargo.lock` change. This
matches the prior `chore(release): v1.0.0 (#2)` and `v1.0.1 (#5)` PRs.

```bash
git checkout main && git pull
git checkout -b chore/release-vX.Y.Z
# edit Cargo.toml: version = "X.Y.Z"
cargo build --release      # propagates the new version into Cargo.lock
```

Verify before committing (all four are the project's definition of done):

```bash
cargo test                 # all tests green
cargo clippy --all-targets # 0 warnings (CI floor)
cargo fmt --check          # clean
cargo build --release      # succeeds
```

Commit (follow the `git-commit` pre-commit gate), push, open the PR:

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore(release): vX.Y.Z"
git push -u origin chore/release-vX.Y.Z
gh pr create --base main --title "chore(release): vX.Y.Z" --body "..."
```

Do **not** stage unrelated working-tree changes (e.g. local tooling config).

## Step 2 — Merge, then tag (human-gated)

After the human merges the PR, fast-forward local main and tag the squash
commit. Get explicit authorization before pushing the tag:

```bash
git checkout main && git pull
git tag vX.Y.Z          # annotate from the merged release commit
git push origin vX.Y.Z  # DUAL-CONTROL: triggers release.yml
```

The tag must match `v*` (the workflow trigger) and the bumped crate version.

## Step 3 — What CI does automatically

`release.yml` (`on: push tags v*`) runs three jobs:

1. **build** — cross-compiles for `linux-x86_64`, `linux-aarch64`,
   `macos-x86_64`, `macos-aarch64`, `windows-x86_64`; packages each as
   `truyenazz-crawl-<label>.{tar.gz,zip}`; creates the release if it does
   not exist (`gh release create … --notes ""`, i.e. **empty notes**) and
   uploads the asset with `--clobber`.
2. **publish-homebrew** — renders `Formula/truyenazz-crawler.rb` with fresh
   SHA-256s and pushes it to `kurokeita/homebrew-brew` (needs
   `HOMEBREW_TAP_TOKEN`).
3. **publish-winget** — submits the manifest for `Kurokeita.TruyenazzCrawler`
   (needs `WINGET_TOKEN`).

Watch it:

```bash
gh run watch $(gh run list --workflow=release.yml --limit 1 --json databaseId -q '.[0].databaseId')
gh release view vX.Y.Z
```

## Step 4 — Write the release notes

CI creates the release with **empty** notes, so the rich notes are written
afterward. Draft them in a file, then apply:

```bash
gh release edit vX.Y.Z --notes-file RELEASE_NOTES_vX.Y.Z.md
```

The notes file is a working artifact, not committed to the repo. Use the
structure in `references/release-notes-template.md`, modeled on the `v1.0.1`
release. Keep the asset-name list in sync with the build matrix.

## Reference files

- **`references/release-notes-template.md`** — the GitHub release-notes
  skeleton (Highlights / Quality / Install / Downloads / What's changed),
  matching the established `v1.0.x` format.
