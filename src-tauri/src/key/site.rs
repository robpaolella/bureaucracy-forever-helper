//! The site client: `GET /api/helper/me` with the key (ADDON-SPEC 7.1, 7.2) and
//! `GET /api/helper/raids` (7.5), their answers sorted into the engine's results, and
//! `POST /api/helper/operations`, its answer handed back as it came for the upload client
//! to sort.

use super::{Connection, HelperKey, Refusal, Site, SiteError};
use crate::raids::{Raid, RaidsFailure};
use crate::throttle::{self, Throttle};
use serde_json::Value;
use std::time::Duration;

/// The longest answer read; `/api/helper/me` is a few hundred bytes.
const MAX_BODY: u64 = 64 * 1024;

/// The longest snapshot or catch-up read: a raid's entry in the inbox may be 200,000 bytes
/// (7.3), and its JSON is a little longer.
const MAX_RAID_BODY: u64 = 1024 * 1024;

pub struct SiteClient {
    /// The budget every request waits its turn on; none for a client in a test.
    throttle: Option<&'static Throttle>,
    agent: ureq::Agent,
    production: String,
    staging: String,
}

impl Default for SiteClient {
    fn default() -> Self {
        SiteClient {
            throttle: Some(&throttle::SHARED),
            ..SiteClient::with_addresses(
                Site::Production.address(),
                Site::Staging.address(),
                Duration::from_secs(20),
            )
        }
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
            throttle: None,
            agent,
            production: production.to_string(),
            staging: staging.to_string(),
        }
    }

    /// Asks `site` whether it accepts `key`.
    pub fn check(&self, site: Site, key: &HelperKey) -> Result<Connection, SiteError> {
        let (status, html, body, _) = self.get(site, key, "/api/helper/me", MAX_BODY)?;
        sort(site, status, html, body.as_ref())
    }

    /// Asks `site` for its upcoming raids (ADDON-SPEC 7.5), in the order it gives them.
    /// Its failures are sorted as `check` sorts them.
    pub fn raids(&self, site: Site, key: &HelperKey) -> Result<Vec<Raid>, RaidsFailure> {
        let (status, html, body, retry_after) = self
            .get(site, key, "/api/helper/raids", MAX_BODY)
            .map_err(RaidsFailure::from)?;
        let error = match (status, body.as_ref().and_then(Raid::parse_list)) {
            (200, Some(raids)) => return Ok(raids),
            (200, None) => {
                unreachable("it answered 200 with an answer the helper can't read".into())
            }
            _ => failure(site, status, html, body.as_ref()),
        };
        Err(RaidsFailure { error, retry_after })
    }

    /// Asks `site` for one raid's `snapshot` or `catchup` (ADDON-SPEC 7.1) as JSON. Its
    /// failures are sorted as `raids` sorts them; an answer that isn't a JSON object is
    /// unreadable.
    pub fn raid_document(
        &self,
        site: Site,
        key: &HelperKey,
        raid_id: &str,
        part: &str,
    ) -> Result<Value, RaidsFailure> {
        let path = format!("/api/helper/raids/{}/{part}", path_segment(raid_id));
        let (status, html, body, retry_after) = self
            .get(site, key, &path, MAX_RAID_BODY)
            .map_err(RaidsFailure::from)?;
        let error = match body {
            Some(body) if status == 200 && body.is_object() => return Ok(body),
            _ if status == 200 => {
                unreachable("it answered 200 with an answer the helper can't read".into())
            }
            body => failure(site, status, html, body.as_ref()),
        };
        Err(RaidsFailure { error, retry_after })
    }

    /// A `GET` of `path` with the key: the status, whether the answer is a web page, its
    /// body when that is JSON, and its `Retry-After` in seconds when given as a number.
    fn get(
        &self,
        site: Site,
        key: &HelperKey,
        path: &str,
        limit: u64,
    ) -> Result<(u16, bool, Option<Value>, Option<u64>), SiteError> {
        self.take_turn(site);
        let response = self
            .agent
            .get(format!("{}{path}", self.base(site)))
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
        let retry_after = retry_after(&response);
        let text = response
            .body_mut()
            .with_config()
            .limit(limit)
            .read_to_string()
            .map_err(|e| unreachable(format!("its answer couldn't be read: {}", no_answer(&e))))?;
        Ok((
            status,
            html,
            serde_json::from_str::<Value>(&text).ok(),
            retry_after,
        ))
    }
}

/// The site's answer to an upload, as it came: its status, its `Retry-After` in seconds
/// (when given as a number), and its body when that is JSON.
#[derive(Debug, Clone, PartialEq)]
pub struct UploadReply {
    pub status: u16,
    pub retry_after: Option<u64>,
    pub body: Option<Value>,
}

impl SiteClient {
    /// Sends `body`, a `{ "operations": [ ... ] }` upload, to `site` with `key`. An error is
    /// no answer at all, in words that can't hold the key.
    pub fn post_operations(
        &self,
        site: Site,
        key: &HelperKey,
        body: &str,
    ) -> Result<UploadReply, String> {
        self.take_turn(site);
        let response = self
            .agent
            .post(format!("{}/api/helper/operations", self.base(site)))
            .header("Authorization", format!("Bearer {}", key.secret()))
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .send(body);
        let mut response = response.map_err(|e| no_answer(&e))?;
        let status = response.status().as_u16();
        let retry_after = retry_after(&response);
        let text = response
            .body_mut()
            .with_config()
            .limit(MAX_BODY)
            .read_to_string()
            .map_err(|e| format!("its answer couldn't be read: {}", no_answer(&e)))?;
        Ok(UploadReply {
            status,
            retry_after,
            body: serde_json::from_str(&text).ok(),
        })
    }

    /// Waits for this request's turn in the key's one request budget.
    fn take_turn(&self, site: Site) {
        if let Some(throttle) = self.throttle {
            throttle.wait(self.base(site));
        }
    }

    fn base(&self, site: Site) -> &str {
        match site {
            Site::Production => &self.production,
            Site::Staging => &self.staging,
        }
    }
}

/// `text` made safe to put in a URL path: anything but letters, digits, `-`, `_` and `.` is
/// percent-encoded.
fn path_segment(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// The answer's `Retry-After` in seconds, when given as a number.
fn retry_after<B>(response: &ureq::http::Response<B>) -> Option<u64> {
    response
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse().ok())
}

/// Sorts an answer by its status, whether it is a web page, and, when it is JSON, its body.
fn sort(
    site: Site,
    status: u16,
    html: bool,
    body: Option<&Value>,
) -> Result<Connection, SiteError> {
    match status {
        200 => body.and_then(connection).ok_or_else(|| {
            unreachable("it answered 200 with an answer the helper can't read".into())
        }),
        _ => Err(failure(site, status, html, body)),
    }
}

/// Sorts an answer that isn't a `200`.
fn failure(site: Site, status: u16, html: bool, body: Option<&Value>) -> SiteError {
    // The site's own errors are `{ "error", "code" }` (ADDON-SPEC 7). Vercel's login page
    // is a redirect or a 401 web page; the helper API never redirects.
    let code = body
        .filter(|b| b["error"].is_string())
        .and_then(|b| b["code"].as_str());
    let login_page = (300..=399).contains(&status) || (status == 401 && html);
    match (status, code) {
        (401, Some(_)) => SiteError::KeyRefused(Refusal::BadKey),
        (403, Some(_)) => SiteError::KeyRefused(Refusal::NotOfficer),
        (503, Some("helper-off")) => SiteError::HelperOff,
        _ if login_page && site == Site::Staging => SiteError::StagingNotOpen,
        // Only the status: what the site sent back never goes into a message.
        (status, _) => unreachable(format!("it answered {status}")),
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

#[cfg(test)]
mod throttle_tests {
    use super::*;

    #[test]
    fn the_real_client_waits_its_turn_and_a_test_client_does_not() {
        assert!(SiteClient::default().throttle.is_some());
        let test = SiteClient::with_addresses("http://a", "http://b", Duration::from_secs(1));
        assert!(test.throttle.is_none());
    }
}
