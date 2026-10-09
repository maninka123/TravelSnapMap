# Release notes

## 0.5.0

A redesign around three places — Explore, Import and My Trips — plus fixes that make your corrections permanent.

**New**
- **Explore**: map and library in one place, with one search (places, cities, countries) and shared filters. A list
  beside the map follows what's visible; hovering a row highlights the pin. Breadcrumbs (All countries › Japan ›
  Kyoto). The map reopens where you left it.
- **Import**: drop screenshots anywhere in the window or choose files; each picture is imported once. Sources for
  Photos, folders, Reels and videos, live progress, retry, and the AI cost so far.
- **Review** grouped by reason, each explaining why it's asking, with a “Not travel” shortcut.
- **My Trips**: trip cards, day-by-day itinerary with drag and drop (and move up/down or “Move to day” from the
  keyboard), numbered trip map, visited ticks, Markdown export.
- **Welcome guide**: privacy and Photos access, choosing a source, saving the AI key to the Keychain, a small first import.
- **Settings** reorganised into sections, appearance (Match Mac / Light / Dark), and **Restore a backup**.
- New look: consistent spacing and type, icons instead of emoji, better dark mode, compact sidebar on narrow windows,
  Reduce Motion / Transparency / Contrast respected.

**Fixed**
- Removing a place from a screenshot, answering “not a place”, merging two places or correcting a location could be
  undone when the screenshot was read again. These decisions are now kept for good, and answered questions stay in
  “Recently answered”.
- The on-device filter skipped some travel posts (walks, food lists, non-English posts) before they reached the AI.
- The trip page could freeze while its places loaded.
- “1 screenshots” wording.

**Under the hood**
- Library v7 (automatic upgrade with a safety copy): correction tables and indexes for faster lookups.
- 95 Rust tests (incl. migration from v6, restore, import dedupe, accuracy gates) and 30 UI tests.
- Release workflow for a signed and notarized DMG when Apple credentials are configured.
