#![allow(clippy::same_length_and_capacity)] // Emitted by wit-bindgen 0.60.

mod config;
mod discord;
mod http;
mod progress;
mod registry;
mod reports;
mod state;

use serde_json::Value;
use wit_bindgen::generate;
use wstd::runtime::block_on;

use crate::{
    config::Settings,
    progress::ProgressReporter,
    registry::{ensure_catalog, refresh_catalog},
    reports::{
        ReportInterval, ReportKind, ScheduleKind,
        ping::{CURRENT_CLASSIFIER_VERSION, check_saved_extension_live, load_latest_ping_report},
    },
    state::save_settings,
};

generate!({
    path: "../wit",
    world: "plugin",
});

struct ExtensionStatusPlugin;

const STATUS_COMMAND: &str = "status";
const SET_REPORT_CHANNEL_COMMAND: &str = "set-report-channel";
const SET_REPORT_INTERVAL_COMMAND: &str = "set-report-interval";
const POST_REPORT_COMMAND: &str = "post-report";
const POST_STALE_REPORT_COMMAND: &str = "post-stale-report";
const MANAGE_GUILD_PERMISSION: u64 = 1 << 5;

impl exports::wpbs::plugin::core_export_functions::Guest for ExtensionStatusPlugin {
    fn initialization(settings: String) -> Result<(), String> {
        let mut settings = Settings::from_json(&settings);
        state::apply_channel_overrides(&mut settings)?;
        state::apply_interval_overrides(&mut settings)?;
        save_settings(&settings)?;

        let status_command = discord::status_command_json(STATUS_COMMAND);
        let set_report_channel_command =
            discord::set_report_channel_command_json(SET_REPORT_CHANNEL_COMMAND);
        let set_report_interval_command =
            discord::set_report_interval_command_json(SET_REPORT_INTERVAL_COMMAND);
        let post_report_command = discord::post_report_command_json(POST_REPORT_COMMAND);
        let post_stale_report_command =
            discord::post_stale_report_command_json(POST_STALE_REPORT_COMMAND);
        let scheduled_jobs = settings.scheduled_jobs();

        let registrations = wpbs::plugin::core_import_types::Registrations {
            core: None,
            services: Some(wpbs::plugin::core_import_types::ServicesRegistrations {
                job_scheduler: Some(
                    wpbs::plugin::job_scheduler_import_types::JobSchedulerRegistrations {
                        scheduled_jobs: Some(scheduled_jobs),
                    },
                ),
                discord: Some(wpbs::plugin::discord_import_types::DiscordRegistrations {
                    events: Some(vec![
                        wpbs::plugin::discord_import_types::DiscordEventKinds::InteractionCreate,
                    ]),
                    interactions: Some(
                        wpbs::plugin::discord_import_types::DiscordRegistrationsInteractions {
                            application_commands: Some(vec![
                                status_command.to_string(),
                                set_report_channel_command.to_string(),
                                set_report_interval_command.to_string(),
                                post_report_command.to_string(),
                                post_stale_report_command.to_string(),
                            ]),
                            message_components: None,
                            modals: None,
                        },
                    ),
                }),
            }),
        };

        let result = wpbs::plugin::core_import_functions::register(&registrations);
        if let Err(err) = state::save_job_registrations(&settings, &result) {
            discord::log_warn(&format!("Scheduled report registration failed: {err}"));
        }
        discord::log_info(&format!(
            "Registered extension status command and extension monitor jobs: {result:?}"
        ));

        match state::load_latest_ping_report()? {
            Some(report) if report.classifier_is_current() => {
                if let Some(channel_id) = settings.channel_for(ReportKind::Ping) {
                    match upsert_ping_report(channel_id, &report, &settings) {
                        Ok(()) => discord::log_info(
                            "Rewrote the saved ping report message on plugin load",
                        ),
                        Err(err) => discord::log_warn(&format!(
                            "Could not rewrite the saved ping report message on plugin load: {err}"
                        )),
                    }
                }
            }
            latest_report => {
                let reason = match latest_report {
                    Some(report) => format!(
                        "Saved ping report classifier version {} is older than {}; regenerating on plugin load",
                        report.classifier_version, CURRENT_CLASSIFIER_VERSION
                    ),
                    None => "No saved ping report found; generating initial report on plugin load"
                        .to_string(),
                };
                discord::log_info(&reason);
                if let Err(err) = block_on(generate_initial_report(&settings)) {
                    discord::log_warn(&format!("Initial ping report generation failed: {err}"));
                }
            }
        }

        Ok(())
    }

    fn dependency_function(_function_id: String, _params: Vec<u8>) -> Result<Vec<u8>, String> {
        Err("extension-status does not expose dependency functions".to_string())
    }

    fn shutdown() -> Result<(), String> {
        Ok(())
    }
}

impl exports::wpbs::plugin::job_scheduler_export_functions::Guest for ExtensionStatusPlugin {
    fn scheduled_job(job_id: String) -> Result<(), String> {
        let settings = state::load_settings()?.unwrap_or_else(Settings::default);
        let Some(job_kind) = state::load_job_kind(&job_id)? else {
            discord::log_info(&format!("Ignoring stale scheduled job {job_id}"));
            return Ok(());
        };

        match job_kind {
            state::JobKind::Discovery => {
                block_on(refresh_catalog_job(&settings))?;
            }
            state::JobKind::Ping => {
                block_on(generate_ping_report_job(&settings, false, None))?;
            }
            state::JobKind::DiscoveryAndPing => {
                block_on(refresh_catalog_job(&settings))?;
                block_on(generate_ping_report_job(&settings, false, None))?;
            }
        }

        Ok(())
    }
}

impl exports::wpbs::plugin::discord_export_functions::Guest for ExtensionStatusPlugin {
    fn discord_application_commands(
        registrations_result: wpbs::plugin::discord_export_types::DiscordRegistrationsResultApplicationCommands,
    ) {
        discord::log_info(&format!(
            "Discord command registration result: {registrations_result:?}"
        ));
    }

    fn discord_event(
        event: wpbs::plugin::discord_export_types::DiscordEvents,
    ) -> Result<(), String> {
        let wpbs::plugin::discord_export_types::DiscordEvents::InteractionCreate(interaction) =
            event
        else {
            return Ok(());
        };

        let interaction: Value = serde_json::from_str(&interaction)
            .map_err(|err| format!("Failed to parse interaction JSON: {err}"))?;

        let command_name = interaction
            .get("data")
            .and_then(|data| data.get("name"))
            .and_then(Value::as_str)
            .unwrap_or_default();

        if command_name == POST_REPORT_COMMAND {
            let (interaction_id, interaction_token) = interaction_context(&interaction)?;
            let application_id = discord::read_snowflake(&interaction["application_id"])
                .ok_or_else(|| {
                    "Interaction payload did not include a valid application id".to_string()
                })?;
            discord::defer_interaction(interaction_id, interaction_token.clone())?;

            let response = match block_on(post_report_response(
                &interaction,
                application_id,
                interaction_token.clone(),
            )) {
                Ok(response) => response,
                Err(err) => discord::error_response_embed("Could not post report", &err),
            };

            return discord::update_interaction_embed(application_id, interaction_token, &response);
        }

        let response = match command_name {
            STATUS_COMMAND => match block_on(status_response(
                &command_option_string(&interaction, "extension").unwrap_or_default(),
            )) {
                Ok(response) => response,
                Err(err) => discord::error_response_embed("Extension status unavailable", &err),
            },
            SET_REPORT_CHANNEL_COMMAND => match set_report_channel_response(&interaction) {
                Ok(response) => response,
                Err(err) => discord::error_response_embed("Could not update report channel", &err),
            },
            SET_REPORT_INTERVAL_COMMAND => match set_report_interval_response(&interaction) {
                Ok(response) => response,
                Err(err) => discord::error_response_embed("Could not update report interval", &err),
            },
            POST_STALE_REPORT_COMMAND => match post_stale_report_response(&interaction) {
                Ok(response) => response,
                Err(err) => discord::error_response_embed("Could not post saved report", &err),
            },
            _ => return Ok(()),
        };

        let (interaction_id, interaction_token) = interaction_context(&interaction)?;
        discord::send_interaction_embed(interaction_id, interaction_token, response)
    }
}

async fn status_response(extension_name: &str) -> Result<Value, String> {
    if extension_name.trim().is_empty() {
        return Err(
            "Pass an extension name, for example `/status extension: MangaFire`.".to_string(),
        );
    }

    let Some(report) = load_latest_ping_report()? else {
        return Err("No ping report has been generated yet. Startup generation may still be running or failed; check the bot logs.".to_string());
    };

    let Some(result) = report.find_extension(extension_name) else {
        return Ok(discord::unknown_extension_embed(extension_name, &report));
    };

    let mut settings = state::load_settings()?.unwrap_or_else(Settings::default);
    settings.request_timeout_ms = settings.request_timeout_ms.clamp(1_000, 2_500);

    let checked_at_unix = state::now_unix();
    let live_result =
        check_saved_extension_live(result, &settings, checked_at_unix, report.generated_at_unix)
            .await;
    Ok(discord::extension_status_embed(
        &live_result,
        &report,
        checked_at_unix,
    ))
}

fn set_report_channel_response(interaction: &Value) -> Result<Value, String> {
    if !member_can_manage_guild(interaction) {
        return Err("You need the Manage Server permission to set report channels.".to_string());
    }

    let report = command_option_string(interaction, "report")
        .ok_or_else(|| "Choose either `ping` or `tests`.".to_string())?;
    let kind = ReportKind::from_slug(&report)
        .ok_or_else(|| "Choose either `ping` or `tests`.".to_string())?;
    let channel_id = command_option_snowflake(interaction, "channel")
        .ok_or_else(|| "Choose a Discord text channel.".to_string())?;

    let mut settings = state::load_settings()?.unwrap_or_else(Settings::default);
    settings.set_channel(kind, channel_id.clone());
    state::save_channel_override(kind, channel_id.clone())?;
    state::save_settings(&settings)?;

    let note =
        publish_report_to_channel(kind, &settings, Some(&channel_id)).unwrap_or_else(|err| {
            format!("Channel was saved, but posting the current report failed: {err}")
        });

    Ok(discord::report_channel_saved_embed(
        kind,
        &channel_id,
        &note,
    ))
}

fn set_report_interval_response(interaction: &Value) -> Result<Value, String> {
    if !member_can_manage_guild(interaction) {
        return Err("You need the Manage Server permission to set report intervals.".to_string());
    }

    let schedule = command_option_string(interaction, "schedule")
        .ok_or_else(|| "Choose a schedule to update.".to_string())?;
    let kind = ScheduleKind::from_slug(&schedule)
        .ok_or_else(|| "Choose a valid schedule to update.".to_string())?;
    let interval = command_option_string(interaction, "interval")
        .and_then(|value| ReportInterval::from_slug(&value))
        .ok_or_else(|| "Choose one of the available report intervals.".to_string())?;

    state::save_interval_override(kind, interval)?;

    let note = match kind {
        ScheduleKind::Discovery => {
            let mut settings = state::load_settings()?.unwrap_or_else(Settings::default);
            settings.discovery_cron = interval.cron().to_string();
            state::save_settings(&settings)?;
            replace_scheduled_jobs(&settings)?;
            "The new extension-list refresh schedule is active now."
        }
        ScheduleKind::Ping => {
            let mut settings = state::load_settings()?.unwrap_or_else(Settings::default);
            settings.ping_cron = interval.cron().to_string();
            state::save_settings(&settings)?;
            replace_scheduled_jobs(&settings)?;
            "The new ping schedule is active now."
        }
        ScheduleKind::Tests => {
            "The interval is saved for the future test runner; no tests job runs yet."
        }
    };

    Ok(discord::report_interval_saved_embed(kind, interval, note))
}

async fn post_report_response(
    interaction: &Value,
    application_id: u64,
    interaction_token: String,
) -> Result<Value, String> {
    if !member_can_manage_guild(interaction) {
        return Err("You need the Manage Server permission to post reports.".to_string());
    }

    let report = command_option_string(interaction, "report")
        .ok_or_else(|| "Choose either `ping` or `tests`.".to_string())?;
    let kind = ReportKind::from_slug(&report)
        .ok_or_else(|| "Choose either `ping` or `tests`.".to_string())?;

    let settings = state::load_settings()?.unwrap_or_else(Settings::default);
    let channel_id = settings
        .channel_for(kind)
        .ok_or_else(|| format!("No {} channel is configured yet.", kind.label()))?
        .to_string();

    match kind {
        ReportKind::Ping => {
            generate_ping_report_job(&settings, false, Some((application_id, interaction_token)))
                .await?;
            Ok(discord::report_posted_embed(kind, &channel_id))
        }
        ReportKind::Tests => {
            let note = publish_report_to_channel(kind, &settings, Some(&channel_id))?;
            Ok(discord::report_channel_saved_embed(
                kind,
                &channel_id,
                &note,
            ))
        }
    }
}

fn post_stale_report_response(interaction: &Value) -> Result<Value, String> {
    if !member_can_manage_guild(interaction) {
        return Err("You need the Manage Server permission to post reports.".to_string());
    }

    let settings = state::load_settings()?.unwrap_or_else(Settings::default);
    let channel_id = settings
        .channel_for(ReportKind::Ping)
        .ok_or_else(|| "No ping report channel is configured yet.".to_string())?;
    let report =
        load_latest_ping_report()?.ok_or_else(|| "No saved ping report exists yet.".to_string())?;

    upsert_ping_report(channel_id, &report, &settings)?;
    Ok(discord::stale_report_posted_embed(channel_id))
}

fn publish_report_to_channel(
    kind: ReportKind,
    settings: &Settings,
    channel_id: Option<&str>,
) -> Result<String, String> {
    let Some(channel_id) = channel_id.or_else(|| settings.channel_for(kind)) else {
        return Ok(format!(
            "No {} channel is configured yet.",
            kind.label().to_ascii_lowercase()
        ));
    };

    match kind {
        ReportKind::Ping => {
            let Some(report) = load_latest_ping_report()? else {
                return Ok(
                    "No saved ping report exists yet. It will post after the first report run."
                        .to_string(),
                );
            };

            if !report.classifier_is_current() {
                return Ok("The saved ping report is stale. Restart the bot or wait for the next ping run before posting it.".to_string());
            }

            upsert_ping_report(channel_id, &report, settings)?;
            Ok(format!("Posted the latest ping report to <#{channel_id}>."))
        }
        ReportKind::Tests => Ok(
            "Tests report publishing is reserved for the future test-runner integration."
                .to_string(),
        ),
    }
}

fn replace_scheduled_jobs(settings: &Settings) -> Result<(), String> {
    let registrations = wpbs::plugin::core_import_types::Registrations {
        core: None,
        services: Some(wpbs::plugin::core_import_types::ServicesRegistrations {
            job_scheduler: Some(
                wpbs::plugin::job_scheduler_import_types::JobSchedulerRegistrations {
                    scheduled_jobs: Some(settings.scheduled_jobs()),
                },
            ),
            discord: None,
        }),
    };
    let result = wpbs::plugin::core_import_functions::register(&registrations);
    let registered = state::save_job_registrations(settings, &result)?;

    if registered == 0 {
        return Err(
            "The job scheduler did not register the new interval. Check that the service is enabled and the plugin has ScheduledJobs permission."
                .to_string(),
        );
    }

    Ok(())
}

fn upsert_ping_report(
    channel_id: &str,
    report: &reports::ping::PingReport,
    settings: &Settings,
) -> Result<(), String> {
    let previous = state::load_ping_report_messages()?;
    let messages = discord::upsert_channel_embeds(
        channel_id,
        discord::ping_report_embeds(report, settings)?,
        previous.as_ref(),
    )?;
    state::save_ping_report_messages(&messages)
}

fn member_can_manage_guild(interaction: &Value) -> bool {
    interaction
        .get("member")
        .and_then(|member| member.get("permissions"))
        .and_then(read_permissions)
        .is_some_and(|permissions| permissions & MANAGE_GUILD_PERMISSION != 0)
}

fn read_permissions(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
}

fn interaction_context(interaction: &Value) -> Result<(u64, String), String> {
    let interaction_id = discord::read_snowflake(&interaction["id"])
        .ok_or_else(|| "Interaction payload did not include a valid id".to_string())?;
    let interaction_token = interaction
        .get("token")
        .and_then(Value::as_str)
        .ok_or_else(|| "Interaction payload did not include a token".to_string())?
        .to_string();

    Ok((interaction_id, interaction_token))
}

fn command_option_string(interaction: &Value, option_name: &str) -> Option<String> {
    command_option(interaction, option_name)?
        .get("value")
        .and_then(|value| value.as_str().map(str::to_string))
}

fn command_option_snowflake(interaction: &Value, option_name: &str) -> Option<String> {
    let value = command_option(interaction, option_name)?.get("value")?;
    discord::read_snowflake(value)
        .map(|value| value.to_string())
        .or_else(|| value.as_str().map(str::to_string))
}

fn command_option<'a>(interaction: &'a Value, option_name: &str) -> Option<&'a Value> {
    interaction
        .get("data")
        .and_then(|data| data.get("options"))
        .and_then(Value::as_array)?
        .iter()
        .find(|option| option.get("name").and_then(Value::as_str) == Some(option_name))
}

async fn generate_initial_report(settings: &Settings) -> Result<(), String> {
    generate_ping_report_job(settings, true, None).await
}

async fn refresh_catalog_job(settings: &Settings) -> Result<(), String> {
    let mut progress = ProgressReporter::new(settings, "Inkdex extension discovery");
    progress.start("Refreshing extension list from Inkdex");
    let catalog = refresh_catalog(settings, &mut progress).await?;
    state::save_latest_catalog(&catalog)?;
    progress.finish(&format!(
        "Extension discovery complete: captured {} extensions",
        catalog.targets.len()
    ));
    Ok(())
}

async fn generate_ping_report_job(
    settings: &Settings,
    bootstrap: bool,
    interaction: Option<(u64, String)>,
) -> Result<(), String> {
    let manual = interaction.is_some();
    let mut progress = match interaction {
        Some((application_id, token)) => ProgressReporter::for_interaction(
            settings,
            "Inkdex extension ping",
            application_id,
            token,
        ),
        None => ProgressReporter::new(settings, "Inkdex extension ping"),
    };
    if bootstrap {
        progress.start("No saved ping report found; generating initial report");
    } else if manual {
        progress.start("Generating requested ping report");
    } else {
        progress.start("Generating scheduled ping report");
    }

    let catalog = ensure_catalog(settings, &mut progress).await?;
    let report = reports::ping::run_ping_report(settings, &catalog, &mut progress).await?;
    state::save_latest_ping_report(&report)?;
    progress.finish(&format!(
        "Ping report completed: working={}, cloudflare={}, failed={}, new={}",
        report.working_count,
        report.cloudflare_count,
        report.failed_count,
        report.new_sites.len()
    ));

    if let Some(channel_id) = settings.channel_for(ReportKind::Ping) {
        upsert_ping_report(channel_id, &report, settings)?;
    } else {
        discord::log_warn("Ping report completed, but no ping report channel is configured");
    }

    Ok(())
}

export!(ExtensionStatusPlugin);
