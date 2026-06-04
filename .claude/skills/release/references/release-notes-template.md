# Release notes template

Skeleton for the GitHub release body, modeled on the `v1.0.1` release.
Copy the section structure below into `RELEASE_NOTES_vX.Y.Z.md`, fill in
the placeholders, and apply with
`gh release edit vX.Y.Z --notes-file RELEASE_NOTES_vX.Y.Z.md`.

Drop sections that do not apply (omit "Highlights" for a packaging-only
patch; add a "Packaging change" section when the archive layout changes).

## Section structure

- **Lead line** — `**truyenazz-crawler vX.Y.Z** — <feature / packaging / fix>. <one-sentence summary>. <breaking-change note, or "No breaking CLI changes from vPREV.">`
- **`## Highlights`** — bulleted, bold-led feature entries: `**<name>.** <what it does and why a user cares>`
- **`## Other improvements`** — smaller changes, dependency cleanup
- **`## Quality`** — four lines: `<N> tests passing`, `cargo clippy --all-targets` clean, `cargo fmt --check` clean, `cargo build --release` succeeds
- **`## Install`** — Homebrew (`brew tap kurokeita/brew` + `brew install truyenazz-crawler`) and winget (`winget install Kurokeita.TruyenazzCrawler`) blocks
- **`## Downloads`** — the five attached assets (see list below)
- **`## What's changed since vPREV`** — one bullet per merged PR: `<commit subject> (#PR)`

## Asset list (keep verbatim, in sync with `release.yml` matrix)

- Linux x86_64 (`truyenazz-crawl-linux-x86_64.tar.gz`)
- Linux ARM64 (`truyenazz-crawl-linux-aarch64.tar.gz`)
- macOS Intel (`truyenazz-crawl-macos-x86_64.tar.gz`)
- macOS Apple Silicon (`truyenazz-crawl-macos-aarch64.tar.gz`)
- Windows x86_64 (`truyenazz-crawl-windows-x86_64.zip`)

## Notes

- The asset-name list must match the `matrix.label` values in
  `.github/workflows/release.yml`. Update both together if a target is
  added or removed.
- The Homebrew/winget sections only apply from `v1.0.1` onward (when those
  publish jobs were added). Keep them for every release since.
- A filled-in example lives at the repo root as `RELEASE_NOTES_v1.1.0.md`
  whenever a release is in flight (it is a working artifact, not committed).
