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
