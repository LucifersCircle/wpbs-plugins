use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    config::Settings,
    http::{self, ProbeKind},
    progress::ProgressReporter,
    state,
};

const CURRENT_CATALOG_VERSION: u16 = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::struct_field_names)]
pub struct Catalog {
    #[serde(default)]
    pub catalog_version: u16,
    pub generated_at_unix: u64,
    pub registry_url: String,
    pub targets: Vec<ExtensionTarget>,
}

impl Catalog {
    fn is_current(&self) -> bool {
        self.catalog_version >= CURRENT_CATALOG_VERSION
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionTarget {
    pub name: String,
    pub normalized_name: String,
    pub repository: String,
    pub index_url: String,
    pub website_url: Option<String>,
    #[serde(default)]
    pub probe_kind: ProbeKind,
    pub build_time: Option<String>,
    pub discovery_error: Option<String>,
}

#[derive(Debug, Clone)]
struct DiscoveredProbe {
    url: String,
    probe_kind: ProbeKind,
}

#[derive(Debug, Clone, Copy)]
struct Candidate {
    score: i32,
    probe_kind: ProbeKind,
}

pub async fn ensure_catalog(
    settings: &Settings,
    progress: &mut ProgressReporter<'_>,
) -> Result<Catalog, String> {
    if let Some(catalog) = state::load_latest_catalog()? {
        if catalog.is_current() {
            progress.update(
                "Using saved extension catalog",
                catalog.targets.len(),
                catalog.targets.len(),
            );
            return Ok(catalog);
        }

        progress.update("Saved extension catalog is stale; refreshing", 0, 0);
    } else {
        progress.update("No saved extension catalog found; refreshing", 0, 0);
    }

    let catalog = refresh_catalog(settings, progress).await?;
    state::save_latest_catalog(&catalog)?;
    Ok(catalog)
}

pub async fn refresh_catalog(
    settings: &Settings,
    progress: &mut ProgressReporter<'_>,
) -> Result<Catalog, String> {
    progress.update("Fetching Inkdex extension metadata", 0, 0);
    let metadata = http::get_text(&settings.metadata_url, settings).await?;
    let metadata = serde_json::from_str::<Value>(&metadata.body)
        .map_err(|err| format!("Failed to parse registry metadata JSON: {err}"))?;
    let mut targets = Vec::new();

    let repositories = metadata
        .as_object()
        .ok_or_else(|| "Registry metadata root was not an object".to_string())?;

    for (repository, extensions) in repositories {
        let Some(extensions) = extensions.as_object() else {
            continue;
        };

        for (name, details) in extensions {
            let index_url = format!("{}/{name}/index.js", settings.registry_base_url);
            let build_time = details
                .get("build_time")
                .and_then(Value::as_str)
                .map(str::to_string);

            targets.push(ExtensionTarget {
                name: name.clone(),
                normalized_name: normalize_name(name),
                repository: repository.clone(),
                index_url,
                website_url: None,
                probe_kind: ProbeKind::Get,
                build_time,
                discovery_error: None,
            });
        }
    }

    targets.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
    });

    if let Some(max_extensions) = settings.max_extensions {
        targets.truncate(max_extensions);
    }

    let total = targets.len();
    for (index, target) in targets.iter_mut().enumerate() {
        match discover_probe_target(target, settings).await {
            Ok(probe) => {
                target.website_url = Some(probe.url);
                target.probe_kind = probe.probe_kind;
            }
            Err(err) => target.discovery_error = Some(err),
        }

        progress.update("Capturing extension site URLs", index + 1, total);
    }

    Ok(Catalog {
        catalog_version: CURRENT_CATALOG_VERSION,
        generated_at_unix: state::now_unix(),
        registry_url: settings.registry_base_url.clone(),
        targets,
    })
}

pub fn normalize_name(value: &str) -> String {
    value
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect()
}

async fn discover_probe_target(
    target: &ExtensionTarget,
    settings: &Settings,
) -> Result<DiscoveredProbe, String> {
    let index = http::get_text(&target.index_url, settings).await?;
    extract_primary_probe_target(&index.body)
        .ok_or_else(|| format!("No website URL discovered in {}", target.index_url))
}

fn extract_primary_probe_target(source: &str) -> Option<DiscoveredProbe> {
    let mut candidates = BTreeMap::<String, Candidate>::new();

    for literal in string_literals(source) {
        let literal = static_url_prefix(&literal);

        if !literal.starts_with("http://") && !literal.starts_with("https://") {
            continue;
        }

        if !is_candidate_url(literal) {
            continue;
        }

        let Some(url) = candidate_probe_url(literal) else {
            continue;
        };

        let probe_kind = ProbeKind::from_url(&url);
        let score = score_candidate_url(literal, probe_kind);
        candidates
            .entry(url)
            .and_modify(|current| {
                if score > current.score {
                    *current = Candidate { score, probe_kind };
                }
            })
            .or_insert(Candidate { score, probe_kind });
    }

    candidates
        .into_iter()
        .max_by(|a, b| {
            a.1.score
                .cmp(&b.1.score)
                .then_with(|| b.0.len().cmp(&a.0.len()))
        })
        .map(|(url, candidate)| DiscoveredProbe {
            url,
            probe_kind: candidate.probe_kind,
        })
}

fn static_url_prefix(literal: &str) -> &str {
    literal
        .find("${")
        .map_or(literal, |index| &literal[..index])
}

fn string_literals(source: &str) -> Vec<String> {
    let bytes = source.as_bytes();
    let mut literals = Vec::new();
    let mut index = 0;

    while index < bytes.len() {
        let quote = bytes[index];
        if quote != b'\'' && quote != b'"' && quote != b'`' {
            index += 1;
            continue;
        }

        index += 1;
        let start = index;
        let mut escaped = false;
        while index < bytes.len() {
            let current = bytes[index];
            if escaped {
                escaped = false;
                index += 1;
                continue;
            }

            if current == b'\\' {
                escaped = true;
                index += 1;
                continue;
            }

            if current == quote {
                if let Ok(literal) = std::str::from_utf8(&bytes[start..index]) {
                    literals.push(literal.to_string());
                }
                index += 1;
                break;
            }

            index += 1;
        }
    }

    literals
}

fn is_candidate_url(url: &str) -> bool {
    if url.contains("${") || url.contains("$1") || url.contains("$2") {
        return false;
    }

    let Some(host) = url_host(url) else {
        return false;
    };

    let blocked_hosts = [
        "w3.org",
        "ibm.com",
        "github.com",
        "github.io",
        "raw.githubusercontent.com",
        "opencollective.com",
        "patreon.com",
        "placehold.co",
    ];

    if blocked_hosts.iter().any(|blocked| host.ends_with(blocked)) {
        return false;
    }

    let blocked_suffixes = [
        ".png", ".jpg", ".jpeg", ".gif", ".webp", ".avif", ".svg", ".css", ".js",
    ];

    let lowercase = url.to_ascii_lowercase();
    !blocked_suffixes.iter().any(|suffix| {
        lowercase
            .split('?')
            .next()
            .unwrap_or_default()
            .ends_with(suffix)
    })
}

fn score_candidate_url(url: &str, probe_kind: ProbeKind) -> i32 {
    let host = url_host(url).unwrap_or_default();
    let path = url_path(url);
    let mut score = 0;

    if path.is_empty() || path == "/" {
        score += 30;
    }

    if host.starts_with("api.") || host.starts_with("graphql.") || host.contains("-api.") {
        score -= 25;
    }

    if host.contains("cdn") || host.contains("mfcdn") {
        score -= 25;
    }

    if path.contains("/api/") || path.contains("graphql") {
        score -= 20;
    }

    if path.contains("oauth") || path.contains("authorize") {
        score -= 30;
    }

    if probe_kind == ProbeKind::GraphQlPost {
        score += 10;
    }

    score
}

fn candidate_probe_url(url: &str) -> Option<String> {
    match ProbeKind::from_url(url) {
        ProbeKind::Get => url_origin(url),
        ProbeKind::GraphQlPost => url_without_query_or_fragment(url),
    }
}

fn url_origin(url: &str) -> Option<String> {
    let scheme_end = url.find("://")?;
    let scheme = &url[..scheme_end].to_ascii_lowercase();
    let rest = &url[scheme_end + 3..];
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = rest[..authority_end].to_ascii_lowercase();

    if authority.is_empty() || authority.contains('$') || authority.contains('{') {
        return None;
    }

    Some(format!("{scheme}://{authority}"))
}

fn url_without_query_or_fragment(url: &str) -> Option<String> {
    let scheme_end = url.find("://")?;
    let scheme = &url[..scheme_end].to_ascii_lowercase();
    let rest = &url[scheme_end + 3..];
    let end = rest.find(['?', '#']).unwrap_or(rest.len());
    let endpoint = rest[..end].trim_end_matches('/').to_ascii_lowercase();

    if endpoint.is_empty() || endpoint.contains('$') || endpoint.contains('{') {
        return None;
    }

    Some(format!("{scheme}://{endpoint}"))
}

fn url_host(url: &str) -> Option<String> {
    let origin = url_origin(url)?;
    let host = origin.split("://").nth(1)?;
    let host = host
        .rsplit_once('@')
        .map_or(host, |(_, host)| host)
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

#[cfg(test)]
mod tests {
    use super::extract_primary_probe_target;

    #[test]
    fn extracts_static_origin_from_templated_url() {
        let source = "const requestUrl = `https://api.mangaupdates.com${path}`;";
        let probe = extract_primary_probe_target(source).expect("probe target");

        assert_eq!(probe.url, "https://api.mangaupdates.com");
    }

    #[test]
    fn rejects_url_with_dynamic_host() {
        let source = "const requestUrl = `https://${host}/v1/series`;";

        assert!(extract_primary_probe_target(source).is_none());
    }
}
