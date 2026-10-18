# Releasing

## One-time setup (before the first public release)

- [ ] **Employment contract:** check the clauses on side projects and who owns code written outside work.
- [ ] **Name:** "PenaDF" / "PenaDF" is already used by other PDF projects on GitHub. See the
      note in the delivery message; rename before publishing if you choose a new name.
- [ ] **Indonesian trademark:** search the name at https://pdki-indonesia.dgip.go.id (class 9, software).
- [ ] **Copyright line:** confirm the holder name in `NOTICE` (currently "Fery").
- [ ] **Two-step login (MFA)** on the GitHub account (required by SignPath).
- [ ] Create the GitHub repository and push `main`. CI runs the tests on Linux and Windows.

## Each release

1. Bump `version` in `Cargo.toml`, run `cargo build` so `Cargo.lock` updates.
2. `scripts/test.sh` passes; CI is green on `main`.
3. `git tag v<version> && git push origin v<version>`. CI builds the installer and zip and
   attaches them to a **draft** release.
4. Download the draft's installer, install it on a Windows PC, open two PDFs, close a tab,
   uninstall. Then publish the draft.

## After the first release

- **Code signing:** apply at https://signpath.org (free for open source; whole app must stay
  open source). Add a "Code signing policy" section to the README with the text SignPath asks for.
- **winget:** submit a manifest to https://github.com/microsoft/winget-pkgs so people can run
  `winget install PenaDF`. Do this after signing so installs show no warning.
