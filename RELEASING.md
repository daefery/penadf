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

## Removing the SmartScreen warning

Microsoft's own comparison (learn.microsoft.com/windows/apps/package-and-deploy/code-signing-options,
checked 18 Oct 2026):

| Route | Cost | SmartScreen |
|---|---|---|
| Microsoft Store (MSIX), the Store signs it | Free | No warnings |
| Azure Artifact Signing | ~$9.99/month | Warnings until reputation builds; individuals: USA and Canada only |
| OV / EV certificate | $150-400+/year | Warnings until reputation builds (EV no longer bypasses since 2024) |
| SignPath Foundation (open source) | Free | Signed by "SignPath Foundation"; apply at signpath.org |

**1. Microsoft Store (removes the warning for Store installs):**
1. Partner Center (https://partner.microsoft.com/dashboard), individual developer account (free,
   ID verification).
2. Apps and games > New product > MSIX or PWA app > reserve the name **PenaDF**.
3. Product identity page: copy Package/Identity/Name, Package/Identity/Publisher and
   PublisherDisplayName.
4. `PENADF_STORE_IDENTITY=... PENADF_STORE_PUBLISHER=... PENADF_STORE_PUBLISHER_NAME=... scripts/build-msix.sh`
5. Upload `dist/PenaDF-<version>.msix` (unsigned) in a new submission. Store listing text and
   screenshots: use `docs/assets/screen-*.png`; privacy policy URL:
   https://github.com/daefery/penadf/blob/main/PRIVACY.md
6. The `runFullTrust` capability needs a one-line justification in the submission: "PenaDF is a
   desktop PDF editor that opens and saves files the user chooses."

**2. SignPath Foundation (signs the GitHub installer and zip):** apply at https://signpath.org with
the repository URL. Before applying: two-step login on GitHub, a "Code signing policy" section in
the README (text in SignPath's terms), and PRIVACY.md (done). After approval, add their signing
step to `.github/workflows/ci.yml` before the release files are made.

## macOS: removing the "cannot be checked" warning

The `.dmg` is ad-hoc signed, so Gatekeeper asks users to allow it once (System Settings →
Privacy & Security → Open Anyway; Control-click no longer works since macOS Sequoia). Removing
that needs an Apple Developer Program membership ($99/year, developer.apple.com/programs),
a Developer ID Application certificate, and notarization (`xcrun notarytool submit ... --wait`,
then `xcrun stapler staple`). Add those steps to `scripts/build-macos.sh` and the macOS CI job,
with the certificate and an app-specific password as GitHub secrets.

## After the first release

- **winget:** submit a manifest to https://github.com/microsoft/winget-pkgs so people can run
  `winget install PenaDF`. Do this after signing so installs show no warning.
