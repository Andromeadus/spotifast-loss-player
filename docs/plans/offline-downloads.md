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
- librespot changes live in a local clone at `../librespot` (branch
  `offline`, from crmne/librespot 8c68baf), wired in with a Cargo path
  patch. Publish it as a GitHub fork once it works.

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

- `packaging/test-launchers.py` fails on macOS (GNU `install -D` and
  `sed -i`); it is a Linux packaging check.
