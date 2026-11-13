# Packages

| Channel | State | Install |
| --- | --- | --- |
| GitHub release archives | published | download from [Releases](https://github.com/Gabions256/codeatlas/releases) |
| Homebrew (macOS, Linux) | published, own tap | `brew install gabions256/tap/codeatlas` |
| cargo binstall / install | works from git | `cargo binstall --git https://github.com/Gabions256/codeatlas atlas-cli` |
| winget (Windows) | manifests ready, not submitted | see below |
| crates.io | not published | crate names must be decided first |

## Homebrew

The formula lives in [Gabions256/homebrew-tap](https://github.com/Gabions256/homebrew-tap). For a new release, update `version`, the three URLs and the three `sha256` values in `Formula/codeatlas.rb`. Its CI installs and tests the formula on macOS and Linux.

## winget

`winget/manifests/g/Gabions256/CodeAtlas/<version>/` follows the layout of [microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs). Check them with:

    winget validate --manifest packaging/winget/manifests/g/Gabions256/CodeAtlas/0.1.1

To publish, copy the folder into a fork of `microsoft/winget-pkgs` at the same path and open a pull request. The first pull request asks the submitter to accept Microsoft's Contributor License Agreement; only the account owner can do that.

For a new release, copy the folder, then change `PackageVersion`, `ReleaseDate`, the URL, the nested file path and `InstallerSha256` (upper case) to the new Windows archive.
