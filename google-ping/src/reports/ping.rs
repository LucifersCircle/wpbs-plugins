use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    config::Settings,
    http::{self, ProbeKind},
    progress::ProgressReporter,
    registry::{self, Catalog, ExtensionTarget},
    state,
};

pub const CURRENT_CLASSIFIER_VERSION: u16 = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PingReport {
    #[serde(default)]
    pub classifier_version: u16,
    pub generated_at_unix: u64,
    pub registry_url: String,
    pub total: usize,
    pub working_count: usize,
    pub cloudflare_count: usize,
    pub failed_count: usize,
    pub new_sites: Vec<NewSite>,
    pub baseline_created: bool,
    pub results: Vec<ExtensionStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewSite {
    pub name: String,
    pub website_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionStatus {
    pub name: String,
    pub normalized_name: String,
    pub repository: String,
    pub website_url: Option<String>,
    #[serde(default)]
    pub probe_kind: ProbeKind,
    pub build_time: Option<String>,
    pub status: StatusKind,
    pub http_status: Option<u16>,
    pub elapsed_ms: Option<u64>,
    #[serde(default)]
    pub first_failed_at_unix: Option<u64>,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StatusKind {
    Working,
    Cloudflare,
    Failed,
}

impl PingReport {
    pub fn classifier_is_current(&self) -> bool {
        self.classifier_version >= CURRENT_CLASSIFIER_VERSION
    }

    pub fn find_extension(&self, query: &str) -> Option<&ExtensionStatus> {
        let normalized_query = registry::normalize_name(query);

        self.results
            .iter()
            .find(|result| result.normalized_name == normalized_query)
            .or_else(|| {
                self.results.iter().find(|result| {
                    result.normalized_name.contains(&normalized_query)
                        || normalized_query.contains(&result.normalized_name)
                })
            })
    }
}

pub fn load_latest_ping_report() -> Result<Option<PingReport>, String> {
    state::load_latest_ping_report()
}

pub async fn check_saved_extension_live(
    saved: &ExtensionStatus,
    settings: &Settings,
    checked_at_unix: u64,
    saved_report_at_unix: u64,
) -> ExtensionStatus {
    let Some(website_url) = saved.website_url.clone() else {
        return ExtensionStatus {
            status: StatusKind::Failed,
            http_status: None,
            elapsed_ms: None,
            first_failed_at_unix: saved.first_failed_at_unix.or(Some(checked_at_unix)),
            reason: "No website URL is available in the latest ping report".to_string(),
            ..saved.clone()
        };
    };

    let probe_kind = ProbeKind::from_saved_or_url(saved.probe_kind, &website_url);
    let probe = http::probe(&website_url, probe_kind, settings).await;
    let status = status_from_probe(&probe);
    let first_failed_at_unix = if status == StatusKind::Failed {
        saved
            .first_failed_at_unix
            .or_else(|| (saved.status == StatusKind::Failed).then_some(saved_report_at_unix))
            .or(Some(checked_at_unix))
    } else {
        None
    };

    ExtensionStatus {
        probe_kind,
        status,
        http_status: (probe.status > 0).then_some(probe.status),
        elapsed_ms: Some(probe.elapsed_ms),
        first_failed_at_unix,
        reason: probe.reason,
        ..saved.clone()
    }
}

pub async fn run_ping_report(
    settings: &Settings,
    catalog: &Catalog,
    progress: &mut ProgressReporter<'_>,
) -> Result<PingReport, String> {
    let previous = state::load_latest_ping_report()?;
    let previous_sites = previous
        .as_ref()
        .map_or_else(BTreeMap::new, previous_site_map);
    let baseline_created = previous.is_none();

    let total = catalog.targets.len();
    let mut results = Vec::with_capacity(total);

    for (index, target) in catalog.targets.iter().cloned().enumerate() {
        results.push(check_extension(target, settings).await);
        progress.update("Pinging captured extension sites", index + 1, total);
    }

    let generated_at_unix = now_unix();
    apply_failure_history(&mut results, previous.as_ref(), generated_at_unix);

    let new_sites = if baseline_created {
        Vec::new()
    } else {
        detect_new_sites(&results, &previous_sites)
    };

    let working_count = results
        .iter()
        .filter(|result| result.status == StatusKind::Working)
        .count();
    let cloudflare_count = results
        .iter()
        .filter(|result| result.status == StatusKind::Cloudflare)
        .count();
    let failed_count = results
        .iter()
        .filter(|result| result.status == StatusKind::Failed)
        .count();
    Ok(PingReport {
        classifier_version: CURRENT_CLASSIFIER_VERSION,
        generated_at_unix,
        registry_url: catalog.registry_url.clone(),
        total: results.len(),
        working_count,
        cloudflare_count,
        failed_count,
        new_sites,
        baseline_created,
        results,
    })
}

async fn check_extension(target: ExtensionTarget, settings: &Settings) -> ExtensionStatus {
    let Some(website_url) = target.website_url.clone() else {
        return ExtensionStatus {
            name: target.name,
            normalized_name: target.normalized_name,
            repository: target.repository,
            website_url: None,
            probe_kind: target.probe_kind,
            build_time: target.build_time,
            status: StatusKind::Failed,
            http_status: None,
            elapsed_ms: None,
            first_failed_at_unix: None,
            reason: target
                .discovery_error
                .unwrap_or_else(|| "No website URL discovered".to_string()),
        };
    };

    let probe_kind = ProbeKind::from_saved_or_url(target.probe_kind, &website_url);
    let probe = http::probe(&website_url, probe_kind, settings).await;

    ExtensionStatus {
        name: target.name,
        normalized_name: target.normalized_name,
        repository: target.repository,
        website_url: Some(website_url),
        probe_kind,
        build_time: target.build_time,
        status: status_from_probe(&probe),
        http_status: (probe.status > 0).then_some(probe.status),
        elapsed_ms: Some(probe.elapsed_ms),
        first_failed_at_unix: None,
        reason: probe.reason,
    }
}

fn status_from_probe(probe: &http::ProbeResponse) -> StatusKind {
    if probe.cloudflare {
        StatusKind::Cloudflare
    } else if (200..400).contains(&probe.status) {
        StatusKind::Working
    } else {
        StatusKind::Failed
    }
}

fn apply_failure_history(
    results: &mut [ExtensionStatus],
    previous: Option<&PingReport>,
    generated_at_unix: u64,
) {
    for result in results {
        if result.status != StatusKind::Failed {
            result.first_failed_at_unix = None;
            continue;
        }

        result.first_failed_at_unix = previous
            .and_then(|report| {
                report
                    .results
                    .iter()
                    .find(|previous_result| same_failed_target(previous_result, result))
                    .map(|previous_result| {
                        previous_result
                            .first_failed_at_unix
                            .unwrap_or(report.generated_at_unix)
                    })
            })
            .or(Some(generated_at_unix));
    }
}

fn same_failed_target(previous: &ExtensionStatus, current: &ExtensionStatus) -> bool {
    previous.status == StatusKind::Failed
        && previous.normalized_name == current.normalized_name
        && previous.website_url == current.website_url
}

fn previous_site_map(report: &PingReport) -> BTreeMap<String, String> {
    report
        .results
        .iter()
        .filter_map(|result| {
            Some((
                result.normalized_name.clone(),
                result.website_url.as_ref()?.clone(),
            ))
        })
        .collect()
}

fn detect_new_sites(
    results: &[ExtensionStatus],
    previous_sites: &BTreeMap<String, String>,
) -> Vec<NewSite> {
    let previous_names = previous_sites.keys().cloned().collect::<BTreeSet<_>>();

    results
        .iter()
        .filter_map(|result| {
            let website_url = result.website_url.as_ref()?;
            let is_new_extension = !previous_names.contains(&result.normalized_name);
            let changed_site = previous_sites
                .get(&result.normalized_name)
                .is_some_and(|previous_url| previous_url != website_url);

            if is_new_extension || changed_site {
                Some(NewSite {
                    name: result.name.clone(),
                    website_url: website_url.clone(),
                })
            } else {
                None
            }
        })
        .collect()
}

fn now_unix() -> u64 {
    state::now_unix()
}
