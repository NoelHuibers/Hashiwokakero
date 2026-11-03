# Stores beyond Google Play and the App Store

Hashi in more app stores, one store per pull request. Google Play and the App Store are in
[mobile.md](mobile.md); how the builds and the release work is in
[app/README.md](../app/README.md#android-builds). Each store's upload is skipped with a notice
until its secrets exist, so its pull request can be merged before the account is there.

| # | Store | Build | Status |
|---|-------|-------|--------|
| 1 | Our own F-Droid repository (GitHub Pages) | foss APKs, direct key | ✓ (this PR) |
| 2 | F-Droid (f-droid.org) | built by F-Droid from source | |
| 3 | Samsung Galaxy Store | | |
| 4 | Amazon Appstore | foss APK | |
| 5 | Huawei AppGallery | foss | |
| 6 | Desktop builds (Windows, macOS, Linux) | | |
| 7 | Microsoft Store | | |
| 8 | Flathub, Snap | | |
| 9 | itch.io | | |
| 10 | Steam | | |

Not planned: IzzyOnDroid (its policy turns away apps written with AI), the Chinese stores (they
need a Chinese company), Poki and CrazyGames (they need their ads SDKs).

## The builds and keys

- **foss**, the Android build without any proprietary code (no Google Play Games):
  `HASHI_ANDROID_FLAVOR=foss`, an APK per ABI ([app/README.md](../app/README.md#android-builds)).
  For F-Droid and every store that isn't Google Play.
- **The direct key** signs it: a PKCS12 keystore, key `direct`, RSA 4096, in the secrets
  `ANDROID_DIRECT_KEYSTORE_BASE64`, `ANDROID_DIRECT_KEYSTORE_PASSWORD`, `ANDROID_DIRECT_KEY_ALIAS`
  and `ANDROID_DIRECT_KEY_PASSWORD`. SHA-256
  `80:B7:94:1F:80:88:45:EC:15:06:72:CE:3D:16:9F:D9:DB:71:D9:83:56:6B:9B:DD:4C:E7:9C:65:A2:93:22:6A`.
  Not the Play upload key. Every store that takes our signed APK (Amazon, Huawei, Samsung) gets
  APKs signed with it, so an app from one of them updates from another.
- **The F-Droid index key** signs our F-Droid repository's index: key `fdroid-repo`, in the
  secrets `FDROID_KEYSTORE_BASE64`, `FDROID_KEYSTORE_PASSWORD`, `FDROID_KEY_ALIAS` and
  `FDROID_KEY_PASSWORD`. Its fingerprint is what users pin.

Both can't be replaced without every user acting (reinstalling the app, adding the repository
again): keep their offline backups.

## Our F-Droid repository

Repository `https://noelhuibers.github.io/Hashiwokakero/fdroid/repo`, fingerprint
`6A8181FD3400E0D24AF2F603D1503D679BD37295DA1A598F1971A6DE9255AFD2`. Start page with both, an "Add
to F-Droid" link and a QR code: <https://noelhuibers.github.io/Hashiwokakero/>, short
<https://hashi.rs/fdroid>.

**Adding it** in F-Droid (Droid-ify, Neo Store and others alike): open the start page on the phone
and tap "Add to F-Droid"; or Settings, Repositories, "+", paste

```
https://noelhuibers.github.io/Hashiwokakero/fdroid/repo?fingerprint=6A8181FD3400E0D24AF2F603D1503D679BD37295DA1A598F1971A6DE9255AFD2
```

and confirm. Hashi then shows up in the app list (pull to refresh) and updates from there. An app
from Google Play has another signature: uninstall it first (with an account, results and streak
stay on hashi.rs).

Each release tag publishes it (app-release.yml, [app/README.md](../app/README.md#f-droid-repository)).
`gh workflow run app-release.yml -f fdroid=true` publishes it again from the GitHub releases, e.g.
after new store texts or screenshots.

### Owner tasks

Done (2026-11-03): the direct key and the F-Droid index key, in the secrets above; GitHub Pages on
with GitHub Actions as its source; the environment `github-pages` allows the tags `app-v*` next to
main.

1. **Move the key backups offline**: `~/agents/stores/keys/` has both keystores with their
   passwords (`README.txt` says which is which). Into the password manager or another offline
   place, then delete the folder. Without the direct key no update reaches the users of the foss
   build; without the index key everyone has to add the repository again.
2. **The first publication** comes with the next release tag (`app-v1.1.0` predates the foss
   build). Check it: the run's "F-Droid repository" summary, then the start page, then add the
   repository on a phone.
3. **Screenshots**: the repository takes them from a "Store listing" run on main at most three days
   old, else keeps the ones it has. The first release has none until one ran:
   `gh workflow run store-listing.yml -f platform=play -f what=images -f validate_only=true`
   (saves nothing on Play), then `gh workflow run app-release.yml -f fdroid=true`. The "Store
   images" job currently times out on main, so this needs that fixed first.
