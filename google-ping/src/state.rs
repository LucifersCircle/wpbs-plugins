use std::collections::BTreeMap;

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{
    config::{ReportChannels, Settings},
    registry::Catalog,
    reports::{ReportInterval, ReportKind, ScheduleKind, ping::PingReport},
};

const SETTINGS_KEY: &str = "settings";
const CHANNEL_OVERRIDES_KEY: &str = "settings/channel-overrides";
const INTERVAL_OVERRIDES_KEY: &str = "settings/interval-overrides";
const LATEST_CATALOG_KEY: &str = "catalog/latest";
const LATEST_PING_REPORT_KEY: &str = "reports/ping/latest";
const PING_REPORT_MESSAGES_KEY: &str = "reports/ping/messages";
const JOB_REGISTRATIONS_KEY: &str = "jobs/registrations";
const RUN_PROGRESS_KEY: &str = "run/progress";

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum JobKind {
    Discovery,
    Ping,
    DiscoveryAndPing,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRegistrations {
    pub jobs: BTreeMap<String, JobKind>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReportIntervalOverrides {
    pub discovery: Option<ReportInterval>,
    pub ping: Option<ReportInterval>,
    pub tests: Option<ReportInterval>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunProgress {
    pub title: String,
    pub phase: String,
    pub completed: usize,
    pub total: usize,
    pub updated_at_unix: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PingReportMessages {
    pub channel_id: String,
    pub message_ids: Vec<u64>,
}

pub fn save_settings(settings: &Settings) -> Result<(), String> {
    save_json(SETTINGS_KEY, settings)
}

pub fn load_settings() -> Result<Option<Settings>, String> {
    load_json(SETTINGS_KEY)
}

pub fn apply_channel_overrides(settings: &mut Settings) -> Result<(), String> {
    let Some(overrides) = load_channel_overrides()? else {
        return Ok(());
    };

    if let Some(channel_id) = overrides.ping {
        settings.channels.ping = Some(channel_id);
    }

    if let Some(channel_id) = overrides.tests {
        settings.channels.tests = Some(channel_id);
    }

    Ok(())
}

pub fn apply_interval_overrides(settings: &mut Settings) -> Result<(), String> {
    let Some(overrides) = load_interval_overrides()? else {
        return Ok(());
    };

    if let Some(interval) = overrides.discovery {
        settings.discovery_cron = interval.cron().to_string();
    }

    if let Some(interval) = overrides.ping {
        settings.ping_cron = interval.cron().to_string();
    }

    Ok(())
}

pub fn save_channel_override(
    kind: ReportKind,
    channel_id: String,
) -> Result<ReportChannels, String> {
    let mut overrides = load_channel_overrides()?.unwrap_or_default();

    match kind {
        ReportKind::Ping => overrides.ping = Some(channel_id),
        ReportKind::Tests => overrides.tests = Some(channel_id),
    }

    save_json(CHANNEL_OVERRIDES_KEY, &overrides)?;
    Ok(overrides)
}

pub fn save_interval_override(
    kind: ScheduleKind,
    interval: ReportInterval,
) -> Result<ReportIntervalOverrides, String> {
    let mut overrides = load_interval_overrides()?.unwrap_or_default();

    match kind {
        ScheduleKind::Discovery => overrides.discovery = Some(interval),
        ScheduleKind::Ping => overrides.ping = Some(interval),
        ScheduleKind::Tests => overrides.tests = Some(interval),
    }

    save_json(INTERVAL_OVERRIDES_KEY, &overrides)?;
    Ok(overrides)
}

pub fn save_latest_catalog(catalog: &Catalog) -> Result<(), String> {
    save_json(LATEST_CATALOG_KEY, catalog)
}

pub fn load_latest_catalog() -> Result<Option<Catalog>, String> {
    load_json(LATEST_CATALOG_KEY)
}

pub fn save_latest_ping_report(report: &PingReport) -> Result<(), String> {
    save_json(LATEST_PING_REPORT_KEY, report)
}

pub fn load_latest_ping_report() -> Result<Option<PingReport>, String> {
    load_json(LATEST_PING_REPORT_KEY)
}

pub fn save_ping_report_messages(messages: &PingReportMessages) -> Result<(), String> {
    save_json(PING_REPORT_MESSAGES_KEY, messages)
}

pub fn load_ping_report_messages() -> Result<Option<PingReportMessages>, String> {
    load_json(PING_REPORT_MESSAGES_KEY)
}

pub fn save_job_registrations(
    settings: &Settings,
    result: &crate::wpbs::plugin::core_import_types::RegistrationsResult,
) -> Result<usize, String> {
    let mut jobs = BTreeMap::new();
    let mut registration_error = None;

    if let Some(services) = &result.services {
        match &services.job_scheduler {
            Some(Ok(job_scheduler)) => match &job_scheduler.scheduled_jobs {
                Some(Ok(scheduled_jobs)) => {
                    for (cron, job_id_result) in scheduled_jobs {
                        match job_id_result {
                            Ok(job_id) => {
                                jobs.insert(job_id.clone(), job_kind_for_cron(settings, cron));
                            }
                            Err(err) => registration_error = Some(err.clone()),
                        }
                    }
                }
                Some(Err(err)) => registration_error = Some(err.clone()),
                None => {}
            },
            Some(Err(err)) => registration_error = Some(err.clone()),
            None => {}
        }
    }

    if jobs.is_empty() {
        return registration_error.map_or(Ok(0), |err| {
            Err(format!("Could not schedule reports: {err}"))
        });
    }

    let job_count = jobs.len();
    save_json(JOB_REGISTRATIONS_KEY, &JobRegistrations { jobs }).map(|()| job_count)
}

pub fn load_job_kind(job_id: &str) -> Result<Option<JobKind>, String> {
    Ok(load_json::<JobRegistrations>(JOB_REGISTRATIONS_KEY)?
        .and_then(|registrations| registrations.jobs.get(job_id).copied()))
}

pub fn save_run_progress(progress: &RunProgress) -> Result<(), String> {
    save_json(RUN_PROGRESS_KEY, progress)
}

pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn save_json<T: Serialize>(key: &str, value: &T) -> Result<(), String> {
    let bytes =
        serde_json::to_vec(value).map_err(|err| format!("Failed to serialize state: {err}"))?;
    crate::wpbs::plugin::core_import_functions::set_state(key, &bytes)
        .map_err(|err| format!("Failed to save plugin state at {key}: {err}"))
}

fn load_channel_overrides() -> Result<Option<ReportChannels>, String> {
    load_json(CHANNEL_OVERRIDES_KEY)
}

fn load_interval_overrides() -> Result<Option<ReportIntervalOverrides>, String> {
    load_json(INTERVAL_OVERRIDES_KEY)
}

fn load_json<T: DeserializeOwned>(key: &str) -> Result<Option<T>, String> {
    let Some(bytes) = crate::wpbs::plugin::core_import_functions::get_state(key)
        .map_err(|err| format!("Failed to load plugin state at {key}: {err}"))?
    else {
        return Ok(None);
    };

    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|err| format!("Failed to parse plugin state at {key}: {err}"))
}

fn job_kind_for_cron(settings: &Settings, cron: &str) -> JobKind {
    if settings.discovery_cron == settings.ping_cron {
        return JobKind::DiscoveryAndPing;
    }

    if cron == settings.discovery_cron {
        JobKind::Discovery
    } else {
        JobKind::Ping
    }
}
