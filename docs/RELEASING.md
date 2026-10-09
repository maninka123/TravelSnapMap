# Releasing

Releases are built by `.github/workflows/release.yml` on macOS when a tag `v*` is pushed (or run manually). It runs
type-check, UI tests and Rust tests, builds `TravelSnapMap.app` and the `.dmg` for Apple silicon, and attaches the
DMG to a **draft** GitHub release for you to review and publish.

```bash
# bump the version in package.json, src-tauri/tauri.conf.json and src-tauri/Cargo.toml, update docs/RELEASE_NOTES.md
git tag v0.5.0 && git push origin v0.5.0
```

## Signing and notarization (needs an Apple Developer account)

Without these secrets the workflow still builds, but the app is **unsigned and not notarized** (ad-hoc signature),
and the release text says so. Add these repository secrets to sign and notarize:

| Secret | Value |
| --- | --- |
| `APPLE_CERTIFICATE` | Developer ID Application certificate exported as .p12, base64-encoded (`base64 -i cert.p12`) |
| `APPLE_CERTIFICATE_PASSWORD` | The .p12 password |
| `APPLE_SIGNING_IDENTITY` | e.g. `Developer ID Application: Your Name (TEAMID)` |
| `APPLE_ID` | Apple ID email used for notarization |
| `APPLE_PASSWORD` | An app-specific password for that Apple ID |
| `APPLE_TEAM_ID` | Your 10-character team ID |

Tauri signs the app and the bundled Swift helper with the hardened runtime and `src-tauri/Entitlements.plist`
(Photos library access only), submits it for notarization and staples the ticket. The workflow then checks the
result with `codesign`, `spctl` and `stapler`.

**Still to verify with real credentials:** that the signed Photos helper (`binaries/photos-bridge`) keeps its Photos
permission prompt under the hardened runtime. If macOS refuses access, give the helper its own entitlements file
(same key) and sign it before bundling.

## Updates

The app does not update itself: Settings → About → Check for updates opens the Releases page. An automatic updater
(Tauri updater plugin) needs a signing key pair whose private key is kept as a secret; it can be added once releases
are signed.

## Checklist

- [ ] Version bumped in the three files; `docs/RELEASE_NOTES.md` updated
- [ ] CI green on the commit
- [ ] Draft release: DMG opens, app launches, welcome guide works, Photos permission prompt appears
- [ ] Release text states correctly whether the build is signed and notarized
