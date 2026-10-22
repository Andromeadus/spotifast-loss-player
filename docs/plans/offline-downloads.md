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
on macOS the dev build gets its own home under `target/dev-home`, which
keeps it apart from an installed Spotifast (data and single-instance lock).
Link the real keychain in once so sign-ins are remembered:

```bash
mkdir -p target/dev-home/Library target/dev-home/tmp && ln -sfn ~/Library/Keychains target/dev-home/Library/Keychains
```

```bash
HOME=$PWD/target/dev-home TMPDIR=$PWD/target/dev-home/tmp/ ./target/debug/spotifast --verbose
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
  HOME=$PWD/target/dev-home TMPDIR=$PWD/target/dev-home/tmp/ cargo run --example offline_probe -- spotify:track:4uLU6hMCjMI75M1A2tKUQC
  ```

- Step 2 done: librespot `offline` branch adds `OfflineTracks` and
  `Player::set_offline_tracks`; the player asks it before the network,
  online or not. The backend opens the store when it restores sign-ins
  and hands it to every engine. A song is kept under the URI asked for and
  the one Spotify relinks it to, since the app plays the relinked one.
- Known gap: the store opens once at startup. If the keychain does not
  answer within 20 s (a locked keychain, or macOS asking about a rebuilt
  binary), downloads stay closed until the next start. Retry in step 5.
- librespot comes from github.com/Andromeadus/librespot, branch `offline`,
  pinned by commit in `Cargo.toml`. The local clone at `../librespot` has
  that branch with a `fork` remote: commit there, push, then bump the rev.
- Step 3a done: after each verified sign-in the profile is kept in
  `<cache>/library/profile.json`; a cold start that cannot reach Spotify
  at all (a network error, not a rate limit) uses it, so the library
  shows offline. Sign-out removes it with the library cache.
- Known gap: an offline cold start took about 13 s to leave the
  Connecting card (2026-10-22, macOS). Not measured yet; likely the first
  `/me` attempt (token refresh) or the keychain reads.
- Known gap: only macOS follows network reachability. Elsewhere
  `network_reachable` stays true, so offline requests are still sent and
  fail; anything retried every frame needs its own guard.
- `packaging/test-launchers.py` fails on macOS (GNU `install -D` and
  `sed -i`); it is a Linux packaging check.
