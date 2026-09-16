pub mod ping;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ReportKind {
    Ping,
    Tests,
}

impl ReportKind {
    pub fn from_slug(value: &str) -> Option<Self> {
        match value {
            "ping" => Some(Self::Ping),
            "tests" => Some(Self::Tests),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Ping => "Ping report",
            Self::Tests => "Tests report",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduleKind {
    Discovery,
    Ping,
    Tests,
}

impl ScheduleKind {
    pub fn from_slug(value: &str) -> Option<Self> {
        match value {
            "discovery" => Some(Self::Discovery),
            "ping" => Some(Self::Ping),
            "tests" => Some(Self::Tests),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Discovery => "Extension list refresh",
            Self::Ping => "Ping report",
            Self::Tests => "Tests report",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportInterval {
    EveryThreeMinutes,
    Hourly,
    EverySixHours,
    EveryTwelveHours,
    Daily,
}

impl ReportInterval {
    pub fn from_slug(value: &str) -> Option<Self> {
        match value {
            "every_3_minutes" => Some(Self::EveryThreeMinutes),
            "hourly" => Some(Self::Hourly),
            "every_6_hours" => Some(Self::EverySixHours),
            "every_12_hours" => Some(Self::EveryTwelveHours),
            "daily" => Some(Self::Daily),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::EveryThreeMinutes => "Every 3 minutes",
            Self::Hourly => "Every hour",
            Self::EverySixHours => "Every 6 hours",
            Self::EveryTwelveHours => "Every 12 hours",
            Self::Daily => "Daily at 09:00",
        }
    }

    pub fn cron(self) -> &'static str {
        match self {
            Self::EveryThreeMinutes => "0 */3 * * * * *",
            Self::Hourly => "0 0 * * * * *",
            Self::EverySixHours => "0 0 */6 * * * *",
            Self::EveryTwelveHours => "0 0 */12 * * * *",
            Self::Daily => "0 0 9 * * * *",
        }
    }
}
