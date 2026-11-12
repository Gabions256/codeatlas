# Packaging

Install channels and what the owner has to do for each release. The release
workflow (`.github/workflows/release.yml`) builds these archives for every
`vX.Y.Z` tag and attaches a `.sha256` file to each:

| Target | Archive |
| --- | --- |
| x86_64-unknown-linux-gnu | `atlas-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz` |
| x86_64-pc-windows-msvc | `atlas-vX.Y.Z-x86_64-pc-windows-msvc.zip` |
| aarch64-apple-darwin | `atlas-vX.Y.Z-aarch64-apple-darwin.tar.gz` |
| x86_64-apple-darwin | `atlas-vX.Y.Z-x86_64-apple-darwin.tar.gz` |

Each archive holds one folder, `atlas-vX.Y.Z-<target>/`, with the `atlas`
binary, `LICENSE` and `README.md`. The binaries are not code-signed.

The release is created as a draft. Nothing below works until the owner
publishes it on GitHub.

## cargo binstall

No per-release work. `crates/atlas-cli/Cargo.toml` tells binstall where the
archives are:

    cargo binstall --git https://github.com/Gabions256/codeatlas atlas-cli

This uses `--git` because the crate names `atlas-cli`, `codeatlas` and
`codeatlas-cli` are taken on crates.io by other projects.

## Homebrew

One-time: create the public repository `Gabions256/homebrew-tap`.

Per release:

1. Copy `homebrew/codeatlas.rb` to `Formula/codeatlas.rb` in the tap.
2. Set `version` and the three URLs to the new version.
3. Replace each `REPLACE_WITH_SHA256_<target>` with the hash from the
   matching `.sha256` file on the release.
4. Check with `brew install --build-from-source ./Formula/codeatlas.rb` and
   `brew test codeatlas`, then push.

Users then run `brew install gabions256/tap/codeatlas`. The package is
called `codeatlas` because other tools already use the name `atlas`; the
command it installs is still `atlas`.

## winget

Per release, after publishing the GitHub release:

1. Copy `winget/manifests/g/Gabions256/CodeAtlas/<version>/` and update the
   version, URL and `InstallerSha256` (from the `.zip.sha256` file).
2. Validate on Windows: `winget validate <folder>` and
   `winget install --manifest <folder>`.
3. Open a pull request to `microsoft/winget-pkgs` with the folder at the
   same path. Microsoft's bots and moderators review it.

Users then run `winget install Gabions256.CodeAtlas`.
