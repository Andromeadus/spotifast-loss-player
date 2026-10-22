//! Diagnostic: download songs into the offline store with the playback
//! sign-in, then read them back from the sealed index.
//!
//!   cargo run --example offline_probe -- spotify:track:4uLU6hMCjMI75M1A2tKUQC

use librespot_core::{Session, SessionConfig};
use spotifast::credentials::{Grant, Slot, Store as Credentials};
use spotifast::offline::{self, Store};

fn main() -> anyhow::Result<()> {
    fastframe_log::Logging::new("spotifast", env!("CARGO_PKG_VERSION"))
        .filter("warn")
        .init()?;
    let uris: Vec<String> = std::env::args().skip(1).collect();
    if uris.is_empty() {
        anyhow::bail!("usage: offline_probe <spotify:track:...>...");
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let dirs = spotifast::paths::AppDirs::discover().dirs;
    let (grant, key) = runtime.block_on(async {
        let credentials = Credentials::new(dirs.clone());
        let grant = credentials.lease(Slot::Playback).load().await?.grant;
        let offline = credentials.lease(Slot::Offline);
        let key = match offline.load().await?.grant {
            Some(Grant::Offline(key)) => key,
            _ => {
                let key = Store::new_key()?;
                offline.save(Grant::Offline(key.clone())).await?;
                key
            }
        };
        anyhow::Ok((grant, key))
    })?;
    let Some(Grant::Playback(grant)) = grant else {
        anyhow::bail!("Enable playback in Spotifast first");
    };

    let session = {
        let _context = runtime.enter();
        Session::new(SessionConfig::default(), None)
    };
    runtime.block_on(session.connect(grant, false))?;
    let dir = offline::dir(&dirs);
    let mut store = Store::open(dir.clone(), &key)?;
    for uri in &uris {
        let fetched = runtime.block_on(offline::fetch(&session, uri))?;
        let track = &fetched.track;
        let artists: Vec<_> = track.artists.iter().map(|a| a.name.as_str()).collect();
        println!(
            "{}: {} by {} at {} kbps, {} bytes encrypted",
            fetched.uris.join(" = "),
            track.name,
            artists.join(", "),
            track.kbps,
            fetched.encrypted.len()
        );
        store.add(fetched)?;
    }

    let store = Store::open(dir.clone(), &key)?;
    for uri in &uris {
        let track = store
            .get(uri)
            .ok_or_else(|| anyhow::anyhow!("{uri} missing after reopening"))?;
        let size = std::fs::metadata(store.file_path(track))?.len();
        println!("kept {uri} ({size} bytes) in {}", dir.display());
    }
    Ok(())
}
