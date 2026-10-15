//! The site client: `GET /api/helper/me` with the key (ADDON-SPEC 7.1, 7.2), its answer
//! sorted into the engine's results.

use super::{Connection, HelperKey, Refusal, Site, SiteError};
use serde_json::Value;
use std::time::Duration;

/// The longest answer read; `/api/helper/me` is a few hundred bytes.
const MAX_BODY: u64 = 64 * 1024;

pub struct SiteClient {
    agent: ureq::Agent,
    production: String,
    staging: String,
}

impl Default for SiteClient {
    fn default() -> Self {
        SiteClient::with_addresses(
            Site::Production.address(),
            Site::Staging.address(),
            Duration::from_secs(20),
        )
    }
}

impl SiteClient {
    /// A client for the two sites at these addresses, giving up on a request after
    /// `timeout`. Redirects are not followed: the site's helper API never redirects, and
    /// staging's login page arrives as one.
    pub fn with_addresses(production: &str, staging: &str, timeout: Duration) -> Self {
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .timeout_global(Some(timeout))
            .build()
            .into();
        SiteClient {
            agent,
            production: production.to_string(),
            staging: staging.to_string(),
        }
    }

    /// Asks `site` whether it accepts `key`.
    pub fn check(&self, site: Site, key: &HelperKey) -> Result<Connection, SiteError> {
        let base = match site {
            Site::Production => &self.production,
            Site::Staging => &self.staging,
        };
        let response = self
            .agent
            .get(format!("{base}/api/helper/me"))
            .header("Authorization", format!("Bearer {}", key.secret()))
            .header("Accept", "application/json")
            .call();
        let mut response = match response {
            Ok(response) => response,
            Err(e) => return Err(unreachable(no_answer(&e))),
        };
        let status = response.status().as_u16();
        let html = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.trim().to_ascii_lowercase().starts_with("text/html"));
        let text = response
            .body_mut()
            .with_config()
            .limit(MAX_BODY)
            .read_to_string()
            .map_err(|e| unreachable(format!("its answer couldn't be read: {}", no_answer(&e))))?;
        let body = serde_json::from_str::<Value>(&text).ok();
        sort(site, status, html, body.as_ref())
    }
}

/// Sorts an answer by its status, whether it is a web page, and, when it is JSON, its body.
fn sort(
    site: Site,
    status: u16,
    html: bool,
    body: Option<&Value>,
) -> Result<Connection, SiteError> {
    // The site's own errors are `{ "error", "code" }` (ADDON-SPEC 7). Vercel's login page
    // is a redirect or a 401 web page; the helper API never redirects.
    let code = body
        .filter(|b| b["error"].is_string())
        .and_then(|b| b["code"].as_str());
    let login_page = (300..=399).contains(&status) || (status == 401 && html);
    match (status, code) {
        (200, _) => body.and_then(connection).ok_or_else(|| {
            unreachable("it answered 200 with an answer the helper can't read".into())
        }),
        (401, Some(_)) => Err(SiteError::KeyRefused(Refusal::BadKey)),
        (403, Some(_)) => Err(SiteError::KeyRefused(Refusal::NotOfficer)),
        (503, Some("helper-off")) => Err(SiteError::HelperOff),
        _ if login_page && site == Site::Staging => Err(SiteError::StagingNotOpen),
        // Only the status: what the site sent back never goes into a message.
        (status, _) => Err(unreachable(format!("it answered {status}"))),
    }
}

/// Reads an accepted key's answer; `None` if it isn't `{ officer, env, formatVersions,
/// addon: { version, url } }`.
fn connection(body: &Value) -> Option<Connection> {
    let officer = body["officer"].as_str().filter(|s| !s.trim().is_empty())?;
    let format_versions = body["formatVersions"]
        .as_array()?
        .iter()
        .map(Value::as_i64)
        .collect::<Option<Vec<i64>>>()?;
    Some(Connection {
        officer: officer.to_string(),
        env: body["env"].as_str()?.to_string(),
        format_versions,
        addon_version: body["addon"]["version"].as_str()?.to_string(),
        addon_url: body["addon"]["url"].as_str()?.to_string(),
    })
}

/// Why no answer came, in words that can't hold the key (the request's headers are never
/// part of them).
fn no_answer(e: &ureq::Error) -> String {
    match e {
        ureq::Error::Timeout(_) => "it didn't answer in time".to_string(),
        ureq::Error::HostNotFound => "its address wasn't found".to_string(),
        ureq::Error::ConnectionFailed => "the connection failed".to_string(),
        ureq::Error::Io(e) => format!("the connection failed ({:?})", e.kind()),
        ureq::Error::Tls(_) | ureq::Error::Rustls(_) => "the secure connection failed".to_string(),
        _ => "the request failed".to_string(),
    }
}

fn unreachable(detail: String) -> SiteError {
    SiteError::Unreachable { detail }
}
