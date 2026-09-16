use std::time::Instant;

use serde::{Deserialize, Serialize};
use wstd::{
    http::{Body, Client, HeaderMap, Method, Request},
    time::Duration,
};

use crate::config::Settings;

#[derive(Debug)]
pub struct TextResponse {
    pub body: String,
}

#[derive(Debug)]
pub struct ProbeResponse {
    pub status: u16,
    pub elapsed_ms: u64,
    pub cloudflare: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ProbeKind {
    #[default]
    Get,
    GraphQlPost,
}

impl ProbeKind {
    pub fn from_url(url: &str) -> Self {
        let lower = url.to_ascii_lowercase();
        let host = url_host(&lower).unwrap_or_default();
        let path = url_path(&lower);

        if host.starts_with("graphql.") || path.contains("graphql") {
            Self::GraphQlPost
        } else {
            Self::Get
        }
    }

    pub fn from_saved_or_url(saved: Self, url: &str) -> Self {
        match (saved, Self::from_url(url)) {
            (Self::Get, Self::GraphQlPost) => Self::GraphQlPost,
            _ => saved,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::GraphQlPost => "GraphQL POST",
        }
    }
}

pub async fn get_text(url: &str, settings: &Settings) -> Result<TextResponse, String> {
    let response = client(settings)
        .send(build_get_request(
            url,
            "application/json,text/javascript,*/*",
        )?)
        .await
        .map_err(|err| format!("Request failed for {url}: {err}"))?;

    let status = response.status().as_u16();
    let mut body = response.into_body();
    let body = body
        .str_contents()
        .await
        .map_err(|err| format!("Failed to read response body from {url}: {err}"))?
        .to_string();

    if status >= 400 {
        return Err(format!("HTTP {status} from {url}"));
    }

    Ok(TextResponse { body })
}

pub async fn probe(url: &str, probe_kind: ProbeKind, settings: &Settings) -> ProbeResponse {
    let started = Instant::now();
    let response = client(settings)
        .send(match build_probe_request(url, probe_kind) {
            Ok(request) => request,
            Err(err) => {
                return ProbeResponse {
                    status: 0,
                    elapsed_ms: elapsed_ms(started),
                    cloudflare: false,
                    reason: err,
                };
            }
        })
        .await;

    match response {
        Ok(response) => {
            let status = response.status().as_u16();
            let headers = response.headers().clone();
            let cloudflare_edge = cloudflare_edge_headers(&headers);
            let protection_reason = protection_header_reason(&headers);

            let protected = protection_reason.is_some();
            let reason = if let Some(reason) = protection_reason {
                reason.to_string()
            } else if (200..400).contains(&status) {
                let prefix = match probe_kind {
                    ProbeKind::Get => "HTTP request",
                    ProbeKind::GraphQlPost => "GraphQL POST request",
                };

                if cloudflare_edge {
                    format!("{prefix} completed through Cloudflare CDN")
                } else {
                    format!("{prefix} completed")
                }
            } else {
                format!("{} returned HTTP {status}", probe_kind.label())
            };

            ProbeResponse {
                status,
                elapsed_ms: elapsed_ms(started),
                cloudflare: protected,
                reason,
            }
        }
        Err(err) => ProbeResponse {
            status: 0,
            elapsed_ms: elapsed_ms(started),
            cloudflare: false,
            reason: format!("Request failed: {err}"),
        },
    }
}

fn client(settings: &Settings) -> Client {
    let timeout = Duration::from_millis(settings.request_timeout_ms);
    let mut client = Client::new();
    client.set_connect_timeout(timeout);
    client.set_first_byte_timeout(timeout);
    client.set_between_bytes_timeout(Duration::from_millis(3_000));
    client
}

fn build_probe_request(url: &str, probe_kind: ProbeKind) -> Result<Request<Body>, String> {
    match probe_kind {
        ProbeKind::Get => build_get_request(url, "text/html,*/*"),
        ProbeKind::GraphQlPost => Request::builder()
            .method(Method::POST)
            .uri(url)
            .header("accept", "application/json,*/*")
            .header("content-type", "application/json")
            .header(
                "user-agent",
                "Mozilla/5.0 (compatible; InkdexStatusBot/0.1)",
            )
            .body(Body::from(r#"{"query":"query { __typename }"}"#))
            .map_err(|err| format!("Failed to build GraphQL POST request for {url}: {err}")),
    }
}

fn build_get_request(url: &str, accept: &str) -> Result<Request<Body>, String> {
    Request::builder()
        .method(Method::GET)
        .uri(url)
        .header("accept", accept)
        .header(
            "user-agent",
            "Mozilla/5.0 (compatible; InkdexStatusBot/0.1)",
        )
        .body(Body::empty())
        .map_err(|err| format!("Failed to build request for {url}: {err}"))
}

fn url_host(url: &str) -> Option<String> {
    let scheme_end = url.find("://")?;
    let rest = &url[scheme_end + 3..];
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host)
        .split(':')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();

    if host.is_empty() { None } else { Some(host) }
}

fn url_path(url: &str) -> &str {
    let Some(scheme_end) = url.find("://") else {
        return "";
    };
    let rest = &url[scheme_end + 3..];
    let Some(path_start) = rest.find('/') else {
        return "";
    };
    &rest[path_start..]
}

fn cloudflare_edge_headers(headers: &HeaderMap) -> bool {
    header_contains(headers, "server", "cloudflare") || headers.contains_key("cf-ray")
}

fn protection_header_reason(headers: &HeaderMap) -> Option<&'static str> {
    if header_contains(headers, "cf-mitigated", "challenge") {
        return Some("Cloudflare challenge detected from cf-mitigated header");
    }

    None
}

fn header_contains(headers: &HeaderMap, name: &str, needle: &str) -> bool {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.to_ascii_lowercase().contains(needle))
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}
