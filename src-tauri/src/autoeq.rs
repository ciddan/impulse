//! AutoEq database access: headphone index (GitHub tree API, cached) and
//! per-profile file downloads (raw.githubusercontent.com).

use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

const REPO: &str = "jaakkopasanen/AutoEq";
const BRANCH: &str = "master";
const INDEX_TTL_SECS: u64 = 7 * 24 * 3600;
const USER_AGENT: &str = "WinAutoConv/0.1 (+https://github.com)";

/// Preferred measurement sources, best first. Everything else ranks after.
const SOURCE_PRIORITY: &[&str] = &[
    "oratory1990",
    "crinacle",
    "Rtings",
    "Innerfidelity",
    "Headphone.com Legacy",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeadphoneEntry {
    pub source: String,
    pub form: String,
    pub name: String,
    /// Lower is better; derived from SOURCE_PRIORITY.
    pub rank: u32,
    pub has_ir: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IndexCache {
    pub fetched_at: u64,
    pub entries: Vec<HeadphoneEntry>,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn source_rank(source: &str) -> u32 {
    SOURCE_PRIORITY
        .iter()
        .position(|s| s.eq_ignore_ascii_case(source))
        .map(|p| p as u32)
        .unwrap_or(SOURCE_PRIORITY.len() as u32)
}

fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .context("build http client")
}

#[derive(Deserialize)]
struct TreeResponse {
    tree: Vec<TreeItem>,
    #[serde(default)]
    truncated: bool,
}

#[derive(Deserialize)]
struct TreeItem {
    path: String,
    #[serde(rename = "type")]
    kind: String,
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Primary index source: results/INDEX.md — one raw fetch that lists every
/// profile as "- [Name](./Source/Form/Name) by ...". The GitHub tree API is
/// only a fallback because its response caps out before the full results
/// tree fits (which silently dropped oratory1990 & friends).
async fn fetch_index() -> Result<Vec<HeadphoneEntry>> {
    let client = client()?;
    let url = format!(
        "https://raw.githubusercontent.com/{}/{}/results/INDEX.md",
        REPO, BRANCH
    );
    let resp = client.get(&url).send().await.context("request INDEX.md")?;
    if !resp.status().is_success() {
        bail!("INDEX.md fetch returned {}", resp.status());
    }
    let body = resp.text().await.context("read INDEX.md")?;

    let mut entries = Vec::new();
    for line in body.lines() {
        // "- [1Custom SA02](./crinacle/711%20in-ear/1Custom%20SA02) by crinacle on 711"
        let Some(rest) = line.trim_start().strip_prefix("- [") else {
            continue;
        };
        let Some(link_start) = rest.find("](./") else {
            continue;
        };
        let after_link = &rest[link_start + 4..];
        let Some(close) = after_link.find(')') else {
            continue;
        };
        let path = percent_decode(&after_link[..close]);
        let parts: Vec<&str> = path.split('/').collect();
        if parts.len() != 3 {
            continue;
        }
        entries.push(HeadphoneEntry {
            source: parts[0].to_string(),
            form: parts[1].to_string(),
            name: parts[2].to_string(),
            rank: source_rank(parts[0]),
            // Nearly every results folder ships minimum-phase IRs; verified
            // for real at download time (apply falls back to GraphicEQ).
            has_ir: true,
        });
    }
    if entries.len() < 100 {
        bail!(
            "INDEX.md parse produced only {} entries — format change?",
            entries.len()
        );
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name).then(a.rank.cmp(&b.rank)));
    Ok(entries)
}

/// Fallback: GitHub tree API. WARNING: response is size-capped and the full
/// results tree exceeds it, so this can silently miss sources.
async fn fetch_index_tree() -> Result<Vec<HeadphoneEntry>> {
    let client = client()?;
    let url = format!(
        "https://api.github.com/repos/{}/git/trees/{}:results?recursive=1",
        REPO, BRANCH
    );
    let resp = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .context("request AutoEq index")?;
    if !resp.status().is_success() {
        bail!("GitHub tree API returned {}", resp.status());
    }
    let tree: TreeResponse = resp.json().await.context("parse tree json")?;
    if tree.truncated {
        // Still usable, but note it — should not happen for the results subtree.
        eprintln!("warning: AutoEq tree listing truncated");
    }

    // Paths look like: "<source>/<form>/<name>/<name> GraphicEQ.txt"
    let mut entries: Vec<HeadphoneEntry> = Vec::new();
    let mut ir_dirs: std::collections::HashSet<String> = std::collections::HashSet::new();
    for item in &tree.tree {
        if item.kind != "blob" {
            continue;
        }
        if item.path.ends_with("Hz.wav") && item.path.contains("minimum phase") {
            if let Some(dir) = item.path.rsplit_once('/').map(|(d, _)| d.to_string()) {
                ir_dirs.insert(dir);
            }
        }
    }
    for item in &tree.tree {
        if item.kind != "blob" || !item.path.ends_with(" GraphicEQ.txt") {
            continue;
        }
        let parts: Vec<&str> = item.path.split('/').collect();
        if parts.len() != 4 {
            continue;
        }
        let (source, form, name) = (parts[0], parts[1], parts[2]);
        let dir = format!("{}/{}/{}", source, form, name);
        entries.push(HeadphoneEntry {
            source: source.to_string(),
            form: form.to_string(),
            name: name.to_string(),
            rank: source_rank(source),
            has_ir: ir_dirs.contains(&dir),
        });
    }
    if entries.is_empty() {
        bail!("AutoEq index came back empty — API change?");
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name).then(a.rank.cmp(&b.rank)));
    Ok(entries)
}

/// Load the index from cache, refreshing from GitHub when stale or forced.
pub async fn get_index(cache_dir: &Path, force: bool) -> Result<IndexCache> {
    let cache_file = cache_dir.join("autoeq_index.json");
    if !force {
        if let Ok(raw) = fs::read_to_string(&cache_file) {
            if let Ok(cache) = serde_json::from_str::<IndexCache>(&raw) {
                if now_unix().saturating_sub(cache.fetched_at) < INDEX_TTL_SECS {
                    return Ok(cache);
                }
            }
        }
    }
    let fetched = match fetch_index().await {
        Ok(entries) => Ok(entries),
        // INDEX.md unavailable? Tree API is better than nothing, though it
        // can miss sources due to response size caps.
        Err(primary) => fetch_index_tree().await.map_err(|_| primary),
    };
    match fetched {
        Ok(entries) => {
            let cache = IndexCache {
                fetched_at: now_unix(),
                entries,
            };
            fs::create_dir_all(cache_dir).ok();
            fs::write(&cache_file, serde_json::to_string(&cache)?).ok();
            Ok(cache)
        }
        Err(e) => {
            // Offline fallback: serve stale cache if we have one.
            if let Ok(raw) = fs::read_to_string(&cache_file) {
                if let Ok(cache) = serde_json::from_str::<IndexCache>(&raw) {
                    return Ok(cache);
                }
            }
            Err(e)
        }
    }
}

fn encode_path_segment(s: &str) -> String {
    // Percent-encode for a raw.githubusercontent.com URL path.
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// Fetch "<name>.csv" from a profile's results folder.
pub async fn download_curves_csv(source: &str, form: &str, name: &str) -> Result<Vec<u8>> {
    let client = client()?;
    let url = format!(
        "https://raw.githubusercontent.com/{}/{}/results/{}/{}",
        REPO,
        BRANCH,
        encode_path_segment(&format!("{}/{}/{}", source, form, name)),
        encode_path_segment(&format!("{}.csv", name))
    );
    let resp = client
        .get(&url)
        .send()
        .await
        .context("download curves csv")?;
    if !resp.status().is_success() {
        bail!("curves csv download failed: {}", resp.status());
    }
    Ok(resp.bytes().await.context("read curves csv")?.to_vec())
}

pub struct ProfileFiles {
    /// (file name we store as, bytes) for each impulse response found.
    pub irs: Vec<(String, Vec<u8>)>,
    /// Non-IR extras to store alongside (e.g. curves.csv).
    pub extras: Vec<(String, Vec<u8>)>,
}

/// Download the minimum-phase IRs (and curves CSV) for one headphone profile.
pub async fn download_profile(source: &str, form: &str, name: &str) -> Result<ProfileFiles> {
    let client = client()?;
    let base = format!(
        "https://raw.githubusercontent.com/{}/{}/results/{}",
        REPO,
        BRANCH,
        encode_path_segment(&format!("{}/{}/{}", source, form, name))
    );

    // Measurement CSV (raw/target/equalization curves) — optional, used by
    // the response chart. Ignore failures.
    let mut extras = Vec::new();
    if let Ok(bytes) = download_curves_csv(source, form, name).await {
        extras.push(("curves.csv".to_string(), bytes));
    }

    let mut irs = Vec::new();
    for (rate, file_name) in [
        (44100u32, "minphase_44100.wav"),
        (48000, "minphase_48000.wav"),
    ] {
        let url = format!(
            "{}/{}",
            base,
            encode_path_segment(&format!("{} minimum phase {}Hz.wav", name, rate))
        );
        if let Ok(resp) = client.get(&url).send().await {
            if resp.status().is_success() {
                if let Ok(bytes) = resp.bytes().await {
                    irs.push((file_name.to_string(), bytes.to_vec()));
                }
            }
        }
    }

    if irs.is_empty() {
        bail!("This profile publishes no convolution impulse responses");
    }
    Ok(ProfileFiles { irs, extras })
}
