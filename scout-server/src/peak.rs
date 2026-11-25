//! Memory peak of one heavy job (decompress + aggregate + recompress) on a real raw snapshot.
//! Run: `SCOUT_PEAK_FILE=<…json.zst> cargo test -p scout-server --release peak -- --ignored --nocapture`
//! (`SCOUT_ZSTD_LEVEL`, default 15, as in the server). Prints `VmHWM`, the process's peak RSS.

use crate::aggregate;
use crate::storage::Storage;

fn vm_hwm() -> String {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmHWM"))
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "VmHWM indisponible".to_owned())
}

#[test]
#[ignore = "needs a real raw snapshot: SCOUT_PEAK_FILE"]
fn peak_of_one_heavy_job() {
    let file = std::env::var("SCOUT_PEAK_FILE").expect("SCOUT_PEAK_FILE");
    let level = std::env::var("SCOUT_ZSTD_LEVEL").map_or(15, |v| v.parse().expect("level"));
    println!("start: {}", vm_hwm());
    let compressed = std::fs::read(&file).expect("read");
    let bytes = zstd::decode_all(&compressed[..]).expect("decode");
    println!("{} bytes of JSON, decompressed: {}", bytes.len(), vm_hwm());
    let (_, rows) = aggregate::aggregate(&bytes).expect("aggregate");
    println!("{} items, aggregated: {}", rows.len(), vm_hwm());
    let dir = std::env::temp_dir().join(format!("scout-peak-{}", std::process::id()));
    let size = Storage::new(&dir, level)
        .write("peak.json.zst", &bytes)
        .expect("write");
    let _ = std::fs::remove_dir_all(&dir);
    println!("recompressed to {size} bytes (level {level}): {}", vm_hwm());
}
