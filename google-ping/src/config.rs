use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::reports::ReportKind;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub registry_base_url: String,
    pub metadata_url: String,
    pub discovery_cron: String,
    pub ping_cron: String,
    pub request_timeout_ms: u64,
    pub max_extensions: Option<usize>,
    pub progress: ProgressSettings,
    pub channels: ReportChannels,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReportChannels {
    pub ping: Option<String>,
    pub tests: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressSettings {
    pub discord: bool,
    pub every: usize,
}

impl Default for ProgressSettings {
    fn default() -> Self {
        Self {
            discord: false,
            every: 10,
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            registry_base_url: default_registry_base_url(),
            metadata_url: default_metadata_url(),
            discovery_cron: default_discovery_cron(),
            ping_cron: default_ping_cron(),
            request_timeout_ms: 10_000,
            max_extensions: None,
            progress: ProgressSettings::default(),
            channels: ReportChannels::default(),
        }
    }
}

impl Settings {
    pub fn from_json(input: &str) -> Self {
        let value = serde_json::from_str::<Value>(input).unwrap_or(Value::Null);
        let mut settings = Self::default();

        if let Some(registry_base_url) = read_string(&value, "registry_base_url") {
            settings.registry_base_url = trim_trailing_slash(&registry_base_url);
            settings.metadata_url = format!("{}/metadata.json", settings.registry_base_url);
        }

        if let Some(metadata_url) = read_string(&value, "metadata_url") {
            settings.metadata_url = metadata_url;
        }

        if let Some(daily_cron) = read_string(&value, "daily_cron") {
            settings.ping_cron = daily_cron;
        }

        if let Some(discovery_cron) = read_string(&value, "discovery_cron") {
            settings.discovery_cron = discovery_cron;
        }

        if let Some(ping_cron) = read_string(&value, "ping_cron") {
            settings.ping_cron = ping_cron;
        }

        if let Some(request_timeout_ms) = read_u64(&value, "request_timeout_ms") {
            settings.request_timeout_ms = request_timeout_ms.max(1_000);
        }

        settings.max_extensions = read_u64(&value, "max_extensions")
            .and_then(|value| usize::try_from(value).ok())
            .filter(|value| *value > 0);

        if let Some(channel_id) = read_string(&value, "report_channel_id") {
            settings.channels.ping = Some(channel_id);
        }

        if let Some(channel_id) = read_string(&value, "ping_report_channel_id") {
            settings.channels.ping = Some(channel_id);
        }

        if let Some(channel_id) = read_string(&value, "tests_report_channel_id") {
            settings.channels.tests = Some(channel_id);
        }

        if let Some(channels) = value.get("channels").and_then(Value::as_object) {
            if let Some(channel_id) = read_string_from_object(channels, "ping") {
                settings.channels.ping = Some(channel_id);
            }

            if let Some(channel_id) = read_string_from_object(channels, "tests") {
                settings.channels.tests = Some(channel_id);
            }
        }

        if let Some(progress) = value.get("progress").and_then(Value::as_object) {
            if let Some(discord) = progress.get("discord").and_then(Value::as_bool) {
                settings.progress.discord = discord;
            }

            if let Some(every) = progress
                .get("every")
                .and_then(|value| {
                    value
                        .as_u64()
                        .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
                })
                .and_then(|value| usize::try_from(value).ok())
                .filter(|value| *value > 0)
            {
                settings.progress.every = every;
            }
        }

        settings
    }

    pub fn scheduled_jobs(&self) -> Vec<String> {
        if self.discovery_cron == self.ping_cron {
            vec![self.discovery_cron.clone()]
        } else {
            vec![self.discovery_cron.clone(), self.ping_cron.clone()]
        }
    }

    pub fn channel_for(&self, kind: ReportKind) -> Option<&str> {
        match kind {
            ReportKind::Ping => self.channels.ping.as_deref(),
            ReportKind::Tests => self.channels.tests.as_deref(),
        }
    }

    pub fn set_channel(&mut self, kind: ReportKind, channel_id: String) {
        match kind {
            ReportKind::Ping => self.channels.ping = Some(channel_id),
            ReportKind::Tests => self.channels.tests = Some(channel_id),
        }
    }

    pub fn next_ping_check_unix(&self, now_unix: u64) -> Option<u64> {
        let interval_seconds = match self.ping_cron.as_str() {
            "0 */3 * * * * *" => 3 * 60,
            "0 0 * * * * *" => 60 * 60,
            _ => return None,
        };

        Some(
            now_unix
                .saturating_div(interval_seconds)
                .saturating_add(1)
                .saturating_mul(interval_seconds),
        )
    }

    pub fn ping_schedule_label(&self) -> Option<&'static str> {
        match self.ping_cron.as_str() {
            "0 */3 * * * * *" => Some("Every 3 minutes"),
            "0 0 * * * * *" => Some("Every hour"),
            "0 0 */6 * * * *" => Some("Every 6 hours"),
            "0 0 */12 * * * *" => Some("Every 12 hours"),
            "0 0 9 * * * *" => Some("Daily at 09:00 local time"),
            _ => None,
        }
    }
}

fn default_registry_base_url() -> String {
    "https://inkdex.github.io/extensions/0.9/stable".to_string()
}

fn default_metadata_url() -> String {
    format!("{}/metadata.json", default_registry_base_url())
}

fn default_discovery_cron() -> String {
    "0 0 */6 * * * *".to_string()
}

fn default_ping_cron() -> String {
    "0 */3 * * * * *".to_string()
}

fn read_string(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(value_to_string)
}

fn read_string_from_object(object: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    object.get(key).and_then(value_to_string)
}

fn read_u64(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(|value| {
        value
            .as_u64()
            .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
    })
}

fn value_to_string(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::to_string)
        .or_else(|| value.as_u64().map(|value| value.to_string()))
        .filter(|value| !value.trim().is_empty())
}

fn trim_trailing_slash(value: &str) -> String {
    value.trim_end_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_ping_check_uses_next_three_minute_boundary() {
        let settings = Settings::default();

        assert_eq!(settings.next_ping_check_unix(181), Some(360));
        assert_eq!(settings.next_ping_check_unix(360), Some(540));
    }
}
