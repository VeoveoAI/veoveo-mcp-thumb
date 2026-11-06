//! The Hub's own routes, called for a tool's caller with the bearer the SDK exchanges for their
//! profile. The Hub decides each call, so Lore's rules decide each read; its refusals reach the
//! caller with their status and RFC 9457 problem detail.
use std::{fmt, path::Path, time::Duration};

use futures::StreamExt as _;
use reqwest::{
    StatusCode,
    header::{ACCEPT, CONTENT_LENGTH, CONTENT_RANGE, ETAG, RANGE},
};
use serde::Deserialize;
use tokio::io::{AsyncSeekExt as _, AsyncWriteExt as _};
use veoveo_hub_sdk::{
    HubProfileId,
    client::{CallError, HubClient},
};

use crate::contract::{BranchName, FilePath, RepositoryId};

/// How long a listing or a probe may take.
const LISTING_TIMEOUT: Duration = Duration::from_secs(30);
/// How long one file may take, to its last byte.
const FILE_TIMEOUT: Duration = Duration::from_secs(600);

/// The Hub at the origin of its issuer.
#[derive(Clone)]
pub struct Hub {
    base: url::Url,
    http: reqwest::Client,
}

/// Why a call to the Hub produced no answer.
#[derive(Debug, thiserror::Error)]
pub enum HubError {
    /// The SDK obtained no bearer for the caller.
    #[error("no token for the caller: {0}")]
    Bearer(#[from] CallError),
    #[error("the Hub could not be reached: {0}")]
    Transport(reqwest::Error),
    /// The Hub answered with an error status.
    #[error("the Hub answered {status}{problem}")]
    Answered {
        status: StatusCode,
        problem: Problem,
    },
    #[error("the Hub's answer is not what the server reads: {0}")]
    Decode(#[from] serde_json::Error),
    #[error("a file could not be written: {0}")]
    Local(#[from] std::io::Error),
}

impl From<reqwest::Error> for HubError {
    fn from(error: reqwest::Error) -> Self {
        // A URL names a repository and a path, and holds no credential, but it has no place in
        // a caller's error text either.
        Self::Transport(error.without_url())
    }
}

/// The RFC 9457 problem detail the Hub answers an error with.
#[derive(Debug, Default, Deserialize)]
pub struct Problem {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for part in [&self.title, &self.detail].into_iter().flatten() {
            write!(f, ": {part}")?;
        }
        Ok(())
    }
}

/// What one byte of a file says about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probe {
    /// The file's content address.
    pub address: String,
    /// The file's length.
    pub size: u64,
}

/// What a folder's listing says of its files, by name. Lore's address is the file's `ETag`.
#[derive(Debug, Deserialize)]
struct Listing {
    entries: Vec<ListedEntry>,
}

#[derive(Debug, Deserialize)]
struct ListedEntry {
    name: String,
    #[serde(default)]
    address: Option<String>,
}

/// The Hub, for one caller at one profile.
#[derive(Clone)]
pub struct Reader {
    pub hub: Hub,
    pub client: HubClient,
    pub profile: HubProfileId,
}

impl Hub {
    pub fn new(issuer: &url::Url, http: reqwest::Client) -> anyhow::Result<Self> {
        anyhow::ensure!(
            issuer.has_host() && !issuer.cannot_be_a_base(),
            "the Hub's issuer is no HTTP origin"
        );
        Ok(Self {
            base: issuer.clone(),
            http,
        })
    }

    /// A route of `segments`, each percent-encoded as one path segment.
    fn route<'a>(&self, segments: impl IntoIterator<Item = &'a str>) -> url::Url {
        let mut url = self.base.clone();
        url.set_query(None);
        url.set_fragment(None);
        url.path_segments_mut()
            .expect("an HTTP origin has a path")
            .clear()
            .extend(segments);
        url
    }

    fn files_route<'a>(
        &self,
        profile: &'a HubProfileId,
        repository: &'a RepositoryId,
        branch: &'a BranchName,
        path: impl IntoIterator<Item = &'a str>,
    ) -> url::Url {
        self.route(
            [
                "lore",
                profile.as_str(),
                "repositories",
                repository.as_str(),
                "files",
                branch.as_str(),
            ]
            .into_iter()
            .chain(path),
        )
    }
}

/// The error a refusal is, with its body read.
async fn answered(response: reqwest::Response) -> Result<reqwest::Response, HubError> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let body = response.bytes().await?;
    Err(HubError::Answered {
        status,
        problem: serde_json::from_slice(&body).unwrap_or_default(),
    })
}

impl Reader {
    async fn get(
        &self,
        url: url::Url,
        range: Option<&str>,
        timeout: Duration,
    ) -> Result<reqwest::Response, HubError> {
        let bearer = self.client.hub_bearer().await?;
        let mut request = self
            .hub
            .http
            .get(url)
            .bearer_auth(bearer.expose())
            .header(ACCEPT, "application/octet-stream, application/json")
            .timeout(timeout);
        if let Some(range) = range {
            request = request.header(RANGE, range);
        }
        answered(request.send().await?).await
    }

    /// Reads one byte of a file as the caller, so Lore decides whether they may read it now, and
    /// what it answers names the file's content address and length.
    pub async fn probe(
        &self,
        repository: &RepositoryId,
        branch: &BranchName,
        path: &FilePath,
    ) -> Result<Probe, HubError> {
        let url = self
            .hub
            .files_route(&self.profile, repository, branch, path.segments());
        let response = self.get(url, Some("bytes=0-0"), LISTING_TIMEOUT).await?;
        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
        };
        let address = header(ETAG.as_str())
            .unwrap_or_default()
            .trim_matches('"')
            .to_owned();
        let size = header(CONTENT_RANGE.as_str())
            .and_then(|range| range.rsplit('/').next()?.parse().ok())
            .or_else(|| header(CONTENT_LENGTH.as_str())?.parse().ok())
            .unwrap_or_default();
        Ok(Probe { address, size })
    }

    /// The content addresses of a folder's files, by name, read as the caller, so Lore decides
    /// whether they may read it now.
    pub async fn addresses(
        &self,
        repository: &RepositoryId,
        branch: &BranchName,
        folder: &str,
    ) -> Result<std::collections::HashMap<String, String>, HubError> {
        // The Hub takes a trailing `/` for a folder.
        let url = self.hub.files_route(
            &self.profile,
            repository,
            branch,
            folder
                .split('/')
                .filter(|segment| !segment.is_empty())
                .chain([""]),
        );
        let body = self.get(url, None, LISTING_TIMEOUT).await?.bytes().await?;
        Ok(serde_json::from_slice::<Listing>(&body)?
            .entries
            .into_iter()
            .filter_map(|entry| Some((entry.name, entry.address?)))
            .collect())
    }

    /// Writes the whole file to `to`.
    pub async fn save(
        &self,
        repository: &RepositoryId,
        branch: &BranchName,
        path: &FilePath,
        to: &Path,
    ) -> Result<(), HubError> {
        let url = self
            .hub
            .files_route(&self.profile, repository, branch, path.segments());
        let response = self.get(url, None, FILE_TIMEOUT).await?;
        let mut file = tokio::fs::File::create(to).await?;
        write_body(response, &mut file).await
    }

    /// Writes the file's first `head` and last `tail` bytes to `to`, at their places in a file of
    /// its full `size` that holds nothing between, which is enough to read a frame of a video.
    pub async fn save_ends(
        &self,
        repository: &RepositoryId,
        branch: &BranchName,
        path: &FilePath,
        ends: Ends,
        to: &Path,
    ) -> Result<(), HubError> {
        let url = self
            .hub
            .files_route(&self.profile, repository, branch, path.segments());
        let mut file = tokio::fs::File::create(to).await?;
        file.set_len(ends.size).await?;
        for (first, last) in ends.ranges() {
            let range = format!("bytes={first}-{last}");
            let response = self.get(url.clone(), Some(&range), FILE_TIMEOUT).await?;
            file.seek(std::io::SeekFrom::Start(first)).await?;
            write_body(response, &mut file).await?;
        }
        Ok(())
    }
}

/// The parts of a file a frame can be read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ends {
    pub size: u64,
    pub head: u64,
    pub tail: u64,
}

impl Ends {
    /// The byte ranges to read, both ends included, one when they would meet.
    pub fn ranges(self) -> Vec<(u64, u64)> {
        let last = self.size.saturating_sub(1);
        if self.head + self.tail >= self.size {
            return vec![(0, last)];
        }
        vec![(0, self.head - 1), (self.size - self.tail, last)]
    }
}

async fn write_body(
    response: reqwest::Response,
    file: &mut tokio::fs::File,
) -> Result<(), HubError> {
    let mut body = response.bytes_stream();
    while let Some(chunk) = body.next().await {
        file.write_all(&chunk?).await?;
    }
    file.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hub() -> Hub {
        let _ = rustls::crypto::ring::default_provider().install_default();
        Hub::new(
            &url::Url::parse("https://hub.example/").unwrap(),
            reqwest::Client::builder().build().unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn each_segment_is_encoded_once_at_the_issuers_origin() {
        let path = FilePath::parse("a b/c#d.png").unwrap();
        let url = hub().files_route(
            &HubProfileId::parse("default").unwrap(),
            &RepositoryId::parse("r1").unwrap(),
            &BranchName::parse("feature/x").unwrap(),
            path.segments(),
        );
        assert_eq!(
            url.as_str(),
            "https://hub.example/lore/default/repositories/r1/files/feature%2Fx/a%20b/c%23d.png"
        );
    }

    #[test]
    fn a_small_file_is_read_whole_and_a_large_one_from_its_ends() {
        let ends = |size| Ends {
            size,
            head: 10,
            tail: 5,
        };
        assert_eq!(ends(12).ranges(), [(0, 11)]);
        assert_eq!(ends(15).ranges(), [(0, 14)]);
        assert_eq!(ends(100).ranges(), [(0, 9), (95, 99)]);
    }
}
