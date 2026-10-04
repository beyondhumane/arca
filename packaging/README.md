# Package managers

Normal CLI and GUI builds include read-only RAR/CBR support. `RAR-NOTICES.txt`
contains the published license/notice texts for the adapter's dependency branch,
including shared dependencies and the union of the five release targets. It is
included in portable archives, installers and package-manager installations.
It is not a license inventory for all of Arca or a provenance attestation.

After changing the locked dependency graph, regenerate and check it with:

```sh
python3 packaging/rar-notices.py
python3 packaging/rar-notices.py --check
```

`render.sh` writes the manifests for one release from its `SHA256SUMS.txt`.
The `packages` job in `.github/workflows/release.yml` runs it after publishing
and pushes each one. A channel stays off until its secret exists.

| Channel | Where it lands | Needs |
| --- | --- | --- |
| Homebrew (macOS, Linux) | `beyondhumane/homebrew-tap`, `Formula/arca.rb` | `PACKAGES_TOKEN` |
| Scoop (Windows) | `beyondhumane/scoop-bucket`, `bucket/arca.json` | `PACKAGES_TOKEN` |
| AUR (Arch) | `arca-bin` on aur.archlinux.org | `AUR_SSH_KEY` |
| WinGet (Windows) | pull request to `microsoft/winget-pkgs`, `BeyondHumane.Arca` | `WINGET_TOKEN`, `WINGET_FORK_OWNER` |

- `PACKAGES_TOKEN`: fine-grained token with *Contents: read and write* on the
  two repositories above, which have to exist with a first commit.
- `AUR_SSH_KEY`: private half of an SSH key whose public half is on the AUR
  account that owns `arca-bin`. The package is created by the first push.
- `WINGET_TOKEN`: classic token with `public_repo`, from the account that has
  a fork of `microsoft/winget-pkgs`; `WINGET_FORK_OWNER` (a repository
  variable, not a secret) is that account. WinGet only updates packages it
  already knows: submit the first version by hand with
  `komac new BeyondHumane.Arca`.

Once installed:

```sh
brew install beyondhumane/tap/arca
scoop bucket add arca https://github.com/beyondhumane/scoop-bucket && scoop install arca
winget install BeyondHumane.Arca
yay -S arca-bin
```
