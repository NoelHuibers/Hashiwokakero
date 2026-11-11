# Stores beyond Google Play and the App Store

Hashi in more app stores, one store per pull request. Google Play and the App Store are in
[mobile.md](mobile.md); how the builds and the release work is in
[app/README.md](../app/README.md#android-builds). Each store's upload is skipped with a notice
until its secrets exist, so its pull request can be merged before the account is there.

| # | Store | Build | Status |
|---|-------|-------|--------|
| 1 | Our own F-Droid repository (GitHub Pages) | foss APKs, direct key | ✓ #200 |
| 2 | F-Droid (f-droid.org) | built by F-Droid from source, ships our signed APKs | recipe #211, the MR is an owner task |
| 3 | Samsung Galaxy Store | foss APKs (arm64-v8a, armeabi-v7a), direct key | upload in CI, the account is an owner task |
| 4 | Amazon Appstore (Fire tablets) | foss APKs (arm64-v8a, armeabi-v7a), Amazon re-signs | upload in CI, the account is an owner task |
| 5 | Huawei AppGallery | foss APKs (arm64-v8a, armeabi-v7a), direct key | upload in CI, the account is an owner task |
| 6 | Desktop builds (Windows, macOS, Linux) | Tauri installers: Windows x64/arm64, macOS universal, Linux x64/arm64 | built on each release, macOS signing is an owner task |
| 7 | Microsoft Store | the Windows builds as one MSIX bundle (x64, arm64), Microsoft signs it | upload in CI, the account is an owner task |
| 8 | Flathub, Snap | | |
| 9 | itch.io | | |
| 10 | Steam | | |

Not planned: IzzyOnDroid (its policy turns away apps written with AI), the Chinese stores (they
need a Chinese company), Poki and CrazyGames (they need their ads SDKs).

## The builds and keys

- **foss**, the Android build without any proprietary code (no Google Play Games):
  `HASHI_ANDROID_FLAVOR=foss`, an APK per ABI ([app/README.md](../app/README.md#android-builds)).
  For F-Droid and every store that isn't Google Play.
- **The direct key** signs it: a PKCS12 keystore, key `hashi-direct`, RSA 4096, in the secrets
  `ANDROID_DIRECT_KEYSTORE_BASE64`, `ANDROID_DIRECT_KEYSTORE_PASSWORD`, `ANDROID_DIRECT_KEY_ALIAS`
  and `ANDROID_DIRECT_KEY_PASSWORD`. SHA-256
  `80:B7:94:1F:80:88:45:EC:15:06:72:CE:3D:16:9F:D9:DB:71:D9:83:56:6B:9B:DD:4C:E7:9C:65:A2:93:22:6A`.
  Not the Play upload key. Every store that takes our signed APK (Samsung, Huawei) gets APKs
  signed with it, so an app from one of them updates from another. Amazon gets them too, but
  re-signs them with a key of its own (see [Amazon Appstore](#amazon-appstore)).
- **The F-Droid index key** signs our F-Droid repository's index: key `hashi-fdroid`, in the
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

## F-Droid (f-droid.org)

F-Droid builds the app itself from the source of each release tag, with the recipe
`app/fdroid/fdroiddata/rs.hashi.yml` (in F-Droid's repository fdroiddata as
`metadata/rs.hashi.yml`; how it builds: [app/README.md](../app/README.md#f-droid-fdroidorg)).
Its builds are reproducible: F-Droid compares them with the foss APKs of our GitHub release and
publishes ours, signed with the direct key, so the app from F-Droid, from our repository and from
the GitHub release are the same and update each other. Later versions come by themselves: F-Droid
checks for new `app-v*` tags.

**Anti-features**: `TetheredNet` only, with the text of the recipe. The daily, the rankings,
rivals and groups need hashi.rs, which the app can't be pointed elsewhere (`HASHI_SERVER_URL` is
a build setting); F-Droid flags exactly that, even for a free server, so we declare it rather
than have the reviewers add it. Not `NonFreeNet`: the server is free software (MIT) in the same
repository. Not `Tracking` (no analytics, nothing sent but what the player does, see
`/datenschutz`), not `NonFreeDep`/`NonFreeAssets` (the foss build has no Google code, the fonts
are OFL, the icons ours), not `Ads`.

### Owner tasks

1. **The first release with the foss build.** The recipe builds a release tag whose GitHub release
   has the foss APKs, and `app-v1.1.0` has neither this build script nor foss APKs. So: the next
   release (`node scripts/app-version.ts <x.y.z>`, tag `app-v<x.y.z>`, [app/README.md](../app/README.md#cutting-one)),
   and check that its GitHub release has `hashi-<x.y.z>-foss-{armeabi-v7a,arm64-v8a,x86_64}.apk`.
2. **The recipe for that version.** In `app/fdroid/fdroiddata/rs.hashi.yml` set, in all three
   builds, `versionName: <x.y.z>`, `versionCode` to `node scripts/app-version.ts --code` times ten
   plus 1, 2 and 4 (1.2.0: 10020001, 10020002, 10020004), `commit` to the tag's full commit
   (`git rev-parse app-v<x.y.z>^{commit}`), and `CurrentVersion`/`CurrentVersionCode` to the
   version and the x86_64 versionCode. If Tailwind, Lightning CSS or wasm-opt changed since this
   pull request, nothing else changes (the submodules follow them in our repository).
3. **A GitLab account** (gitlab.com, free) and a **fork of
   [fdroiddata](https://gitlab.com/fdroid/fdroiddata)** ("Fork", public, the branch not
   protected). Clone it (a shallow clone is enough: `git clone --depth 50`), make a branch
   (`git switch -c rs.hashi`), copy the recipe to `metadata/rs.hashi.yml`, commit ("New app:
   Hashi"), push.
4. **Check it before the MR** (optional; the MR's pipeline does the same): with
   [fdroidserver](https://gitlab.com/fdroid/fdroidserver) installed (`pip install fdroidserver`
   or Debian's package), in the fork: `fdroid lint rs.hashi` (no output), `fdroid rewritemeta
   rs.hashi` (changes nothing), `fdroid readmeta`.
5. **Open the merge request** against `fdroid/fdroiddata` `master` with the title and description
   below (the description fills in fdroiddata's template). Its pipeline builds all three APKs from
   source (the "fdroid build" job, about an hour each; F-Droid pays for the CI, if GitLab asks for
   a phone number or a credit card, don't give one and say so in the MR) and compares them with
   our APKs. "check apk" scans them.
6. **Answer the reviewers** (often within days, sometimes weeks; one reviewer, usually linsui,
   asks in the MR's comments). What they usually ask, and the answers:
   - *"Why a build script instead of the steps in the recipe?"* The release builds the APKs with
     the same script, which is what makes them reproducible; it pins or reads every version from
     the repository, so auto-updates keep working when a tool version changes.
   - *"Use srclibs / submodules for the tools."* The sources F-Droid compiles (Tailwind's oxide,
     Lightning CSS, binaryen) are submodules of our repository, at the versions of our lock
     files (CI checks it); Trunk, wasm-bindgen-cli and tauri-cli come from crates.io with
     `cargo install --locked` at pinned versions; Node is the official build with its SHA-256.
   - *"Node, pnpm, `node_modules`"*: only the Tailwind CLI and our script copying the fonts
     (`@fontsource`, OFL) run from it; its prebuilt binaries are deleted by `scandelete` and the
     two Tailwind needs are compiled from source.
   - *"Reduce the build time / split it"*: it is one full build per ABI, like other Rust apps;
     the `timeout` is 18000 s.
   - *"Is the server free software?"* Yes, the same repository (`src/server`), MIT, hence
     `TetheredNet` and not `NonFreeNet`.
   - *"Screenshots / changelogs in fastlane"*: `fastlane/metadata/android/<locale>/` has the
     title and the descriptions in 25 locales (en-US among them), which F-Droid takes from the
     tag, but no images (the store images are made in CI, not committed) and no changelogs. See
     step 8.
   - A version that doesn't build reproducibly: the job log shows the difference; reply in the
     MR, fix it in our repository, release a new version, and update the recipe's builds to it.
7. **After the merge**: F-Droid's build server builds the version within a day or two, and the
   app shows up at `https://f-droid.org/packages/rs.hashi/`. Later releases need nothing: F-Droid's
   checkupdates adds each new tag (it runs a few times a day; the build follows within days).
   Updating the recipe (new build steps, a new ABI) is an MR to fdroiddata like the first one.
8. **Optional, for a better listing** (F-Droid reads these from the tag, nothing to do in
   fdroiddata): commit the en-US images of a "Store listing" run as
   `fastlane/metadata/android/en-US/images/featureGraphic.png` and `phoneScreenshots/1.png`, ...
   (without them F-Droid shows the APK's icon and no screenshots), and a changelog per release as
   `fastlane/metadata/android/en-US/changelogs/<versionCode>.txt` for each of the three ABI
   versionCodes (e.g. a copy of `app/store/whatsnew/whatsnew-en-US`).

### The merge request

Title:

```
New app: Hashi
```

Description (the template of fdroiddata, filled in):

```markdown
## Abstract for new app inclusion

### Please introduce the functions and usage of your app in summary, and also attach some screenshots.

Hashi is Hashiwokakero, the Japanese bridges logic puzzle, in pixel art: connect the islands
with bridges so that each island has as many bridges as its number and all islands form one
network. A new daily puzzle every day, the same for everyone, with rankings, streaks and medals;
unlimited generated puzzles in five difficulties (7×7 to 24×24) with hints, checks, undo and
redo, which work offline (the generator and the solver run on the phone); a workshop for your
own puzzles with a uniqueness check; groups and rivals for friends. 20 languages, no ads, no
in-app purchases, no tracking, no account needed. It is the app of https://hashi.rs: the same
Rust (Leptos) UI in a Tauri 2 WebView.

<attach two or three screenshots here, e.g. from the Play listing or the website>

### Please elaborate why you wrote this app. Please highlight the unique functions it provides over other similar apps in F-Droid.

F-Droid has Bridges in Simon Tatham's Puzzles; Hashi adds a daily puzzle shared by everyone with
rankings, groups and rivals, and generated puzzles up to 24×24 that always have exactly one
solution.

## Checklist

### Policy

* [x] The app complies with the [inclusion policy](https://f-droid.org/docs/Inclusion_Policy).
* [x] The original app author has been notified (and does not oppose the inclusion). I am the author.
* [x] The upstream app source code repo contains the app metadata in a Fastlane folder structure
  (fastlane/metadata/android, 25 locales including en-US).

### Docs

* [x] Please read the guide first if this is your first contribution.
* [x] Please make sure your metadata follows the best practice in our templates.
* [x] Please read the Build Metadata Reference and make sure your metadata is valid.
* [x] Please read the Quick Start Guide.

### Merge Request Setup

* [x] The title of this merge request should follow "New app: app name" format.
* [x] Please make sure your fdroiddata fork is public and your branch is not protected.
* [x] All related fdroiddata and RFP issues have been referenced in this merge request (none).
* [x] Please only submit one app in one MR.

### Metadata

* [x] Metadata must be put in `metadata/<applicationId>.yml`.
* [x] Metadata must be a valid YAML file.
* [x] Metadata must use LF as line ending.
* [x] Don't add summary/description/changelog/images or anything that should be provided in upstream repo.
* [x] Releases are tagged and auto update is enabled (tags `app-v<x.y.z>`, versionCode read from
  `app/src-tauri/tauri.conf.json`, one build per ABI through VercodeOperation).
* [x] There is an issue tracker and contact info of the author.
* [x] An AuthorName must be added.
* [x] External repos are added as git submodules instead of srclibs (`app/fdroid/third_party`:
  tailwindcss, lightningcss, binaryen, compiled by `app/fdroid/build.sh tools`).
* [x] Enable Reproducible Builds: the release workflow builds the foss APKs with the same script
  (`app/fdroid/build.sh`), `binary:` points at them, `AllowedAPKSigningKeys` is our key.
* [x] Setup abi split: armeabi-v7a, arm64-v8a and x86_64, versionCode × 10 + 1, 2, 4.
* [x] Only the latest versions should be kept in the metadata before it's merged.
* [x] Don't add any disabled versions in the metadata.
* [x] The `commit` field should be the full hash.

### Pipeline

* [ ] All pipelines should pass.
* [ ] All warnings and errors in the Reports tab should be fixed or explained.

Notes for the reviewers:

- The build is `app/fdroid/build.sh` in three steps (prepare in prebuild, then tools and apk),
  the same script our release uses for the APKs `binary:` points at. Its header lists where each
  version comes from.
- TetheredNet: the daily puzzle, rankings and groups need hashi.rs, our server (free software, in
  the same repository); Play works offline.
```

## Samsung Galaxy Store

Each release tag uploads the foss APKs to the Galaxy Store as an update of the app and submits it
for review: the job "Upload to Galaxy Store" of app-release.yml with `scripts/galaxy-store.ts`,
through Samsung's [Content Publish API](https://developer.samsung.com/galaxy-store/galaxy-store-developer-api/content-publish-api.html).
It is skipped with a notice until the secrets below exist, and Samsung's API only updates an app
that was registered by hand (owner tasks).

**Which build.** The foss APKs, not Play's build: Play Games' sign-in only works with the signature
Google gives the app on Play (the Play build would show a sign-in that fails), the Galaxy Store
doesn't take part in Play's app signing, and with the foss build the binaries are declared without
Google services (`gms: N`), which Samsung needs for countries without them. The same APKs as
F-Droid's, our repository's and the GitHub release's, so they update each other.

**Which key.** The direct key (`hashi-direct`): Samsung distributes an APK as it is signed (it only
signs App Bundles itself), so its users get our signature, as with F-Droid (Amazon re-signs).
Not the Play upload key: Play signs with its own key, so the Play app and the Galaxy app can't
update each other anyway, and the upload key isn't meant to sign what users install.

**Which APKs.** `arm64-v8a` and `armeabi-v7a`, each a binary of its own: every Samsung phone and
tablet is ARM (x86_64 is only for emulators and Chromebooks), Samsung requires a 64-bit binary, and
the 32-bit one is for the Android Go phones (Galaxy A0x Core and the like), which run 32-bit
Android on 64-bit chips. One universal APK would be twice the download for everyone. Their
versionCodes are the foss build's, the version's times ten plus 2 (arm64) and 1 (32-bit ARM), so
they never collide with each other or with an older version, and the 64-bit one is the higher:
where a device can run both, Samsung delivers the higher one. The repository variable
`GALAXY_STORE_ABIS` changes the set (`gh variable set GALAXY_STORE_ABIS --body arm64-v8a`), e.g.
if Samsung won't take the 32-bit binary next to the 64-bit one (error 5030, "no device configured
for binary").

**What a release does** (the order matters to Samsung):

1. Opens an update of the app (contentUpdate, with `app/store/whatsnew/whatsnew-en-US` as its
   "What's new" unless it names Google Play, Play Games or the App Store), or takes the update a
   run before left open. It refuses while the app is in review, rejected or waiting to be
   published (the job fails with Samsung's status: rerun it afterwards, below), and when the store
   has a newer version.
2. Uploads the APKs (one upload session), then deletes the update's binaries of older versions,
   then adds the new ones, 64-bit first, so each gets its devices. Until the review passes, the
   version on sale stays as it is.
3. Submits the update for review. The repository variable `GALAXY_STORE_SUBMIT=false` leaves it
   open for Seller Portal instead. After the review it goes live, unless the app's publication is
   set to manual in Seller Portal (then it waits as "Ready for Change" until you publish it).

A rerun (or `gh workflow run app-release.yml --ref app-v<x.y.z> -f galaxy=true`, which takes the
APKs from the tag's GitHub release and builds nothing) continues an open update: it uploads only
what is missing. A pull request's dry run checks the APKs, the version codes and the key's use with
a throwaway key, and calls nobody.

**The listing**: `gh workflow run store-listing.yml -f platform=galaxy -f what=all` puts the texts
of `fastlane/metadata/android` (title and full description; Samsung's short description has 40
bytes, Play's is longer, so Samsung's stays) in 19 languages, the icon (`public/icon-512.png`,
512×512) and each language's phone screenshots (Play's, 1080×1920: within Samsung's 320 to
3840 px, at most 2:1, 4 to 8 of them) into the app's open update, or a new one. Not submitted:
the next release submits it with its binaries (or submit it in Seller Portal). `what=text` sends
the texts and keeps the screenshots; `validate_only` only runs the checks. The languages
(`GALAXY` in `scripts/galaxy-store.ts`): Samsung has one Spanish and one Portuguese (they get
es-419 and pt-BR) and no Hindi listing; Thai's description is over Samsung's 4000 bytes (Thai
takes 3 bytes a letter) and stays as Samsung has it; Simplified and Traditional Chinese wait for
China's age rating, which Samsung requires with them. The pull requests of the store listing
check the texts and images against Samsung's rules. The images need no sizes of their own: the
optional hero image (1200×675, games only, some countries) isn't made.

### Owner tasks

1. **A seller account**, free (no sign-up or yearly fee; Samsung takes a share of sales only):
   a Samsung account, then <https://seller.samsungapps.com>, "Sign up" as a **private seller**
   (an individual; corporate is for companies). To publish, even free apps, it needs **commercial
   seller status**: "Apply for commercial seller" in the profile, as a private commercial seller
   (Seller Portal guide, "Private commercial seller request"). The name has to match your ID and
   your payout account; use the hashi@huibers.io address (Samsung asks for an explanation with
   gmail-like domains). Samsung checks it within days. No D-U-N-S number (that's for companies);
   payout details (bank or PayPal) matter only for paid apps, give them if Seller Portal insists.
2. **The next release with foss APKs** (any `app-v*` tag after #200), whose GitHub release has
   `hashi-<x.y.z>-foss-arm64-v8a.apk` and `-armeabi-v7a.apk`. The first upload is by hand from
   there.
3. **Register the app by hand** (Seller Portal, Apps, "Add New App", Android, default language
   English):
   - **App Information**: title `Hashi`; description from
     `fastlane/metadata/android/en-US/full_description.txt` (or "Import from Play Store", once the
     Play listing is public, which takes the texts and screenshots of every language); category
     Games, Puzzle; icon `public/icon-512.png`; 4 to 8 screenshots: the `en/play-phone/*.png` of
     the artifact `store-listing-images` of a "Store listing" run
     (`gh workflow run store-listing.yml -f platform=play -f what=images -f validate_only=true`);
     support email hashi@huibers.io, website https://hashi.rs, privacy policy
     https://hashi.rs/datenschutz, open source URL https://github.com/NoelHuibers/Hashiwokakero,
     copyright holder Noel Huibers.
   - **Age rating**: 4+ (`ageLimit` 4, as the App Store's 4+: no violence, no gambling, no ads,
     no purchases; players' names are public and they can form groups, but there is no free-text
     chat). If Seller Portal asks for China's age rating (with the Chinese listings), 3+. If it
     asks for a Korean game rating certificate for the Games category and has no questionnaire
     for it, leave South Korea out (countries below) and say so.
   - **Binary**: add both APKs ("Add more binary" for the second), Google Mobile Services **No**
     for each; keep the devices Samsung selects. If Seller Portal refuses the 32-bit one (no
     devices left), keep only arm64 and run `gh variable set GALAXY_STORE_ABIS --body arm64-v8a`.
   - **Country/Region & Price**: free; all countries except China (Chinese distribution needs a
     licence of a Chinese entity, as with the Chinese stores). Turn on adding new countries
     automatically if offered.
   - **Publication**: "Manually" for the first one is the safe choice (it waits after the review
     until you publish); afterwards "Automatically after review" makes each release go live by
     itself, as the CI submits it.
   - **Data Safety** (as `/datenschutz` and Play's form): collected, none shared with third
     parties, none sold, encrypted in transit, deletable (account page, or the contact form):
     - Personal info: **Name** (the user or guest name, public in the rankings; app
       functionality, account management; required for the daily ranking) and **Email address**
       (optional: account recovery, and the contact form's reply address; account management,
       developer communications).
     - App activity: **in-app actions / game progress** (results, times, moves, hints, stamps,
       groups, rivals; app functionality, and fraud prevention for the rankings) and **other
       user-generated content** (own puzzles and their titles, group names, reports and their
       note, messages of the contact form; app functionality, developer communications).
     - Device or other IDs: a **random device ID** the app makes (stored only as a daily hashed
       value; fraud prevention, security).
     - Not collected: location, contacts, photos, files, financial or health data, browsing
       history; no advertising ID, no analytics, no crash reports (the contact form shows what it
       sends: app version, platform, OS, language, before sending).
   - **Review** (comment for the testers): "No login needed: the app starts as a guest. The daily
     puzzle, rankings, groups and rivals need an internet connection (our server hashi.rs); Play
     works offline. Free, no ads, no in-app purchases. Source: https://github.com/NoelHuibers/Hashiwokakero."
   - Submit. The app's **Content ID** (12 digits) is in the app list.
4. **The service account** for the API: Seller Portal, Assistance, **API Service**, "Create
   Service Account", scope **Publishing & ITEM** (`publishing`), then download its private key
   (Seller Portal doesn't keep it, it is shown once) and copy the Service Account ID. Then:

   ```sh
   gh secret set SAMSUNG_SERVICE_ACCOUNT_ID --body '<the service account ID>'
   gh secret set SAMSUNG_PRIVATE_KEY < ~/Downloads/<the private key file>
   gh secret set SAMSUNG_CONTENT_ID --body '<the 12-digit content ID>'
   ```

   and delete the downloaded key file. No backup needed: a lost or leaked key is replaced by a new
   service account (and the old one deleted there), nothing users have depends on it. Each run
   gets its own access token and revokes it at the end.
5. **The first update through CI**: once the app is on sale (or right away, if it is still in
   review the job fails saying so), the next release tag uploads; or upload the current release
   now with `gh workflow run app-release.yml --ref app-v<x.y.z> -f galaxy=true`, which only works
   for a version above the one on sale. The job summary says what it did. Then the listing in the
   other languages: `gh workflow run store-listing.yml -f platform=galaxy -f what=all` (it goes
   out with the next release, or submit it in Seller Portal).

**Review**: Samsung reviews every update (pre-review, content review and a test on real
devices), usually within a few working days, sometimes up to two weeks. Seller Portal shows the stage, and Samsung emails the result. Common reasons for a
rejection: the app doesn't start or misbehaves on some device in the device test (the report
names the devices; fix it or leave the devices out under Binary), a listing or what's-new text
that names another store, screenshots that don't show the app, or a missing privacy policy. A
rejected update is fixed in Seller Portal (or by a new release once it is back to
"Re-Registering") and submitted again. While an update is in review, a release tag's upload fails
with the status; rerun it with `-f galaxy=true` after the review.

**Android developer verification**: from 2026 Google requires apps on certified Android devices
in more and more countries to come from a developer registered with Google, also outside Play.
Seller Portal shows a binary's status ("advStatus"); if it says the app isn't installable where
the verification applies, register the package `rs.hashi` with the direct key's certificate
(SHA-256 above) in the Android Developer Console. That affects F-Droid and the other stores the
same way.

## Amazon Appstore

Each release tag uploads the foss APKs to the Amazon Appstore as the next version of the app and
submits it for review: the job "Upload to Amazon Appstore" of app-release.yml with
`scripts/amazon-appstore.ts`, through Amazon's
[App Submission API](https://developer.amazon.com/docs/app-submission-api/overview.html)
([reference](https://amzndevresources.com/appsubapi/swagger-en/index.html)). It is skipped with
a notice until the secrets below exist, and the API only updates an app whose first version was
submitted by hand (owner tasks).

**Where the store still is** (November 2026). Since 20 August 2025 the Amazon Appstore is gone
from Android phones and tablets of other makers (from Windows since March 2025): it is the store
of Amazon's own Fire OS devices only, the Fire tablets and the Android-based Fire TVs. New Fire
TVs run Vega OS, which doesn't run Android apps. On 8 October 2026 Amazon announced the Alexa
Tablets, which replace the Fire tablets (shipping since 14 October in the US, Canada and Mexico,
then the UK and Europe): they run Android 16 with Google Play, so they get Hashi from Google Play,
the Play build with Play Games. The Fire tablets are reported to stop selling within months and to
get updates for years after. So this store reaches the Fire tablets people already have (Fire OS
7 and 8, Android 9 and 11), many millions but no longer growing; on them it is the only store
besides installing APKs by hand. It costs a free account and this job.

**Which build.** The foss APKs: Fire tablets have no Google services, so Play Games couldn't sign
in. The app needs Android 7 (minSdk 24): Fire OS 7 and 8, i.e. every Fire tablet from 2018 on;
the older ones on Fire OS 5 are left out by Amazon itself.

**Which APKs.** `arm64-v8a` and `armeabi-v7a`, as two binaries: every Fire tablet is ARM; those
up to 2020 (Fire 7 2019, Fire HD 8 2018 and 2020, Fire HD 10 2019) run a 32-bit Fire OS only, the
newer ones (Fire HD 10 2021 and 2023, Fire HD 8 2022 and 2024, Fire Max 11) a 64-bit one that runs
32-bit apps too, and Amazon asks to keep the 32-bit binary. Unlike Google Play and Samsung, Amazon
doesn't pick a device's binary by its ABI or versionCode: with two binaries each device is assigned
to one of them in the Developer Console, by hand, once (owner tasks). The job replaces each binary
with the new APK of its ABI, which keeps its devices, so a release changes nothing there; an APK
whose ABI the app has no binary of isn't uploaded. The simpler setup also works: the
`armeabi-v7a` binary alone runs on every Fire tablet (the job then only replaces that one), at the
cost of a 64-bit warning in the console and 32-bit code on the 64-bit tablets. Not x86_64 (no Fire
device has it), not a universal APK (twice the size, and these are the APKs of F-Droid and the
GitHub release). The versionCodes are the foss build's (the version's times ten plus 2 and 1).

**Which key, and Amazon's signature.** Amazon removes our signature and signs the app with a key
of its own for our developer account (the same for all its apps, never changing; its SHA-256 is on
the Developer Console's "Upload Your App File" page), and wraps it with code that talks to the
Appstore when the app starts. So the app from Amazon and the app from F-Droid, our repository, the
GitHub release or the Galaxy Store can't update each other (uninstall one first; with an account
the results and the streak stay on hashi.rs). The job still uploads the APKs signed with the direct
key, the same files as the GitHub release. Amazon DRM: off (the app is free and open source).
Google's Android developer verification doesn't apply: Fire OS isn't a certified Android with
Google's services, and the app there is signed by Amazon.

**What a release does:**

1. Opens an edit (the next version, a copy of the live one), or takes the edit a run before or the
   Developer Console left open. It refuses an edit in review.
2. Replaces each binary of the edit with this version's APK of its ABI (refusing binaries of a
   newer version, or ones that aren't foss APKs, e.g. Play's universal APK uploaded by hand), and
   checks that the edit then has exactly these versions.
3. Puts the release notes (`app/store/whatsnew/whatsnew-en-US`) into every language's "recent
   changes", which Amazon requires for an update: the English notes for all, as Play shows them.
   Notes that name Google Play, Play Games or the App Store become "Hashi <version>" instead.
4. Validates the edit and commits it, which submits it for review; after the review it goes live.
   The repository variable `AMAZON_APPSTORE_SUBMIT=false` leaves the edit open for the Developer
   Console instead.

A rerun (or `gh workflow run app-release.yml --ref app-v<x.y.z> -f amazon=true`, which takes the
APKs from the tag's GitHub release and builds nothing; `-f galaxy=true` next to it is fine)
continues an open edit: binaries that are this version already are skipped. A pull request's dry
run checks the APKs and the release notes and calls nobody.

**The listing**: `gh workflow run store-listing.yml -f platform=amazon -f what=all` puts the texts
of `fastlane/metadata/android` in 10 languages into the app's open edit, or a new one: the title,
the full description, its first paragraph as the short description and the first point of each of
its four lists as the feature bullets (Amazon wants 3 to 5); keywords and recent changes stay as
Amazon has them. With the images: the large icon (`public/icon-512.png`), the small icon
(`app/store/amazon/small-icon.png`, 114×114: the icon's 16×16 pixels at 7 px each on a pixel of
its background, made with `magick public/icon-512.png -filter point -resize 112x112 -background
'#f4ecdd' -gravity center -extent 114x114 -strip PNG32:app/store/amazon/small-icon.png`), the
promotional image (Play's feature graphic, 1024×500) and each language's 7" tablet screenshots
(`play-tablet-7`, 1080×1920: Amazon takes only 800×480, 1024×600, 1280×720, 1280×800,
1920×1080, 1920×1200 and 2560×1600, either way up, which Play's 10" and the iPad's sizes are not;
3 to 10 of them, we have 7). Not submitted: the next release commits it with its binaries (or
submit it in the Developer Console). `what=text` sends the texts only; `validate_only` only runs
the checks, as the pull requests of the store listing do. No Fire TV images: the app isn't for
Fire TV.

The languages (`AMAZON` in `scripts/amazon-appstore.ts`): Amazon's store listings are English
(US, and Britain, Australia and India, which fall back to the US text), German, French, Italian,
Spanish (one, for Spain and Mexico: es-ES), Portuguese (Brazil and Portugal), Dutch, Polish,
Japanese and Swedish (which we don't have). Every other country gets the English listing. A
language the app's listing doesn't have yet is added by the script; should Amazon refuse that,
the job says so, and it is added in the Developer Console (then run it again).

### Owner tasks

1. **A developer account**, free (no sign-up or yearly fee): <https://developer.amazon.com>,
   "Developer Console", with an Amazon account (a new one with hashi@huibers.io keeps it apart
   from shopping), two-step verification on. The profile: developer name `Noel Huibers` (shown in
   the store), the hashi@huibers.io address, website https://hashi.rs, the postal address of the
   Impressum (c/o). Accept the Appstore distribution agreement. Payment and the tax interview are
   only for paid apps and in-app items: skip them if the console lets you, else fill them in.
2. **The next release with foss APKs** (any `app-v*` tag after #200), whose GitHub release has
   `hashi-<x.y.z>-foss-arm64-v8a.apk` and `-armeabi-v7a.apk`. The first version is uploaded by
   hand from there.
3. **Add the app by hand** (Developer Console, Apps, "Add New App", Android):
   - **App information**: title `Hashi: Daily Bridges Puzzle` (Play's), category Games, Puzzles
     (or the nearest, e.g. "Puzzles & Trivia"); customer support email hashi@huibers.io, website
     https://hashi.rs, privacy policy https://hashi.rs/datenschutz.
   - **Availability & pricing**: free, all marketplaces; no in-app items.
   - **Upload your app file**: `hashi-<x.y.z>-foss-arm64-v8a.apk`, then the second binary,
     `-armeabi-v7a.apk`. **Apply Amazon DRM: No.** Device support for each binary: only Fire
     tablets (the device rules "exclude Fire TV" and "exclude Amazon Automotive"; the app needs a
     touch screen), and the CPU architecture rule "exclude 32-bit" for the arm64 binary,
     "exclude 64-bit" for the armeabi-v7a one, so that every tablet has exactly one; the console
     shows conflicts until it is so. (Or only the armeabi-v7a binary, without the CPU rule, see
     above.) Language support: the app's 20 languages. Export compliance: confirm it (the app
     only uses the system's standard HTTPS encryption). Note the certificate hashes shown there:
     Amazon's key for the app.
   - **Target your app**: target audience the age groups from 13 on, not the children's (it isn't
     made for children, names are public and an account takes an email, so Amazon's child-directed
     rules would apply otherwise). Content rating, Subject matter: none of
     violence, cartoon violence, drugs, nudity, sex, intolerance, profanity; academic: no.
     Additional information: account creation or other personal information collected **yes**
     (an optional account, the user name, an optional email); advertisements no; gambling no;
     location detection no; user generated content or user to user communication **yes** (names,
     own puzzles and their titles, group names are seen by others; no chat, players can be
     reported and blocked).
   - **User data privacy**: "Does your app collect user data or transfer user data to third
     parties?" **Yes** (collected, nothing transferred to third parties), privacy policy
     https://hashi.rs/datenschutz, and in the questionnaire (as `/datenschutz` and Play's Data
     safety form):
     - User Info: **Name** (the user or guest name, public in the rankings; App functionality),
       **Email Address** (optional: account recovery and the reply address of the contact form;
       App functionality, Developer communications), **User IDs** (the account; App
       functionality).
     - Device or other IDs: **Device or other IDs** (a random ID the app makes, kept only as a
       daily hash; Fraud Prevention, Security & Compliance).
     - App activity: **App interactions** (results, times, moves, hints, stamps, groups, rivals;
       App functionality, Fraud Prevention, Security & Compliance) and **Other user-generated
       content** (own puzzles and their titles, group names, reports and their note, messages of
       the contact form; App functionality, Developer communications).
     - Nothing else: no location, contacts, photos, files, payment or health data, browsing
       history, no advertising ID, no analytics or crash logs.
   - **Appstore details** (English; the other languages come from the listing run, task 5):
     short description: the first paragraph of
     `fastlane/metadata/android/en-US/full_description.txt`; long description: the whole file;
     feature bullets: the first point of each of its four lists; small icon
     `app/store/amazon/small-icon.png`, large icon `public/icon-512.png`, the screenshots
     `en/play-tablet-7/*.png` and the promotional image `en/feature.png` of the artifact
     `store-listing-images` of a "Store listing" run
     (`gh workflow run store-listing.yml -f platform=play -f what=images -f validate_only=true`).
   - **Review**: notes for the testers: "No login needed: the app starts as a guest. The daily
     puzzle, rankings, groups and rivals need an internet connection (our server hashi.rs); Play
     works offline. Free, no ads, no in-app purchases. Source:
     https://github.com/NoelHuibers/Hashiwokakero." Then submit.
4. **The security profile** for the API: Developer Console, Settings (the gear), **API Access**,
   App Submission API, "Create a new security profile" (e.g. "Hashi CI"), save, then on its Web
   Settings the client ID and client secret; back in API Access, select the profile and
   **Attach** it. The app's ID is in Apps, Hashi, "Upload Your App File", Additional information
   (`amzn1.devportal.mobileapp.<...>`). Then:

   ```sh
   gh secret set AMAZON_CLIENT_ID --body 'amzn1.application-oa2-client.<...>'
   gh secret set AMAZON_CLIENT_SECRET   # paste the client secret when asked
   gh secret set AMAZON_APP_ID --body 'amzn1.devportal.mobileapp.<...>'
   ```

   No backup needed: a lost or leaked secret is replaced by a new security profile (and the old
   one detached and deleted), nothing users have depends on it.
5. **The first update through CI**: once the first version is live (while it is in review, the
   job fails saying so), the next release tag uploads; or upload the current release with
   `gh workflow run app-release.yml --ref app-v<x.y.z> -f amazon=true`, which only works for a
   version above the one live. Then the listing in the other languages:
   `gh workflow run store-listing.yml -f platform=amazon -f what=all` (it goes out with the next
   release, or submit it in the Developer Console).

**Review**: Amazon checks every version (policy, and a test on Fire devices), usually within a few
days. The console's app status shows where it is, and Amazon emails the result, with the devices
on which a test failed. Common reasons: the app crashes or misbehaves on a device (assign that
device to no binary, or fix it), a listing or recent changes that name another store, screenshots
that don't show the app, a missing privacy policy. While a version is in review, a release tag's
upload fails with HTTP 412; rerun it with `-f amazon=true` afterwards. A **risk to watch on the
first review**: the Fire tablets' WebView is Amazon's own Chromium build, updated with Fire OS and
not from Google Play, possibly older than the Chrome versions our CSS needs (Tailwind 4 needs
Chrome 111); if the review or a test on a Fire tablet shows broken styling, exclude the affected
(older) tablets in the device support.

## Huawei AppGallery

Each release tag uploads the foss APKs to AppGallery as the packages of the app's next version and
submits it for review: the job "Upload to Huawei AppGallery" of app-release.yml with
`scripts/huawei-appgallery.ts`, through the
[Publishing API](https://developer.huawei.com/consumer/en/doc/AppGallery-connect-Guides/agcapi-publish_api_overview-0000001111685332)
of AppGallery Connect. It is skipped with a notice until the secret below exists, and the API
can't create an app: it is created by hand first (owner tasks).

**Where the store is** (November 2026). AppGallery is the store of Huawei's phones and tablets.
Those sold since late 2019 (Mate 30 on) have no Google services at all, only Huawei's (HMS), so
AppGallery is their main store; older ones have both. Outside China they run EMUI, which is
Android and installs APKs. HarmonyOS NEXT (HarmonyOS 5 and 6), which runs no Android apps at all,
is on Huawei's phones in China only; Huawei has announced pilots in other countries without a
date. Should it come to phones elsewhere, Hashi would need a HarmonyOS app of its own (ArkTS),
which is out of scope here. China itself is left out (below), so for this store HarmonyOS NEXT
doesn't matter today.

**Which build.** The foss APKs: Huawei's newer phones have no Google services, so Play Games
couldn't sign in; the release marks the app as not needing them (`needGms` 0).

**APK, not App Bundle.** AppGallery takes App Bundles only with its App Signing, where Huawei
signs the APKs it makes from the bundle with a key it keeps (its own, or ours if we hand over the
direct key's private key, which we won't). An APK is distributed as we signed it, with the direct
key (`hashi-direct`): the app from AppGallery and the one from F-Droid, our repository, the GitHub
release or the Galaxy Store update each other. The per-ABI APKs already give what a bundle would
save in download size.

**Which APKs.** `arm64-v8a` and `armeabi-v7a`, as one multi-APK version (AppGallery Connect's "App
version (Support multiple APKs)"): AppGallery gives each device the APK of its CPU architecture,
the higher versionCode where both run, so 64-bit phones get the arm64 one (its versionCode ends in
2, the 32-bit one's in 1). Every Huawei phone and tablet is ARM, AppGallery requires a 64-bit
package for phones, and the 32-bit one is for the cheap Huawei and Honor phones that run a 32-bit
Android on a 64-bit chip. Both are bound to the version in one call, the way Mozilla's AppGallery
client (mozapkpublisher) binds several APKs. The repository variable `HUAWEI_APPGALLERY_ABIS` changes the set
(`gh variable set HUAWEI_APPGALLERY_ABIS --body arm64-v8a`), e.g. if AppGallery won't take the
32-bit package next to the 64-bit one. AppGallery keeps at most 300 APKs per app (150 releases
with two each); old packages of draft versions can be deleted in AppGallery Connect.

**Android developer verification**: Huawei's phones without Google services aren't certified
Android devices, so Google's developer verification doesn't apply there; on the older Huawei
phones with Google services it applies as to every other store (see the Galaxy Store's section).

**What a release does:**

1. Finds the app by its package name (`rs.hashi`) and reads its state. It refuses while the app
   is in review, being released or waiting for its removal (the job fails saying so: rerun it
   afterwards, below), and when AppGallery has a newer version; when this version is on sale
   already there is nothing to do.
2. Marks the app as not needing Google services.
3. Uploads each APK (an upload URL per file, with its size and SHA-256, which Huawei's file server
   checks; not for the Chinese mainland), then binds both to the version being prepared.
4. Puts the release notes (`app/store/whatsnew/whatsnew-en-US`) into every language's "new
   features" (at most 500 characters), the English notes for all, as Play shows them. Notes that
   name Google Play, Play Games or the App Store become "Hashi <version>" instead (AppGallery's
   guidelines turn away other platforms' names).
5. Submits the version for review as a full release; after the review it goes live. AppGallery
   parses uploaded APKs for a few minutes and takes one submission every two minutes, so while it
   answers "still parsing" the job tries again every two minutes, for up to ten; after that the
   APKs stay bound and the version is submitted in AppGallery Connect. The repository variable
   `HUAWEI_APPGALLERY_SUBMIT=false` leaves the version unsubmitted instead.

A rerun (or `gh workflow run app-release.yml --ref app-v<x.y.z> -f huawei=true`, which takes the
APKs from the tag's GitHub release and builds nothing; `-f galaxy=true` and `-f amazon=true` next
to it are fine) uploads and binds the APKs again, which replaces the draft's. A pull request's dry
run checks the APKs, the release notes and the key's use with a throwaway key, and calls nobody.

**No phased release.** AppGallery can roll an update out to a percentage of users, but only for
an app already released in full, with every call of the version (packages, texts, submission)
made as a phased one, and it needs a later call to finish it, which nobody would make. For a small
free game, every user gets a fix at once. For a release that should go out by phase, set
`HUAWEI_APPGALLERY_SUBMIT=false` and choose "Phased release" when submitting it in AppGallery
Connect.

**The listing**: `gh workflow run store-listing.yml -f platform=huawei -f what=all` puts the
texts into the version being prepared, in 23 languages (`HUAWEI` in
`scripts/huawei-appgallery.ts`: each of Play's locales that AppGallery has, which is all but fr-CA
and es-US; zh-HK gets zh-TW's texts as on Play): the name, the short description (Play's, at most
80 characters; AppGallery's "brief introduction") and the full description (at most 8000). The
name is the app's own, `Hashi` (its label in the APK), not Play's title: AppGallery's review
wants the name the app shows, at most 30 characters, without keywords or descriptions in it (Review
Guidelines 1.1, 1.2, 1.20). With the images: the icon (`public/icon-512.png`, 512×512, in the
default language, which the others fall back to) and each language's phone screenshots (Play's,
`play-phone`, 1080×1920: AppGallery takes 3 to 8 upright 9:16 screenshots, each side 320 to
3840 px, at most 2 MB; ours are 7 of about 1 MB). Not submitted: the next release submits it with
its packages (or submit it in AppGallery Connect); the details of a released app change only
after a review. `what=text` sends the texts only; `validate_only` only runs the checks, as the
pull requests of the store listing do. A language the app doesn't have yet is added by the
script; should AppGallery refuse that, the job says so, and it is added in AppGallery Connect.

### Owner tasks

1. **A HUAWEI ID and developer account**, free (no sign-up or yearly fee): sign up at
   <https://developer.huawei.com> with hashi@huibers.io, then the console asks for **identity
   verification** as an **individual** developer, country Germany: a scan of the ID card or the
   passport, plus a bank document with the full name in English (bank card, statement or account
   confirmation, valid for at least six more months); Huawei checks it within 1 to 2 working
   days. An individual may publish free apps and games everywhere but the Chinese mainland (there
   games need a publication number and apps an ICP filing and a Chinese entity, as for the Chinese
   stores). Merchant Service and the tax details are only for paid apps and in-app purchases:
   skip them.
2. **The next release with foss APKs** (any `app-v*` tag after #200), whose GitHub release has
   `hashi-<x.y.z>-foss-arm64-v8a.apk` and `-armeabi-v7a.apk`.
3. **Create the app** (AppGallery Connect, Apps and atomic services, Android, "Release" / "New
   app"): package type APK, device Mobile phone, app name `Hashi`, category **Game**, default
   language English (US); don't add it to a project (no Huawei services are used). Then on its
   pages:
   - **App information**: the English brief introduction (`short_description.txt`) and full
     introduction (`full_description.txt`) of `fastlane/metadata/android/en-US`; icon
     `public/icon-512.png`; 3 to 8 screenshots, the `en/play-phone/*.png` of the artifact
     `store-listing-images` of a "Store listing" run
     (`gh workflow run store-listing.yml -f platform=play -f what=images -f validate_only=true`);
     category Game, Puzzle & casual, Puzzle; game type casual.
   - **Version information**: countries/regions **all except the Chinese mainland** (and "new
     countries/regions" on, if offered); open testing No; payment Free, no in-app purchases;
     **content rating**: the questionnaire, answered as the app is: no violence, horror, sexual
     content, drugs, gambling or bad language, no ads, no purchases; players' names are public
     and they can form groups, but there is no free-text chat; not made for children only. Choose
     the youngest rating it offers (3+ is expected; if it asks about user interaction and gives
     more, take what it gives).
   - **Privacy**: privacy policy URL https://hashi.rs/datenschutz, privacy rights (data subject)
     URL https://hashi.rs/account (where an account is deleted). Privacy tags: collects personal
     data **Yes**, as in Play's Data safety form: the user or guest name (public in the rankings)
     and the optional email address (account, account recovery, the contact form's reply);
     results, times, moves, hints, stamps, groups and rivals (app functionality, fair rankings);
     own puzzles and their titles, group names, reports and contact messages (user content); a
     random device ID kept only as a daily hash (fraud prevention). No location, contacts,
     photos, files, payment or health data, no advertising ID, no analytics or crash logs.
   - **Generative AI service**: Not involved (the app has no AI features).
   - **Copyright**: nothing (copyright certificates and the software copyright document are only
     needed for the Chinese mainland).
   - **For reviewer**: "No login needed: the app starts as a guest. The daily puzzle, rankings,
     groups and rivals need an internet connection (our server hashi.rs); Play works offline.
     Free, no ads, no in-app purchases. Source: https://github.com/NoelHuibers/Hashiwokakero."
   - Packages: none yet (task 5 uploads them), or upload both APKs by hand ("Manage packages",
     "Upload", "+" for the second) and submit, then skip task 5's first run.
4. **The Service Account** for the API: AppGallery Connect, Users and permissions, API key,
   **Connect API**, tab "Service Account", "Create": a name (e.g. "Hashi CI"), type
   **Developer-level**, role **App administrator**. The credential file `<...>private.json`
   downloads once (Huawei doesn't keep its private key). Then:

   ```sh
   gh secret set HUAWEI_SERVICE_ACCOUNT_JSON < ~/Downloads/<the credential file>
   ```

   and delete the downloaded file. No backup needed: a lost or leaked key is replaced by a new key
   of the Service Account (and the old key deleted there), nothing users have depends on it. Use a
   Service Account, not an "API client": Huawei recommends them and is replacing API clients.
5. **The first upload through CI**: `gh workflow run app-release.yml --ref app-v<x.y.z> -f
   huawei=true` uploads the APKs into the version being prepared and submits it. If something
   the review needs is still missing, the job fails with Huawei's message ("pre-check"): fill it
   in AppGallery Connect and submit there (the APKs are already bound). Then the listing in the
   other languages: `gh workflow run store-listing.yml -f platform=huawei -f what=all` (it goes
   out with the next release, or submit it in AppGallery Connect). From then on each release tag
   uploads and submits by itself.

**Review**: AppGallery reviews every version, usually within 1 to 3 working days; AppGallery
Connect shows the status and Huawei emails the result. It also checks each country against local
rules and may drop some from the release. Common reasons for a rejection: an app name or texts
that name other platforms, a name with keywords, screenshots that don't show the app, a missing
privacy policy, or the app failing on some device. While a version is in review, a release tag's
upload fails saying so; rerun it with `-f huawei=true` afterwards (or cancel the review in
AppGallery Connect first). A **risk to watch on the first review**: Huawei's phones without Google
services get their WebView from Huawei rather than from Google Play, possibly older than the
Chrome versions our CSS needs (Tailwind 4 needs Chrome 111); if the review or a Huawei phone shows
broken styling, that's where to look.

## Desktop builds

Each release tag also builds the app for Windows, macOS and Linux and puts the installers on the
tag's GitHub release: the five jobs "Build desktop" of app-release.yml, from the same web bundle
and the same Tauri crate as the phones. They are the source of the desktop stores that follow
(7 Microsoft Store, 8 Flathub and Snap, 9 itch.io, 10 Steam), and a download for anyone who
installs by hand. What the app does differently on a computer is in
[app/README.md](../app/README.md#desktop).

| System | Architectures | Files on the release | Runner |
|--------|---------------|----------------------|--------|
| Windows 10 and 11 | x64 | `hashi-<v>-windows-x64-setup.exe` (NSIS), `hashi-<v>-windows-x64.msi` | `windows-2025` |
| Windows on ARM | arm64 | `hashi-<v>-windows-arm64-setup.exe` (NSIS) | `windows-2025`, cross-compiled |
| macOS 11 and newer | universal (Apple silicon and Intel) | `hashi-<v>-macos-universal.dmg`, `hashi-<v>-macos-universal.app.zip` | `macos-26` |
| Linux | x64, arm64 | `hashi-<v>-linux-<arch>.AppImage`, `.deb`, `.rpm` | `ubuntu-22.04`, `ubuntu-22.04-arm` |

**Why these.**

- **Windows**: the NSIS installer (`setup.exe`) is what people expect from a download, installs per
  user without admin rights (`installMode: currentUser`), in English or German after the system's
  language, and fetches WebView2 should a Windows 10 lack it. The MSI is for installing on many
  computers at once (Intune, Group Policy, `msiexec /quiet`). The Microsoft Store gets the same
  program as an MSIX bundle ([Microsoft Store](#microsoft-store)). Windows on ARM (Snapdragon laptops) gets its own
  native installer: cross-compiled on the x64 runner, whose Visual Studio has the ARM64 tools, so
  it costs no extra runner; Tauri makes MSIs (WiX 3) for x86 and x64 only, so it is NSIS only.
- **macOS**: one universal app for Apple silicon and Intel, as a disk image (drag to
  Applications) and as a zip of the app (for itch.io and Steam, which unpack it themselves; `ditto`
  keeps its signature). macOS 11 at least: the page runs in the system's Safari (`WKWebView`), and
  the CSS needs Safari 16.4, which macOS 11 Big Sur can still install (a Mac on 11 or 12 with an
  older Safari shows the page unstyled until Safari is updated). `macos-26`, GitHub's regular Apple
  silicon image: the desktop app needs none of the iOS SDK that `xcode-27` is there for.
- **Linux**: an AppImage that runs on most distributions without installing (it brings WebKitGTK,
  GTK and GStreamer for the sounds; about 110 MB), and `.deb` and `.rpm` packages (about 23 MB)
  that use the system's WebKitGTK 4.1 (Debian 12, Ubuntu 22.04, Fedora 37 and newer). Built on
  Ubuntu 22.04, the oldest image GitHub has, so they run on systems with its glibc 2.35 or newer.
  arm64 too (Raspberry Pi 5, ARM laptops): GitHub's arm64 runners are free for public
  repositories. Flathub and Snap (step 8) build from source in their own runtimes and don't need
  these.

**How a release checks them**: the Debian package's name, version and architecture and its
desktop entry; the Mac app's bundle ID `rs.hashi`, version, both architectures, its signature,
and once signed the Developer ID, the notarization ticket and Gatekeeper's verdict. The sizes are
in each job's summary. A failed desktop job holds up nothing else (Play, TestFlight, the APKs);
`gh workflow run app-release.yml --ref app-v<x.y.z> -f desktop=true` builds the five again and
replaces the desktop files on the tag's release (and its notes). A pull request that changes
`tauri.conf.json` (the version too) or the icons runs all five as a dry run; "Build app (desktop)"
in ci.yml builds and starts the Linux app for pull requests that change the desktop's files.

### Signing

**macOS: Developer ID and notarization.** An app from outside the Mac App Store opens without a
warning only when it is signed with a Developer ID Application certificate and notarized by
Apple. Both come with the Apple Developer Program the owner already pays for. The release does it
through Tauri once the certificate is in the secrets: Tauri imports it into a keychain of its own
(`APPLE_CERTIFICATE`), signs with the hardened runtime, and notarizes and staples the app with the
App Store Connect API key the iOS release already has (`APPLE_API_KEY_ID`, `APPLE_API_ISSUER`,
`APPLE_API_KEY_P8`; notarizing needs the Developer role or higher, it is Admin). New secrets:

- `APPLE_DEVELOPER_ID_CERT_BASE64`: the certificate with its private key, a `.p12`, base64.
- `APPLE_DEVELOPER_ID_CERT_PASSWORD`: the `.p12`'s password.

Without them the Mac app is signed ad hoc (it runs on Apple silicon, but has no identity), with a
notice in the run: macOS then says it can't check the app for malware and only opens it after
System Settings, Privacy & Security, "Open Anyway" (since macOS 15 the Control-click way is gone).

**Windows: unsigned for now (an owner decision).** Unsigned, SmartScreen shows "Windows protected
your PC" for a new download ("More info", "Run anyway"), and the publisher is "Unknown". The
options, as of November 2026:

| Option | Cost | Who | In CI | Notes |
|--------|------|-----|-------|-------|
| Unsigned | free | | nothing to do | SmartScreen warning on every new version |
| Microsoft Store with MSIX (step 7) | free (the Store account is free for individuals) | anyone | the Store signs the package | No warning for Store users. Store submissions as EXE or MSI instead must be signed with a trusted certificate ourselves |
| [SignPath Foundation](https://signpath.org) | free for open source | OSI license (MIT is), public repository, reviewed application | their GitHub action signs the build's artifacts | The certificate is the Foundation's: the publisher shows "SignPath Foundation", not Noel Huibers |
| [Azure Artifact Signing](https://azure.microsoft.com/products/artifact-signing) (was Trusted Signing) | $9.99 a month (5,000 signatures) | individuals only in the USA and Canada so far, organizations in the EU | `signCommand` with Microsoft's signing tool and an Azure app's secrets | Not open to an individual in Germany today |
| [Certum Open Source Code Signing](https://shop.certum.eu/open-source-code-signing-on-simplysign.html) (cloud) | about €49 the first year | individuals with open source projects | awkward: the key is in Certum's SimplySign cloud, each session needs its one-time code | Publisher "Open Source Developer, Noel Huibers"; reputation still builds slowly |
| An OV certificate from another CA | about €200 to €500 a year | anyone, identity checked | the key must be in an HSM or a cloud signing service since 2023 | No faster SmartScreen reputation than the above |

The suggestion: stay unsigned on GitHub, where few people will install from, and reach Windows
users through the Microsoft Store's MSIX (step 7), which needs no certificate. Should direct
downloads matter later, try the SignPath Foundation first, then Certum. Signing would hook into
`bundle.windows.signCommand` (Tauri runs it on every binary and installer), with its secrets
skipped like the others while they are missing.

### Updates

**No auto-updater** (Tauri's updater plugin) for now:

- Most desktop users will come through stores that update the app themselves (Microsoft Store,
  Flathub, Snap, Steam, the itch app), and their rules forbid an app that updates itself. The
  updater would have to be off in their builds, so every system would need two builds.
- It needs an updater key pair that can never be lost (without it, installed apps take no update
  ever again), a `latest.json` per release, and signed builds on Windows and macOS for a smooth
  update (an unsigned update brings the SmartScreen warning back each time).
- What the GitHub downloads have instead: when the server turns an old version away
  (`HASHI_APP_MIN_VERSION`, docs/mobile.md), the update toast's button opens
  `https://github.com/NoelHuibers/Hashiwokakero/releases` (`api::app::store_url` for the
  desktops; the Microsoft Store's app opens its page in the Store). The server functions are a
  stable contract, so that should be rare.

Adding it later: `tauri-plugin-updater` in the desktop's dependencies, `bundle.createUpdaterArtifacts`,
a key pair from `cargo tauri signer generate` (the private key and its password as secrets,
`TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, the public key in
`plugins.updater.pubkey`), the release writing `latest.json` onto the GitHub release, and a build
flag that keeps it out of the store builds.

### Owner tasks

1. **The Developer ID certificate** (only the Account Holder can make one): on a Mac, Xcode,
   Settings, Accounts, the team Q35TCGM5PW, "Manage Certificates…", "+", "Developer ID
   Application"; or on <https://developer.apple.com/account/resources/certificates>, "+",
   "Developer ID Application" (G2 Sub-CA) with a certificate signing request from Keychain Access
   (Certificate Assistant, "Request a Certificate From a Certificate Authority…", saved to disk).
   Then in Keychain Access, "My Certificates", the new "Developer ID Application: Noel Huibers
   (Q35TCGM5PW)" with its key, "Export…" as a `.p12` with a strong password, and:

   ```sh
   base64 -i DeveloperIDApplication.p12 | gh secret set APPLE_DEVELOPER_ID_CERT_BASE64
   gh secret set APPLE_DEVELOPER_ID_CERT_PASSWORD     # the .p12's password, typed in
   ```

   Keep the `.p12` and its password offline (password manager), then delete the file. Apple allows
   only a few Developer ID certificates per team, and never revoke it: a revoked certificate stops
   every app signed with it from opening (one that merely expires keeps working, notarized apps
   carry their ticket). Then sign the newest release's Mac app: `gh workflow run app-release.yml
   --ref app-v<x.y.z> -f desktop=true` (the run's macOS summary says "Signed by Developer ID
   Application: … and notarized").
2. **Windows signing: decide** between the options above (nothing to do for unsigned).
3. **The first release with desktop builds**: the next `app-v*` tag. Try the files: the setup on a
   Windows PC (SmartScreen's "More info", "Run anyway"), the disk image on a Mac (until task 1,
   "Open Anyway" in System Settings), the AppImage or `.deb` on a Linux PC. The app should start on
   the daily, sign in like the phone app, and play offline.
4. **The iPhone app on Apple silicon Macs** (App Store Connect, the app, Pricing and Availability,
   "iPhone and iPad Apps on Apple Silicon Macs"): leave it off now that there is a Mac app. Both
   have the bundle ID `rs.hashi`, and two apps of one ID on one Mac confuse Launch Services.

**Risks**: the Windows and macOS builds have only run in CI (the Linux app was also started and
used locally: the page, the offline puzzle generator, the save dialog, the theme, Alt+←). The
arm64 builds are only built, never started. Notarization is untested until the certificate
exists. Text sharing on the desktop relies on each `WebView`'s share sheet or clipboard.

## Microsoft Store

Each release tag packs the Windows builds into one MSIX bundle (x64 and arm64) and uploads it as
the package of the app's next submission, which it commits for Microsoft's certification: the
jobs "Microsoft Store package" and "Upload to Microsoft Store" of app-release.yml, with
`scripts/microsoft-store.ts`, through the
[Microsoft Store submission API](https://learn.microsoft.com/windows/uwp/monetize/create-and-manage-submissions-using-windows-store-services).
The package needs the Store's identity of the app (repository variables), the upload the
secrets of an Entra application; until they exist the package is built with a placeholder
identity for the checks and the upload is skipped with a notice. The API only makes the
submissions after the first, so the first one is made by hand in Partner Center (owner tasks).

**Where the store is** (November 2026). The Microsoft Store app is on every Windows 10 and 11 PC,
and <https://apps.microsoft.com> links into it. A developer account is free for individuals
since September 2025 (and for companies since October 2026), with an identity check of an ID
document and a selfie.

**MSIX, not the EXE/MSI route.** The Store takes desktop apps either as MSIX packages or as a link
to an EXE or MSI installer. For us it has to be MSIX:

- **Games must be MSIX.** The Store Policies (version 7.20, 2026-09-14), 10.2.5: a game has to be
  submitted in a package type the Store ingests and be installed and updated only through the
  Store; the EXE/MSI route (10.2.9) is for "non-gaming products" only.
- **EXE/MSI would need a certificate we don't have.** The installer and every program in it must
  be signed with a certificate that chains to Microsoft's Trusted Root Program (still so in
  November 2026; self-signed is refused, and the Store doesn't sign EXE or MSI), see the table in
  [Signing](#signing). Every release would also need a new versioned HTTPS URL entered in Partner
  Center, and the app would have to update itself.
- **MSIX is signed by Microsoft.** The package is uploaded unsigned; after certification the Store
  signs it with Microsoft's certificate, so there is no SmartScreen warning, and the Store
  installs, updates and removes it cleanly.

**How the package is made.** Tauri has no MSIX target (tauri-apps/tauri#4818 is still open, and
Tauri's Store guide covers only the EXE/MSI route). Microsoft's new winapp CLI can pack a Tauri
app (`winapp pack`, a guide for Tauri), but it is a public preview (0.8); the Windows SDK's MakePri
and MakeAppx, which it calls, are on GitHub's Windows runners already and do the same:

1. Each Windows job of "Build desktop" puts the release build's `hashi.exe`, the program of the
   NSIS and MSI installers (one build for all three), into a package folder with its manifest and
   logos (`node scripts/microsoft-store.ts layout`), indexes the logos' sizes with MakePri
   (`resources.pri`) and packs it with MakeAppx, which checks the manifest against Windows'
   schema: `hashi-<v>-windows-<arch>.msix`.
2. "Microsoft Store package" bundles both into `hashi-<v>-windows-store.msixbundle` (version
   x.y.z.0), checks the bundle's and each package's manifest (the identity, the version, x64 and
   arm64, full trust, the program, the languages), then signs a copy with a throwaway certificate
   of the package's publisher that only the runner trusts, installs it, starts the app from the
   Start menu entry the package made and checks that it shows its window and runs from the
   package (a screenshot is the artifact `msstore-smoke`). Built with the Store's identity, the
   bundle goes onto the GitHub release (like the App Bundle and the IPA it doesn't install by
   hand there) and to the Store.

**The manifest** (`manifest` in the script):

- **Identity** from Partner Center's Product identity page: `Package/Identity/Name`,
  `Package/Identity/Publisher` (`CN=<...>`), `Package/Properties/PublisherDisplayName`, and the
  reserved name as the display name. The repository variables `MS_STORE_IDENTITY_NAME`,
  `MS_STORE_PUBLISHER`, `MS_STORE_PUBLISHER_DISPLAY_NAME` (default "Noel Huibers") and
  `MS_STORE_DISPLAY_NAME` (default "Hashi"); they aren't secret. The upload compares the bundle's
  identity with the app's in Partner Center before it uploads anything.
- **Version** x.y.z.0: the fourth part is the Store's and must be 0.
- **Windows 10 1809 and newer** (`Windows.Desktop`, MinVersion 10.0.17763.0, the oldest Windows 10
  still supported, LTSC 2019; tested up to 10.0.26100.0, Windows 11 24H2), x64 and arm64.
- **Capabilities**: `internetClient` (hashi.rs) and `runFullTrust`, which every desktop program
  packed as MSIX needs. `runFullTrust` is a restricted capability: the first submission says why
  (owner tasks), and the certification checks it.
- **Languages**: the app's 20 (en-US first, the default), which the Store shows as the app's
  languages.
- **Logos**: the icons of `app/src-tauri/icons` (`cargo tauri icon`): the 44 px one with the
  target sizes 32, 64 and 256 (and their unplated variants, for Windows 11's taskbar), the 71 and
  150 px tiles of Windows 10's Start menu, and the 50 px Store logo.

**WebView2.** Not in the package: an MSIX can't run WebView2's installer, as the NSIS setup does.
Windows 11 has it, and so has nearly every Windows 10 (Microsoft rolled it out); where it is
missing, Tauri shows a dialog that links to its download page (its release builds on Windows do
that themselves). The Store's policies don't forbid an app that needs it.

**The app's data.** Windows keeps what a packaged app writes into `%LOCALAPPDATA%` in the
package's own folder: the Store's app has its own local storage (its own guest or session), not
shared with an NSIS install on the same PC, and it goes when the app is uninstalled. A player
signs in to have the same account in both.

**Updates.** The Store updates the app; the app never updates itself (it has no updater, see
[Updates](#updates)). When the server turns an old version away, the update toast links to the
app's page in the Store, `https://apps.microsoft.com/detail/<Store ID>`, which opens the Store
app: the page tells it is the Store's app because the plugin reports the desktop `msstore`
instead of `windows` (the program runs from a package, `AppxManifest.xml` lies beside it), which
is also the platform of `x-hashi-app` (the feedback form shows it) ([app/README.md](../app/README.md#desktop)).
The Store ID comes from the repository variable `MS_STORE_APP_ID` when the release builds the
web bundle. Linking the GitHub releases there, as for the NSIS app, would put a second Hashi
next to it.

**What a release does** (`release` in the script):

1. Reads the app (the Store ID, `MS_STORE_APP_ID`): its identity, which the bundle has to have,
   and its published submission. When that has this version already there is nothing to do; when
   it has a newer one it refuses.
2. Takes the app's draft submission (one a run before or Partner Center left), or makes a new
   one, a copy of the published one. It refuses while a submission is in certification or being
   published (rerun it afterwards, below).
3. Makes the bundle the submission's only package (the older ones are deleted from it), and puts
   the release notes (`app/store/whatsnew/whatsnew-en-US`, at most 1500 characters) into every
   language's "What's new"; notes that name Google Play, Play Games or the App Store become
   "Hashi <version>" instead (the 1.1 notes do).
4. Uploads the bundle, zipped, to the submission's upload URL (an Azure blob).
5. Commits the submission, which sends it to certification, and waits up to 20 minutes for
   Microsoft to take it (or to name what is wrong). It goes live after the certification,
   usually within three working days. The repository variable `MS_STORE_SUBMIT=false` leaves the
   submission as a draft for Partner Center instead. No gradual rollout: every player gets a fix
   at once.

A rerun (or `gh workflow run app-release.yml --ref app-v<x.y.z> -f msstore=true`, which takes the
bundle from the tag's GitHub release and builds nothing; the Android stores' inputs next to it
are fine) uploads the bundle into the draft again. `-f desktop=true` builds the installers and
the bundle of a tag again, e.g. once the identity variables are set. A pull request's dry run
(any change to `scripts/microsoft-store.ts`, `tauri.conf.json` or the icons) builds, checks,
installs and starts the bundle, and calls nobody. Don't change a submission the API made in
Partner Center: the API can't change or commit it afterwards (delete it there and rerun).

**The listing**: `gh workflow run store-listing.yml -f platform=microsoft -f what=all` puts the
texts of Google Play's listing into the app's next submission (the draft, or a new one), in 22
languages (`MICROSOFT` in the script: the app's 20, and Play's European Portuguese and Latin
American Spanish as pt-PT and es-MX): the description (at most 10,000 characters), the short
description (Play's, at most 1000) and the features (the first point of each of the
description's lists, at most 20 of 200). The title stays the one Partner Center has (the
reserved name); a new language gets the app's name. Keywords and "What's new" stay as they are.
With the images: each language's desktop screenshots, which replace the Store's.

The screenshots are a new frame of the store images (`desktop` in `store/capture/devices.ts`):
16:9, 1920 × 1080, the desktop app's window with its title bar, the page in the website's desktop
layout at 1280 × 800 (twice), under the same captions as the phones'. The Store takes desktop
screenshots from 1366 × 768, up to 10. Play's tablet images (portrait, 1440 × 2560) would have
passed too, but would show a phone's layout in a frame of an iPad on a Windows app's page; Steam
(step 10) wants 16:9 at 1920 × 1080 as well, and itch.io (step 9) takes them too. Not committed:
the next release commits it with its package (or submit it in Partner Center). A pull request of
the store listing checks the texts and that run's images against the Store's rules.

The privacy policy URL, the website and the support contact are no longer set through the API:
they are entered once on the app's Properties page (owner tasks), as are the category, the age
rating and the markets.

### Owner tasks

1. **The developer account**, free: start at <https://storedeveloper.microsoft.com> (other entry
   points still show the old paid sign-up), sign in with a personal Microsoft account (an
   individual account can't use a work account), account type **Individual**, country Germany,
   publisher display name **Noel Huibers** (what the Store shows as the developer; if another one
   is chosen: `gh variable set MS_STORE_PUBLISHER_DISPLAY_NAME --body '<it>'`), contact
   hashi@huibers.io. Then the identity check: a photo of the ID card or passport and a selfie,
   taken with the phone (the page shows a QR code). Microsoft checks it, usually within a few
   days.
2. **Reserve the name**: Partner Center, Apps and games, "New product", "MSIX or PWA app", name
   **Hashi**. If it is taken: "Hashi: Daily Bridges Puzzle" (Play's title) or "Hashi Bridges", and
   `gh variable set MS_STORE_DISPLAY_NAME --body '<the name>'`.
3. **The identity into the repository variables**: the app, Product management, Product
   identity:

   ```sh
   gh variable set MS_STORE_IDENTITY_NAME --body '<Package/Identity/Name>'
   gh variable set MS_STORE_PUBLISHER --body '<Package/Identity/Publisher, CN=...>'
   gh variable set MS_STORE_APP_ID --body '<Store ID, 9...>'
   ```

4. **A bundle with that identity**: the next release tag, or for the newest one
   `gh workflow run app-release.yml --ref app-v<x.y.z> -f desktop=true`. Its GitHub release then
   has `hashi-<x.y.z>-windows-store.msixbundle`. (The web bundle of a release before task 3 has
   no Store ID, so its Store app's update toast opens hashi.rs; the next release fixes that.)
5. **The first submission**, by hand (the API can't make it), on the app's pages in Partner
   Center:
   - **Pricing and availability**: all markets, Free, public, no free trial.
   - **Properties**: category **Games**, genre **Puzzle & trivia** (Partner Center writes "Puzzle
     + trivia"; a game can't leave the Games category later); privacy policy URL
     **https://hashi.rs/datenschutz**, website https://hashi.rs, support contact
     https://hashi.rs/kontakt; it accesses personal information: Yes (the name, the optional email
     address, results; as Play's Data safety form). Game settings: no Xbox services, not
     multiplayer; system requirements: nothing beyond the defaults (works with mouse, keyboard
     and touch).
   - **Age ratings**: the IARC questionnaire, answered as the app is: no violence, horror, sexual
     content, drugs, gambling or bad language, no ads, no purchases; players' names are public and
     they can form groups, but there is no free-text chat and no location is shared. Or enter the
     IARC rating ID of Google Play's rating, which Partner Center takes instead.
   - **Packages**: upload the bundle of task 4 (it lists x64 and arm64, Windows 10/11 Desktop).
   - **Store listings**: English (United States): description, short description and features
     from `fastlane/metadata/android/en-US` as above, screenshots `en/desktop/*.png` of the
     artifact `store-listing-images` of a "Store listing" run (`gh workflow run store-listing.yml
     -f platform=microsoft -f what=images -f validate_only=true`). The other languages come with
     task 7.
   - **Submission options**, why it needs `runFullTrust`: "Hashi is a Win32 desktop program
     (Tauri, with WebView2) packaged as MSIX; runFullTrust is what a packaged desktop program
     runs with. It uses no other restricted capability." Notes for certification: "No login
     needed: the app starts as a guest. The daily puzzle, rankings, groups and rivals need an
     internet connection (our server hashi.rs); Play works offline. Free, no ads, no in-app
     purchases. Source: https://github.com/NoelHuibers/Hashiwokakero."
   - Submit. The certification takes up to three working days.
6. **The Entra application for the API**: Partner Center, Account settings, User management,
   Microsoft Entra applications (should it ask for a tenant first: Account settings, Tenants,
   create a new one), "Create Microsoft Entra application", role **Manager**; on its page "Add new key". The tenant ID, the client ID and
   the key are shown there (the key only once):

   ```sh
   gh secret set MS_STORE_TENANT_ID       # the tenant ID, typed in
   gh secret set MS_STORE_CLIENT_ID       # the client ID
   gh secret set MS_STORE_CLIENT_SECRET   # the key
   ```

   No backup needed: a lost or leaked key is replaced by a new one of the application (and the
   old one removed there). The keys expire (Partner Center shows when, at most two years): add
   a new one and set the secret again before that.
7. **From then on CI**: each release tag uploads and commits by itself. The listing in the other
   languages: `gh workflow run store-listing.yml -f platform=microsoft -f what=all` (it goes out
   with the next release, or submit it in Partner Center). A release made before task 6:
   `gh workflow run app-release.yml --ref app-v<x.y.z> -f msstore=true`.

**Certification**: Microsoft tests every submission, usually within three working days, and
mails the result; Partner Center shows the report. While one is in certification, a release
tag's upload fails saying so; rerun it with `-f msstore=true` afterwards. **Risks**:

- The MSIX app has only been started on the runner (Windows Server 2025, x64, unsigned copy with
  a test certificate), not on Windows 10 or 11, and the arm64 package not at all.
- The Windows App Certification Kit's "Package sanity test" has been reported to flag fresh Tauri
  2 MSIX packages (tauri-apps/tauri#14935, WebView2 suspected); whether the Store's certification
  minds is unknown. Watch the first certification report.
- The API is untested against Microsoft (no account): the contract comes from Microsoft's
  reference (fetched 2026-11-11), and the upload's headers from Azure Storage's. Whether an
  individual account can use the API through an Entra application isn't stated anywhere; if not,
  upload each bundle by hand (it is on the GitHub release).
- Play's description says the offline puzzles are "made on your phone" (one line); the Store's
  listing shows that on Windows too until there are Windows texts of their own.
