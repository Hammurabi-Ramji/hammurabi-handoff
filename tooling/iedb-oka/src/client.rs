//! Live OKX.AI / HackQuest boundary — the ONLY networked module.
//!
//! Compiled only under the `okx-live` feature so the default build carries no
//! HTTP stack, matching the workspace-wide "egress is opt-in" posture. Even
//! here the client is a thin fetch/post shim: it holds no credentials of the
//! sovereign kind and performs no signing.

use crate::interop::{OkxInvocation, OkxResult};
use crate::{OkaError, Result};

/// A minimal HTTP client against an OKX.AI-compatible endpoint.
pub struct OkxClient {
    base_url: String,
    http: reqwest::Client,
}

impl OkxClient {
    /// Construct a client for `base_url` (e.g. a HackQuest sandbox host).
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            http: reqwest::Client::new(),
        }
    }

    /// Forward a marketplace invocation to the given path and parse the result.
    /// Network/parse failures map to [`OkaError::Boundary`].
    pub async fn dispatch(&self, path: &str, inv: &OkxInvocation) -> Result<OkxResult> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .http
            .post(url)
            .json(inv)
            .send()
            .await
            .map_err(|e| OkaError::Boundary(e.to_string()))?;
        let status = resp.status().as_u16();
        let body = resp
            .json::<serde_json::Value>()
            .await
            .map_err(|e| OkaError::Boundary(e.to_string()))?;
        Ok(OkxResult::for_invocation(inv, status, body))
    }
}
