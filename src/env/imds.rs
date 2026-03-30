//! Shared IMDSv2 client for the EC2 instance metadata service.
//!
//! IMDSv2 requires a two-step flow:
//!   1. PUT /latest/api/token with X-aws-ec2-metadata-token-ttl-seconds header
//!      to obtain a session token.
//!   2. GET /latest/meta-data/{path} with X-aws-ec2-metadata-token header set
//!      to the token obtained in step 1.
//!
//! This module provides a thin wrapper that handles both steps.

/// Base URL for the EC2 instance metadata service.
const IMDS_BASE: &str = "http://169.254.169.254";
/// IMDSv2 token endpoint path.
const IMDS_TOKEN_PATH: &str = "/latest/api/token";
/// Request header used to specify the token TTL when acquiring an IMDSv2 token.
const IMDS_TTL_HEADER: &str = "X-aws-ec2-metadata-token-ttl-seconds";
/// Request header used to pass the IMDSv2 session token on metadata requests.
const IMDS_TOKEN_HEADER: &str = "X-aws-ec2-metadata-token";
/// Token TTL in seconds (6 hours).
const IMDS_TOKEN_TTL: &str = "21600";
/// HTTP request timeout in seconds for IMDS calls.
const IMDS_TIMEOUT_SECS: u64 = 2;

/// IMDSv2 session holding an HTTP client and an acquired session token.
pub(super) struct ImdsClient {
    client: reqwest::blocking::Client,
    token: String,
}

impl ImdsClient {
    /// Acquires an IMDSv2 session token and returns a ready-to-use client.
    ///
    /// Returns `None` if the token request fails (e.g. not running on EC2/EKS).
    pub(super) fn new() -> Option<Self> {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(IMDS_TIMEOUT_SECS))
            .build()
            .ok()?;

        let token = client
            .put(format!("{IMDS_BASE}{IMDS_TOKEN_PATH}"))
            .header(IMDS_TTL_HEADER, IMDS_TOKEN_TTL)
            .send()
            .ok()?
            .text()
            .ok()?;

        Some(Self { client, token })
    }

    /// GETs a metadata path under `/latest/meta-data/` and returns the response body as a `String`.
    pub(super) fn get(&self, path: &str) -> Option<String> {
        let url = format!("{IMDS_BASE}/latest/meta-data/{path}");
        self.client
            .get(&url)
            .header(IMDS_TOKEN_HEADER, &self.token)
            .send()
            .ok()
            .and_then(|r| {
                if r.status().is_success() {
                    r.text().ok()
                } else {
                    None
                }
            })
    }

    /// GETs a path under `/latest/meta-data/` and deserializes the JSON response body.
    pub(super) fn get_json<T: serde::de::DeserializeOwned>(&self, path: &str) -> Option<T> {
        let url = format!("{IMDS_BASE}/latest/meta-data/{path}");
        self.client
            .get(&url)
            .header(IMDS_TOKEN_HEADER, &self.token)
            .send()
            .ok()
            .and_then(|r| {
                if r.status().is_success() {
                    r.json().ok()
                } else {
                    None
                }
            })
    }
}
