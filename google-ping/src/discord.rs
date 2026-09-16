use std::{collections::BTreeMap, fmt::Write as _};

use serde_json::{Value, json};

use crate::{
    config::Settings,
    reports::{
        ReportInterval, ReportKind, ScheduleKind,
        ping::{ExtensionStatus, NewSite, PingReport, StatusKind},
    },
    state::{PingReportMessages, RunProgress},
    wpbs,
};

const GREEN: u32 = 0x002e_cc71;
const YELLOW: u32 = 0x00f1_c40f;
const ORANGE: u32 = 0x00e6_7e22;
const RED: u32 = 0x00e7_4c3c;
const BLUE: u32 = 0x0034_98db;

pub fn status_command_json(name: &str) -> Value {
    json!({
        "application_id": null,
        "contexts": [0],
        "default_member_permissions": null,
        "description": "Live ping an Inkdex extension using the latest report URL",
        "description_localizations": null,
        "dm_permission": null,
        "guild_id": null,
        "id": null,
        "integration_types": [0],
        "type": 1,
        "name": name,
        "name_localizations": null,
        "nsfw": false,
        "options": [{
            "type": 3,
            "name": "extension",
            "description": "Extension name, for example MangaFire",
            "required": true
        }],
        "version": "1"
    })
}

pub fn set_report_channel_command_json(name: &str) -> Value {
    json!({
        "application_id": null,
        "contexts": [0],
        "default_member_permissions": null,
        "description": "Set the channel for extension status reports",
        "description_localizations": null,
        "dm_permission": null,
        "guild_id": null,
        "id": null,
        "integration_types": [0],
        "type": 1,
        "name": name,
        "name_localizations": null,
        "nsfw": false,
        "options": [
            {
                "type": 3,
                "name": "report",
                "description": "Report type to configure",
                "required": true,
                "choices": [
                    {
                        "name": "Ping report",
                        "value": "ping"
                    },
                    {
                        "name": "Tests report",
                        "value": "tests"
                    }
                ]
            },
            {
                "type": 7,
                "name": "channel",
                "description": "Channel where this report should be posted",
                "required": true,
                "channel_types": [0, 5]
            }
        ],
        "version": "1"
    })
}

pub fn post_report_command_json(name: &str) -> Value {
    json!({
        "application_id": null,
        "contexts": [0],
        "default_member_permissions": null,
        "description": "Generate and post an extension report to its configured channel",
        "description_localizations": null,
        "dm_permission": null,
        "guild_id": null,
        "id": null,
        "integration_types": [0],
        "type": 1,
        "name": name,
        "name_localizations": null,
        "nsfw": false,
        "options": [
            {
                "type": 3,
                "name": "report",
                "description": "Report type to post",
                "required": true,
                "choices": [
                    {
                        "name": "Ping report",
                        "value": "ping"
                    },
                    {
                        "name": "Tests report",
                        "value": "tests"
                    }
                ]
            }
        ],
        "version": "1"
    })
}

pub fn post_stale_report_command_json(name: &str) -> Value {
    json!({
        "application_id": null,
        "contexts": [0],
        "default_member_permissions": null,
        "description": "Post the saved ping report without running new checks",
        "description_localizations": null,
        "dm_permission": null,
        "guild_id": null,
        "id": null,
        "integration_types": [0],
        "type": 1,
        "name": name,
        "name_localizations": null,
        "nsfw": false,
        "options": [],
        "version": "1"
    })
}

pub fn set_report_interval_command_json(name: &str) -> Value {
    json!({
        "application_id": null,
        "contexts": [0],
        "default_member_permissions": null,
        "description": "Set how often extension data and reports update",
        "description_localizations": null,
        "dm_permission": null,
        "guild_id": null,
        "id": null,
        "integration_types": [0],
        "type": 1,
        "name": name,
        "name_localizations": null,
        "nsfw": false,
        "options": [
            {
                "type": 3,
                "name": "schedule",
                "description": "Data or report schedule to update",
                "required": true,
                "choices": [
                    {
                        "name": "Extension list refresh",
                        "value": "discovery"
                    },
                    {
                        "name": "Ping report",
                        "value": "ping"
                    },
                    {
                        "name": "Tests report",
                        "value": "tests"
                    }
                ]
            },
            {
                "type": 3,
                "name": "interval",
                "description": "How often the report should run",
                "required": true,
                "choices": [
                    {
                        "name": "Every 3 minutes",
                        "value": "every_3_minutes"
                    },
                    {
                        "name": "Every hour",
                        "value": "hourly"
                    },
                    {
                        "name": "Every 6 hours",
                        "value": "every_6_hours"
                    },
                    {
                        "name": "Every 12 hours",
                        "value": "every_12_hours"
                    },
                    {
                        "name": "Daily at 09:00",
                        "value": "daily"
                    }
                ]
            }
        ],
        "version": "1"
    })
}

pub fn report_channel_saved_embed(kind: ReportKind, channel_id: &str, note: &str) -> Value {
    json!({
        "title": "Report channel updated",
        "color": GREEN,
        "description": format!("{} will post to <#{channel_id}>.\n{note}", kind.label())
    })
}

pub fn report_posted_embed(kind: ReportKind, channel_id: &str) -> Value {
    json!({
        "title": "Report posted",
        "color": GREEN,
        "description": format!("{} was posted to <#{channel_id}>.", kind.label())
    })
}

pub fn stale_report_posted_embed(channel_id: &str) -> Value {
    json!({
        "title": "Saved ping report posted",
        "color": GREEN,
        "description": format!(
            "The saved ping report was posted to <#{channel_id}> without running new checks."
        )
    })
}

pub fn report_interval_saved_embed(
    kind: ScheduleKind,
    interval: ReportInterval,
    note: &str,
) -> Value {
    json!({
        "title": "Report interval updated",
        "color": GREEN,
        "description": format!(
            "**{}:** {}.\n{note}",
            kind.label(),
            interval.label()
        )
    })
}

pub fn ping_report_embeds(report: &PingReport, settings: &Settings) -> Result<Vec<Value>, String> {
    let mut fields = Vec::new();

    if report.results.is_empty() {
        fields.push(json!({
            "name": "Extensions",
            "value": "No extensions were discovered.",
            "inline": false
        }));
    }

    if report.baseline_created {
        fields.push(json!({
            "name": "Discovery",
            "value": "Baseline created. New site detection starts on the next report.",
            "inline": false
        }));
    }

    let mut summary = format!(
        "\u{1f7e2} **Working:** {}\n\u{1f7e0} **Cloudflare:** {}\n\u{1f534} **Unavailable:** {}\n\u{1f195} **New sites:** {}\n\n**{} extensions checked**\n**Last ping:** <t:{}:R>",
        report.working_count,
        report.cloudflare_count,
        report.failed_count,
        report.new_sites.len(),
        report.total,
        report.generated_at_unix
    );
    if let Some(next_check_at_unix) = settings.next_ping_check_unix(crate::state::now_unix()) {
        let _ = write!(summary, "\n**Next ping:** <t:{next_check_at_unix}:R>");
    } else if let Some(schedule) = settings.ping_schedule_label() {
        let _ = write!(summary, "\n**Next ping:** {schedule}");
    }
    let repositories =
        repository_statuses(&report.results, &report.new_sites, report.generated_at_unix);

    let summary_embed = json!({
        "title": "Inkdex Extension Availability",
        "url": "https://github.com/inkdex/extensions",
        "description": summary,
        "color": BLUE,
        "fields": fields,
        "footer": {
            "text": "Only reports site reachability"
        }
    });

    let mut embeds = vec![summary_embed];
    embeds.extend(repositories.iter().map(repository_embed));
    validate_report_embeds(&embeds)?;
    Ok(embeds)
}

pub fn extension_status_embed(
    result: &ExtensionStatus,
    report: &PingReport,
    checked_at_unix: u64,
) -> Value {
    let color = match result.status {
        StatusKind::Working => GREEN,
        StatusKind::Cloudflare => ORANGE,
        StatusKind::Failed => RED,
    };

    let status = match result.status {
        StatusKind::Working => "Working",
        StatusKind::Cloudflare => "Cloudflare",
        StatusKind::Failed => "Failed",
    };

    let mut fields = vec![
        json!({
            "name": "Latency",
            "value": result
                .elapsed_ms.map_or_else(|| "Unavailable".to_string(), |elapsed| format!("{elapsed} ms")),
            "inline": true
        }),
        json!({
            "name": "Repository",
            "value": &result.repository,
            "inline": true
        }),
        json!({
            "name": "Checked",
            "value": format!("<t:{}:R>", checked_at_unix),
            "inline": true
        }),
    ];

    if result.status == StatusKind::Failed {
        fields.push(json!({
            "name": "Down for",
            "value": result
                .first_failed_at_unix.map_or_else(|| "Unknown".to_string(), |started_at| format_duration(checked_at_unix.saturating_sub(started_at))),
            "inline": true
        }));
    }

    if let Some(website_url) = &result.website_url {
        fields.push(json!({
            "name": "Website",
            "value": website_url,
            "inline": false
        }));
    }

    fields.push(json!({
        "name": "Availability",
        "value": public_availability_message(result),
        "inline": false
    }));

    json!({
        "title": format!("{} Extension Status", result.name),
        "description": format!("**{status}**\nLive check using the site captured <t:{}:R>.", report.generated_at_unix),
        "color": color,
        "fields": fields
    })
}

pub fn unknown_extension_embed(query: &str, report: &PingReport) -> Value {
    let examples = report
        .results
        .iter()
        .take(10)
        .map(|result| result.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");

    json!({
        "title": "Extension not found",
        "color": RED,
        "description": format!("No extension matched `{query}` in the latest report."),
        "fields": [{
            "name": "Examples",
            "value": if examples.is_empty() { "No extensions were discovered.".to_string() } else { examples },
            "inline": false
        }]
    })
}

pub fn error_response_embed(title: &str, message: &str) -> Value {
    json!({
        "title": title,
        "color": RED,
        "description": truncate_field(message, 2048)
    })
}

pub fn progress_embed(progress: &RunProgress) -> Value {
    let description = if progress.total == 0 {
        progress.phase.clone()
    } else {
        format!(
            "{}\nProgress: {}/{}",
            progress.phase, progress.completed, progress.total
        )
    };

    json!({
        "title": &progress.title,
        "color": YELLOW,
        "description": description,
        "footer": {
            "text": "Extension status progress"
        },
        "timestamp": null
    })
}

pub fn send_interaction_embed(
    interaction_id: u64,
    interaction_token: String,
    embed: Value,
) -> Result<(), String> {
    send_interaction_embeds(interaction_id, interaction_token, &[embed])
}

pub fn send_interaction_embeds(
    interaction_id: u64,
    interaction_token: String,
    embeds: &[Value],
) -> Result<(), String> {
    let response = json!({
        "type": 4,
        "data": {
            "embeds": embeds
        }
    });

    wpbs::plugin::discord_import_functions::discord_request(
        &wpbs::plugin::discord_import_types::DiscordRequests::InteractionCallback((
            interaction_id,
            interaction_token,
            true,
            response.to_string(),
        )),
    )
    .map_err(|err| format!("Failed to send Discord interaction callback: {err}"))?;

    Ok(())
}

pub fn defer_interaction(interaction_id: u64, interaction_token: String) -> Result<(), String> {
    let response = json!({ "type": 5 });

    wpbs::plugin::discord_import_functions::discord_request(
        &wpbs::plugin::discord_import_types::DiscordRequests::InteractionCallback((
            interaction_id,
            interaction_token,
            true,
            response.to_string(),
        )),
    )
    .map_err(|err| format!("Failed to defer Discord interaction: {err}"))?;

    Ok(())
}

pub fn update_interaction_embed(
    application_id: u64,
    interaction_token: String,
    embed: &Value,
) -> Result<(), String> {
    let body = json!({ "embeds": [embed] });

    wpbs::plugin::discord_import_functions::discord_request(
        &wpbs::plugin::discord_import_types::DiscordRequests::UpdateInteractionOriginal((
            application_id,
            interaction_token,
            body.to_string(),
        )),
    )
    .map_err(|err| format!("Failed to update deferred Discord interaction: {err}"))?;

    Ok(())
}

pub fn send_channel_embed(channel_id: &str, embed: &Value) -> Result<(), String> {
    send_channel_embeds(channel_id, std::slice::from_ref(embed))
}

pub fn send_channel_embeds(channel_id: &str, embeds: &[Value]) -> Result<(), String> {
    let channel_id = channel_id
        .parse::<u64>()
        .map_err(|err| format!("Invalid Discord channel id `{channel_id}`: {err}"))?;
    for embeds in embed_batches(embeds.to_vec())? {
        let body = json!({ "embeds": embeds });

        wpbs::plugin::discord_import_functions::discord_request(
            &wpbs::plugin::discord_import_types::DiscordRequests::CreateMessage((
                channel_id,
                wpbs::plugin::discord_import_types::Body::Json(body.to_string()),
            )),
        )
        .map_err(|err| channel_message_error(&err))?;
    }

    Ok(())
}

pub fn upsert_channel_embeds(
    channel_id: &str,
    embeds: Vec<Value>,
    previous: Option<&PingReportMessages>,
) -> Result<PingReportMessages, String> {
    let channel_snowflake = channel_id
        .parse::<u64>()
        .map_err(|err| format!("Invalid Discord channel id `{channel_id}`: {err}"))?;
    let batches = embed_batches(embeds)?;
    let previous_ids = previous
        .filter(|messages| messages.channel_id == channel_id)
        .map(|messages| messages.message_ids.as_slice())
        .unwrap_or_default();
    let mut message_ids = Vec::with_capacity(batches.len());

    for (index, embeds) in batches.into_iter().enumerate() {
        let body = json!({ "embeds": embeds });
        let message_id = if let Some(message_id) = previous_ids.get(index).copied() {
            match edit_channel_message(channel_snowflake, message_id, &body) {
                Ok(()) => {
                    log_info(&format!("Edited ping report message {message_id}"));
                    message_id
                }
                Err(err) if is_unknown_message_error(&err) => {
                    let replacement = create_channel_message(channel_snowflake, &body)?;
                    log_info(&format!(
                        "Recreated missing ping report message {message_id} as {replacement}"
                    ));
                    replacement
                }
                Err(err) => return Err(err),
            }
        } else {
            let created = create_channel_message(channel_snowflake, &body)?;
            log_info(&format!("Created ping report message {created}"));
            created
        };
        message_ids.push(message_id);
    }

    if let Some(previous) = previous {
        let old_channel_id = previous.channel_id.parse::<u64>().ok();
        let obsolete_ids = if previous.channel_id == channel_id {
            previous
                .message_ids
                .get(message_ids.len()..)
                .unwrap_or_default()
        } else {
            previous.message_ids.as_slice()
        };

        if let Some(old_channel_id) = old_channel_id {
            for message_id in obsolete_ids {
                if let Err(err) = delete_channel_message(old_channel_id, *message_id) {
                    log_warn(&format!(
                        "Could not remove an obsolete ping report message: {err}"
                    ));
                }
            }
        }
    }

    Ok(PingReportMessages {
        channel_id: channel_id.to_string(),
        message_ids,
    })
}

fn create_channel_message(channel_id: u64, body: &Value) -> Result<u64, String> {
    let response = wpbs::plugin::discord_import_functions::discord_request(
        &wpbs::plugin::discord_import_types::DiscordRequests::CreateMessage((
            channel_id,
            wpbs::plugin::discord_import_types::Body::Json(body.to_string()),
        )),
    )
    .map_err(|err| channel_message_error(&err))?
    .ok_or_else(|| "Discord did not return the created report message".to_string())?;
    let response = serde_json::from_str::<Value>(&response)
        .map_err(|err| format!("Failed to parse the created Discord message: {err}"))?;

    read_snowflake(&response["id"])
        .ok_or_else(|| "Discord did not return a valid report message id".to_string())
}

fn edit_channel_message(channel_id: u64, message_id: u64, body: &Value) -> Result<(), String> {
    wpbs::plugin::discord_import_functions::discord_request(
        &wpbs::plugin::discord_import_types::DiscordRequests::EditMessage((
            channel_id,
            message_id,
            wpbs::plugin::discord_import_types::Body::Json(body.to_string()),
        )),
    )
    .map_err(|err| channel_message_error(&err))?;

    Ok(())
}

fn delete_channel_message(channel_id: u64, message_id: u64) -> Result<(), String> {
    wpbs::plugin::discord_import_functions::discord_request(
        &wpbs::plugin::discord_import_types::DiscordRequests::DeleteMessage((
            channel_id, message_id,
        )),
    )
    .map_err(|err| channel_message_error(&err))?;

    Ok(())
}

fn is_unknown_message_error(err: &str) -> bool {
    err.contains("\"code\": 10008") || err.contains("Unknown Message")
}

fn channel_message_error(err: &str) -> String {
    if err.contains("\"code\": 50001") || err.contains("Missing Access") {
        return "Discord rejected the channel message with Missing Access. Re-invite the app with the bot scope, or grant the bot role View Channel, Send Messages, and Embed Links in the selected channel.".to_string();
    }

    if err.contains("\"code\": 50013") || err.contains("Missing Permissions") {
        return "Discord rejected the channel message with Missing Permissions. Grant the bot role Send Messages and Embed Links in the selected channel.".to_string();
    }

    format!("Failed to send Discord channel message: {err}")
}

pub fn read_snowflake(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
}

pub fn log_info(message: &str) {
    wpbs::plugin::core_import_functions::log(
        wpbs::plugin::core_import_types::LogLevels::Info,
        message,
    );
}

pub fn log_warn(message: &str) {
    wpbs::plugin::core_import_functions::log(
        wpbs::plugin::core_import_types::LogLevels::Warn,
        message,
    );
}

struct RepositoryStatus {
    repository: String,
    count: usize,
    description: String,
}

fn repository_statuses(
    results: &[ExtensionStatus],
    new_sites: &[NewSite],
    generated_at_unix: u64,
) -> Vec<RepositoryStatus> {
    let mut by_repository = BTreeMap::<&str, Vec<String>>::new();

    for result in results {
        let status_suffix = match result.status {
            StatusKind::Working | StatusKind::Cloudflare => String::new(),
            StatusKind::Failed => {
                let duration = result.first_failed_at_unix.map_or_else(
                    || "unknown".to_string(),
                    |started_at| format_duration(generated_at_unix.saturating_sub(started_at)),
                );
                format!(" \u{2022} Down for **{duration}**")
            }
        };
        let indicator = match result.status {
            StatusKind::Working => "\u{1f7e2}",
            StatusKind::Cloudflare => "\u{1f7e0}",
            StatusKind::Failed => "\u{1f534}",
        };
        let extension = result.website_url.as_ref().map_or_else(
            || result.name.clone(),
            |url| format!("[{}]({url})", result.name),
        );
        let new_indicator = if new_sites.iter().any(|site| {
            site.name == result.name && result.website_url.as_ref() == Some(&site.website_url)
        }) {
            "\u{1f195} "
        } else {
            ""
        };

        by_repository
            .entry(&result.repository)
            .or_default()
            .push(format!(
                "{new_indicator}{indicator}  **{extension}**{status_suffix}"
            ));
    }

    let mut repositories = Vec::with_capacity(by_repository.len());
    for (repository, lines) in by_repository {
        repositories.push(RepositoryStatus {
            repository: repository.to_string(),
            count: lines.len(),
            description: lines.join("\n"),
        });
    }

    repositories
}

fn repository_embed(status: &RepositoryStatus) -> Value {
    let name = repository_display_name(&status.repository);
    let heading = match repository_url(&status.repository) {
        Some(url) => format!("[**{name}**]({url}) \u{2022} ({})", status.count),
        None => format!("**{name}** \u{2022} ({})", status.count),
    };
    json!({
        "description": format!("{heading}\n\n{}", status.description),
        "color": BLUE
    })
}

fn validate_report_embeds(embeds: &[Value]) -> Result<(), String> {
    embed_batches(embeds.to_vec()).map(|_| ())
}

fn embed_batches(embeds: Vec<Value>) -> Result<Vec<Vec<Value>>, String> {
    const EMBEDS_PER_MESSAGE: usize = 10;
    const CHARACTERS_PER_MESSAGE: usize = 6000;

    let mut batches = Vec::new();
    let mut batch = Vec::new();
    let mut batch_chars = 0;

    for (index, embed) in embeds.into_iter().enumerate() {
        let embed_chars = validate_embed(&embed, index)?;
        if !batch.is_empty()
            && (batch.len() == EMBEDS_PER_MESSAGE
                || batch_chars + embed_chars > CHARACTERS_PER_MESSAGE)
        {
            batches.push(std::mem::take(&mut batch));
            batch_chars = 0;
        }
        if embed_chars > CHARACTERS_PER_MESSAGE {
            return Err(format!(
                "Report embed {} exceeds Discord's total character limit",
                index + 1
            ));
        }
        batch_chars += embed_chars;
        batch.push(embed);
    }

    if !batch.is_empty() {
        batches.push(batch);
    }

    Ok(batches)
}

fn validate_embed(embed: &Value, index: usize) -> Result<usize, String> {
    let mut total_chars = 0;
    for (key, limit) in [("title", 256), ("description", 4096)] {
        if let Some(value) = embed.get(key).and_then(Value::as_str) {
            let length = value.chars().count();
            if length > limit {
                return Err(format!("Report embed {} has an oversized {key}", index + 1));
            }
            total_chars += length;
        }
    }

    if let Some(footer) = embed
        .get("footer")
        .and_then(|footer| footer.get("text"))
        .and_then(Value::as_str)
    {
        let length = footer.chars().count();
        if length > 2048 {
            return Err(format!(
                "Report embed {} has an oversized footer",
                index + 1
            ));
        }
        total_chars += length;
    }

    if let Some(fields) = embed.get("fields").and_then(Value::as_array) {
        if fields.len() > 25 {
            return Err(format!(
                "Report embed {} exceeds Discord's field count limit",
                index + 1
            ));
        }
        for field in fields {
            for (key, limit) in [("name", 256), ("value", 1024)] {
                let value = field.get(key).and_then(Value::as_str).unwrap_or_default();
                let length = value.chars().count();
                if length > limit {
                    return Err(format!(
                        "Report embed {} has an oversized field {key}",
                        index + 1
                    ));
                }
                total_chars += length;
            }
        }
    }

    Ok(total_chars)
}

fn public_availability_message(result: &ExtensionStatus) -> &'static str {
    match result.status {
        StatusKind::Working => "The source site responded successfully.",
        StatusKind::Cloudflare => "The source site presented a Cloudflare challenge.",
        StatusKind::Failed if result.website_url.is_none() => {
            "No source website was found for this extension."
        }
        StatusKind::Failed if result.http_status.is_some() => {
            "The source site did not accept the availability check."
        }
        StatusKind::Failed => "The source site could not be reached.",
    }
}

fn repository_display_name(repository: &str) -> String {
    match repository {
        "general-extensions" => return "General Extensions".to_string(),
        "tracker-extensions" => return "Trackers".to_string(),
        _ => {}
    }

    repository
        .split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn repository_url(repository: &str) -> Option<String> {
    let valid_slug = !repository.is_empty()
        && repository != "."
        && repository != ".."
        && repository
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));

    if valid_slug {
        Some(format!("https://github.com/inkdex/{repository}"))
    } else {
        None
    }
}

fn format_duration(seconds: u64) -> String {
    let minutes = seconds / 60;
    if minutes == 0 {
        return "<1m".to_string();
    }

    let hours = minutes / 60;
    if hours == 0 {
        return format!("{minutes}m");
    }

    let days = hours / 24;
    if days == 0 {
        let remaining_minutes = minutes % 60;
        if remaining_minutes == 0 {
            return format!("{hours}h");
        }
        return format!("{hours}h {remaining_minutes}m");
    }

    let remaining_hours = hours % 24;
    if remaining_hours == 0 {
        format!("{days}d")
    } else {
        format!("{days}d {remaining_hours}h")
    }
}

fn truncate_field(value: &str, max_len: usize) -> String {
    if value.len() <= max_len {
        return value.to_string();
    }

    let mut truncated = value
        .chars()
        .take(max_len.saturating_sub(3))
        .collect::<String>();
    truncated.push_str("...");
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{http::ProbeKind, reports::ping::CURRENT_CLASSIFIER_VERSION};

    fn extension(name: &str, repository: &str, website_url: &str) -> ExtensionStatus {
        ExtensionStatus {
            name: name.to_string(),
            normalized_name: name.to_ascii_lowercase(),
            repository: repository.to_string(),
            website_url: Some(website_url.to_string()),
            probe_kind: ProbeKind::Get,
            build_time: None,
            status: StatusKind::Working,
            http_status: Some(200),
            elapsed_ms: Some(25),
            first_failed_at_unix: None,
            reason: "OK".to_string(),
        }
    }

    #[test]
    fn new_sources_stay_in_their_repository_category() {
        let generic_url = "https://new.example";
        let repositories = repository_statuses(
            &[
                extension(
                    "GeneralSource",
                    "general-extensions",
                    "https://general.example",
                ),
                extension("NewGenericSource", "madara-extensions", generic_url),
                extension(
                    "FutureGenericSource",
                    "future-extensions",
                    "https://future.example",
                ),
            ],
            &[NewSite {
                name: "NewGenericSource".to_string(),
                website_url: generic_url.to_string(),
            }],
            1,
        );
        let description_for = |repository: &str| {
            let status = repositories
                .iter()
                .find(|status| status.repository == repository)
                .unwrap();
            status.description.clone()
        };
        let general = description_for("general-extensions");
        let madara = description_for("madara-extensions");
        let future = description_for("future-extensions");

        assert!(!general.contains("NewGenericSource"));
        assert!(!general.contains("FutureGenericSource"));
        assert!(madara.contains("NewGenericSource"));
        assert!(madara.contains("\u{1f195} \u{1f7e2}"));
        assert!(future.contains("FutureGenericSource"));
    }

    #[test]
    fn status_rows_rely_on_the_key_except_for_failure_duration() {
        let mut cloudflare = extension(
            "ProtectedSource",
            "general-extensions",
            "https://protected.example",
        );
        cloudflare.status = StatusKind::Cloudflare;

        let mut failed = extension(
            "FailedSource",
            "general-extensions",
            "https://failed.example",
        );
        failed.status = StatusKind::Failed;
        failed.first_failed_at_unix = Some(1);

        let repositories = repository_statuses(&[cloudflare, failed], &[], 61);
        let description = &repositories[0].description;

        assert!(!description.contains("Working"));
        assert!(!description.contains("Cloudflare"));
        assert!(!description.contains("Unavailable"));
        assert!(description.contains("\u{1f7e0}  **[ProtectedSource]"));
        assert!(description.contains("\u{1f534}  **[FailedSource]"));
        assert!(description.contains("Down for **1m**"));
        assert!(!description.contains("None"));
    }

    #[test]
    fn large_repository_stays_in_one_embed_without_placeholders() {
        let results = (0..29)
            .map(|index| {
                extension(
                    &format!("Source{index}"),
                    "madara-extensions",
                    &format!("https://source{index}.example/"),
                )
            })
            .collect::<Vec<_>>();
        let mut repositories = repository_statuses(&results, &[], 1);
        let repository = repositories.pop().unwrap();

        assert_eq!(repository.description.lines().count(), 29);
        assert!(!repository.description.contains("None"));
        assert!(!repository.description.contains("continued"));

        let embed = repository_embed(&repository);
        assert!(embed.get("fields").is_none());
        assert!(embed["description"].as_str().unwrap().chars().count() < 4096);
    }

    #[test]
    fn report_headings_link_only_the_repository_name() {
        let results = vec![
            extension(
                "GeneralSource",
                "general-extensions",
                "https://general.example",
            ),
            extension(
                "MadaraSource",
                "madara-extensions",
                "https://madara.example",
            ),
            extension(
                "TrackerSource",
                "tracker-extensions",
                "https://tracker.example",
            ),
            extension(
                "FutureSource",
                "future-extensions",
                "https://future.example",
            ),
        ];
        let report = PingReport {
            classifier_version: CURRENT_CLASSIFIER_VERSION,
            generated_at_unix: 1,
            registry_url: "https://inkdex.github.io/extensions".to_string(),
            total: results.len(),
            working_count: results.len(),
            cloudflare_count: 0,
            failed_count: 0,
            new_sites: Vec::new(),
            baseline_created: false,
            results,
        };

        let embeds =
            ping_report_embeds(&report, &Settings::default()).expect("report embeds should render");
        let repository_embed = |name: &str| {
            embeds
                .iter()
                .find(|embed| {
                    embed["description"]
                        .as_str()
                        .is_some_and(|value| value.starts_with(&format!("[**{name}**]")))
                })
                .unwrap()
        };

        assert_eq!(embeds.len(), 5);
        for (name, repository) in [
            ("General Extensions", "general-extensions"),
            ("Madara Extensions", "madara-extensions"),
            ("Trackers", "tracker-extensions"),
            ("Future Extensions", "future-extensions"),
        ] {
            let description = repository_embed(name)["description"].as_str().unwrap();
            assert!(description.starts_with(&format!(
                "[**{name}**](https://github.com/inkdex/{repository}) \u{2022} (1)\n\n"
            )));
            assert!(!description.contains("Last changed"));
            assert!(repository_embed(name).get("url").is_none());
        }
    }

    #[test]
    fn summary_shows_reachability_scope_and_next_check() {
        let report = PingReport {
            classifier_version: CURRENT_CLASSIFIER_VERSION,
            generated_at_unix: 1,
            registry_url: "https://inkdex.github.io/extensions".to_string(),
            total: 0,
            working_count: 0,
            cloudflare_count: 0,
            failed_count: 0,
            new_sites: Vec::new(),
            baseline_created: false,
            results: Vec::new(),
        };

        let embeds = ping_report_embeds(&report, &Settings::default()).unwrap();
        let summary = &embeds[0];

        assert_eq!(summary["footer"]["text"], "Only reports site reachability");
        assert!(
            summary["description"]
                .as_str()
                .unwrap()
                .contains("Next ping:")
        );
        assert!(
            summary["description"]
                .as_str()
                .unwrap()
                .contains("Last ping:")
        );
        assert!(!summary["description"].as_str().unwrap().contains(":F>"));
    }

    #[test]
    fn additional_repositories_are_batched_instead_of_dropped() {
        let embeds = (0..12)
            .map(|index| json!({ "title": format!("Repository {index}") }))
            .collect();

        let batches = embed_batches(embeds).expect("embeds should batch");

        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].len(), 10);
        assert_eq!(batches[1].len(), 2);
    }
}
