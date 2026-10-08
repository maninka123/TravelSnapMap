# Installing TravelSnapMap

1. Download `TravelSnapMap_<version>_aarch64.dmg` from the [Releases](https://github.com/maninka123/TravelSnapMap/releases) page.
2. Open it and drag **TravelSnapMap** to **Applications**.
3. Open TravelSnapMap. The welcome guide walks through Photos access, your DeepSeek API key and a small first import.

Requirements: a Mac with Apple silicon and macOS 15 or later. A DeepSeek API key (platform.deepseek.com) for
extracting places; screenshots can be imported and read without one.

## If macOS says the app can't be opened

Release notes say whether a build is **signed and notarized**. Builds made without an Apple Developer ID are
unsigned: macOS shows “TravelSnapMap can't be opened because Apple cannot check it”. To open it anyway, right-click
the app in Applications → **Open** → **Open**. You only need to do this once.

## Photos access

The request comes from **TravelSnapMap Photos Bridge**, the small helper that reads screenshots. If you declined:
System Settings → Privacy & Security → Photos → turn on TravelSnapMap Photos Bridge (Settings → Privacy in the app
has a button that opens this page).

## Troubleshooting

| Problem | What to do |
| --- | --- |
| Places aren't extracted | Settings → AI: check the key is stored. Import → status shows “Waiting for network” while offline; it resumes by itself. |
| Some screenshots “couldn't be read” | Usually an iCloud download error or a moved file. Import → Retry failed. |
| A Reel says “needs video” | Instagram didn't share the video. Save it to Photos (or screen-record it) and use Import → Video from Photos. |
| A place is in the wrong spot | Open it → ⋯ → Correct location. The fix survives re-processing. |
| Something looks wrong after an update | Settings → Backup & export → Back up now, then report the issue with the log below. |

Logs: `~/Library/Logs/TravelSnapMap/travelsnapmap.log` (screenshot contents and keys are never logged).
Library: `~/Library/Application Support/com.travelsnapmap.app/` (Settings → Privacy → Show in Finder).

## Back up and restore

Settings → Backup & export → **Back up…** writes one .zip (library, photos, settings — never the API key).
**Restore…** checks the backup, then replaces the library after a restart; the current library is moved to
`backups/before-restore-…` inside the library folder, never deleted. Every library upgrade also makes a copy first
(`backups/pre-migration-…`).

## Updating

Download the newer .dmg and replace the app in Applications. Your library is kept and upgraded automatically (with a
safety copy). Settings → About → Check for updates opens the releases page.
