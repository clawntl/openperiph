//! Fetching from GitHub: release information and downloads (rustls with the system's
//! certificates, so company proxies with their own certificate work too).

use std::io::Read;
use std::time::Duration;

const AGENT: &str = concat!("OpenPeriph/", env!("CARGO_PKG_VERSION"));

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(15)).timeout_read(Duration::from_secs(30)).build()
}

/// A GitHub release: tag, page, and each asset's name, download link and SHA-256 when GitHub
/// reports one.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GhRelease {
    pub tag: String,
    pub page_url: String,
    /// The release notes (Markdown).
    pub body: String,
    pub assets: Vec<GhAsset>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GhAsset {
    pub name: String,
    pub url: String,
    pub sha256: Option<String>,
    pub size: u64,
}

/// The newest release of `owner/repo`.
pub fn latest_release(repo: &str) -> Result<GhRelease, String> {
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let v: serde_json::Value = agent()
        .get(&url)
        .set("User-Agent", AGENT)
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("Couldn't reach GitHub: {e}"))?
        .into_json()
        .map_err(|e| format!("GitHub sent an unexpected answer: {e}"))?;
    let tag = v["tag_name"].as_str().ok_or("GitHub sent no release name.")?.to_string();
    let assets = v["assets"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|x| GhAsset {
                    name: x["name"].as_str().unwrap_or("").to_string(),
                    url: x["browser_download_url"].as_str().unwrap_or("").to_string(),
                    sha256: x["digest"].as_str().and_then(|d| d.strip_prefix("sha256:")).map(str::to_ascii_lowercase),
                    size: x["size"].as_u64().unwrap_or(0),
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(GhRelease {
        tag,
        page_url: v["html_url"].as_str().unwrap_or("").to_string(),
        body: v["body"].as_str().unwrap_or("").to_string(),
        assets,
    })
}

/// Downloads at most `max` bytes, reporting progress (0..1) when the size is known.
pub fn download(url: &str, max: usize, progress: &dyn Fn(f32)) -> Result<Vec<u8>, String> {
    let r = agent().get(url).set("User-Agent", AGENT).call().map_err(|e| format!("Couldn't download {url}: {e}"))?;
    let total: Option<usize> = r.header("Content-Length").and_then(|v| v.parse().ok());
    let mut reader = r.into_reader().take(max as u64 + 1);
    let mut out = Vec::with_capacity(total.unwrap_or(0).min(max));
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf).map_err(|e| format!("The download stopped: {e}"))?;
        if n == 0 {
            break;
        }
        out.extend_from_slice(&buf[..n]);
        if out.len() > max {
            return Err("The download is bigger than expected.".into());
        }
        if let Some(t) = total {
            progress(out.len() as f32 / t.max(1) as f32);
        }
    }
    Ok(out)
}

/// Checks a download against the SHA-256 GitHub reported for it.
pub fn verify(data: &[u8], sha256: Option<&str>) -> Result<(), String> {
    use sha2::{Digest, Sha256};
    let Some(want) = sha256 else { return Ok(()) };
    let got: String = Sha256::digest(data).iter().map(|b| format!("{b:02x}")).collect();
    if got == want {
        Ok(())
    } else {
        Err("The download is damaged (its checksum doesn't match GitHub's).".into())
    }
}
