# Offline downloads (plan)

Goal: download playlists and albums and play them with no network, the way
Spotify's own client does. Downloads stay Spotify's encrypted files and are
only playable inside Spotifast.

## Ground rules (decided 2026-10-22)

- Quality is 320 kbps Ogg Vorbis, the best librespot is given. Lossless FLAC
  keys only come through PlayPlay, Spotify's white-box DRM
  (librespot-org/librespot#1583). Getting them means extracting secrets from
  Spotify's binary, so lossless is out until librespot gains lawful support.
- Files on disk are the encrypted blobs exactly as the CDN serves them. The
  per-track AES keys are sealed with a master key kept in the OS keychain
  (same store `src/credentials.rs` already uses).
- Premium only. No downloads, and existing ones locked, if the session says
  the account is not Premium.
- 30-day check-in: downloads stop playing 30 days after the last successful
  online sign-in.
- Signing out deletes every download, the sealed keys and the master key.
- librespot changes live on github.com/Andromeadus/librespot, branch
  `offline`, started from crmne/librespot 8c68baf (published 2026-10-22).

## Running the dev build

Needs `cmake` (Homebrew) for projectM. Portable mode is Windows-only, so
on macOS the dev build gets its own home in `../spotifast-dev-home`, beside
the checkout and out of `target/`, so cleaning the build never deletes its
downloads or sign-in, which
keeps it apart from an installed Spotifast (data and single-instance lock).
Link the real keychain in once so sign-ins are remembered:

```bash
mkdir -p ../spotifast-dev-home/Library ../spotifast-dev-home/tmp && ln -sfn ~/Library/Keychains ../spotifast-dev-home/Library/Keychains
```

```bash
HOME=$PWD/../spotifast-dev-home TMPDIR=$PWD/../spotifast-dev-home/tmp/ ./target/debug/spotifast --verbose
```

Sign the build with a stable identity before launching it, or macOS asks
for the keychain again after every rebuild (an ad-hoc signature changes
each time, and "Always Allow" trusts only that build). Any Apple
Development identity works:

```bash
codesign -f -s "Apple Development" --identifier rocks.spotifast.dev target/debug/spotifast
```

## How it works today

- Every local play goes through `Spirc` (Spotify Connect) in
  `src/player.rs`. Spirc needs a connected session and Spotify's servers to
  hold the queue, so nothing plays offline.
- librespot's `Player::load_remote_track` resolves metadata over the
  session, opens the encrypted file (from librespot's cache if present),
  asks the session for the audio key, then decrypts while decoding.
- librespot's audio cache keeps encrypted files but evicts under its size
  limit, and never stores keys.
- The fork already plays `spotify:local:` files with no session
  (`load_local_track`), which is the shape offline playback needs.

## Steps

1. **Download store (Spotifast only).** On the backend runtime: resolve
   each track's 320 kbps file, read the full encrypted stream with
   `AudioFile::open`, request the key, write the blob plus a sealed key and
   the track's metadata into `<cache>/offline/`. Index file is atomically
   written like the other state files. Survives librespot cache eviction.
2. **Offline track source (librespot fork).** Let `Player` take an optional
   source it asks first: given a track id it returns the stored metadata,
   file and key, and the player decrypts and decodes it with no session.
   Used online too, so downloaded songs never re-stream.
3. **Offline playback mode (Spotifast).** When the session cannot connect,
   drive `Player` directly with a local queue instead of Spirc. Only
   downloaded songs are playable; the rest show greyed out.
   Mapped 2026-10-22; it splits into four parts:
   - **3a Offline start.** Today a cold start with no network never leaves
     the "Connecting to Spotify" card: the backend retries `/me` forever
     and every account cache (playlist list, Home, Liked Songs) waits for
     that verified account. Use the account the keychain's playback grant
     and the saved rootlist already name, load the caches, keep retrying
     `/me` behind it, and show the app in its existing offline look.
   - **3b Offline player.** `start_player` over an unconnected
     `Session::new` gives a player with the usual `LocalState` events and
     no Spirc. The backend drives it from a plain list of downloaded track
     URIs (the app sends them, built from the page's rows), advancing on
     `EndOfTrack`, which nothing handles today because Spirc does. Next,
     Previous, Seek, Pause, Shuffle and Repeat act on that list.
   - **3c Playable rows.** Offline, a cached playlist's rows show without
     a fresh header, and only downloaded rows are playable (the same seam
     local files use: `row_playable`, `can_queue`).
   - **3d Back online.** The offline player becomes the kept player that
     the next `Engine::connect` already carries over, and the current
     song, position and list are loaded into Spirc as a normal resume.
   Queue rules (`docs/_reference/queue.md`) that need the Web API, such as
   restoring a saved queue or queueing an album page that is not fully
   loaded, stay unavailable offline.
4. **UI.** A Download toggle on playlist and album pages, progress, a
   downloaded marker on rows, and a Downloads view in the library.
5. **Rules.** Premium check, 30-day check-in, wipe on sign-out.

Steps 1 and 2 are testable without going offline. Step 3 is the large one.

## Status

- Step 1 done: `src/offline.rs` and the `offline-key` credential slot.
  Downloads land in `<state>/offline/`. Try it against a signed-in dev
  copy (macOS asks once to let the probe use the keychain items):

  ```bash
  HOME=$PWD/../spotifast-dev-home TMPDIR=$PWD/../spotifast-dev-home/tmp/ cargo run --example offline_probe -- spotify:track:4uLU6hMCjMI75M1A2tKUQC
  ```

- Step 2 done: librespot `offline` branch adds `OfflineTracks` and
  `Player::set_offline_tracks`; the player asks it before the network,
  online or not. The backend opens the store when it restores sign-ins
  and hands it to every engine. A song is kept under the URI asked for and
  the one Spotify relinks it to, since the app plays the relinked one.
- The store is asked for again, from 5 s up to once a minute, until the
  keychain answers (a locked keychain, or macOS asking about a rebuilt
  binary), and no longer after sign-out.
- librespot comes from github.com/Andromeadus/librespot, branch `offline`,
  pinned by commit in `Cargo.toml`. The local clone at `../librespot` has
  that branch with a `fork` remote: commit there, push, then bump the rev.
- Step 3a done: after each verified sign-in the profile is kept in
  `<cache>/library/profile.json`; a cold start that cannot reach Spotify
  at all (a network error, not a rate limit) uses it, so the library
  shows offline. Sign-out removes it with the library cache.
- Step 4 done (playlists and albums): the Download button, a ring that
  fills while songs arrive, a spinner on the song fetching, green arrows on
  kept songs, and a re-read of every download whenever playback connects.
  Liked Songs download too (added 2026-10-23), read over the session
  from the account's `collection` set, newest first.
- Steps 3b and 3c done (tested 2026-10-23, macOS): with no engine, the
  backend plays downloads on a player over an unconnected session, keeps
  the play order, and moves on at each song's end. Offline, a play goes
  to it at once; Pause, Next, Previous, seek, volume, Shuffle and Repeat
  work; the queue does not. When macOS reports the network gone, the
  Connect engine is ended at once, as a dropped session is, and a
  downloaded song playing carries on offline in the list it was started
  from. A Downloads page lists the kept playlists and albums, and offline
  a kept playlist's or album's own page opens as its downloaded songs.
  Elsewhere offline, only downloaded rows play.
- Step 3d done: when the engine connects again, the offline song moves to
  Spotify Connect where it had got to.
- Fixed 2026-10-23: Connect took about 75 s to take over after Wi-Fi came
  back, waiting out a retry backoff that grew offline. The network
  returning now starts the backoff over and connects at once.
- Step 5 done: the sealed index records when the playback session last
  reached Spotify; offline, downloads stop playing 30 days after that and
  the pages say to go online. Without Premium they do not play and the
  pages say so. Signing out revokes the offline key and deletes the
  downloads folder; a new key also clears files an old one left.
  The check-in trusts the computer's clock.
- Offline pages: a playlist's page takes its name and version from the
  cached library list, so its cached rows show; each row says for itself
  whether it plays (downloaded ones do). A song played from a playlist not
  downloaded as a whole plays with that playlist's other downloaded songs.
  Album pages have no cache, so only downloaded albums open offline.
- An offline cold start took about 13 s to leave the Connecting card
  (2026-10-22, macOS). When macOS already reports no network as the
  account check starts, the remembered profile is now used at once, with
  no request to wait on (untested offline; if reachability arrives later,
  the first failed request still decides).
- Linux and Windows (not tested on either; built and unit tested on
  macOS): the system's word on the network is not used, so a playback
  connection that cannot reach Spotify at all (a lookup or connect error)
  marks Spotifast offline, retries every 15 s, and the first connection
  that gets through marks it online again. Until playback has tried,
  `network_reachable` stays true, so anything retried every frame still
  needs its own guard.
- `packaging/test-launchers.py` fails on macOS (GNU `install -D` and
  `sed -i`); it is a Linux packaging check.
