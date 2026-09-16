use crate::{
    config::Settings,
    discord,
    reports::ReportKind,
    state::{self, RunProgress},
};

pub struct ProgressReporter<'a> {
    settings: &'a Settings,
    title: String,
    last_channel_update: usize,
    interaction: Option<InteractionProgress>,
}

struct InteractionProgress {
    application_id: u64,
    token: String,
}

impl<'a> ProgressReporter<'a> {
    pub fn new(settings: &'a Settings, title: &str) -> Self {
        Self {
            settings,
            title: title.to_string(),
            last_channel_update: 0,
            interaction: None,
        }
    }

    pub fn for_interaction(
        settings: &'a Settings,
        title: &str,
        application_id: u64,
        token: String,
    ) -> Self {
        Self {
            settings,
            title: title.to_string(),
            last_channel_update: 0,
            interaction: Some(InteractionProgress {
                application_id,
                token,
            }),
        }
    }

    pub fn start(&mut self, phase: &str) {
        self.record(phase, 0, 0, true);
    }

    pub fn update(&mut self, phase: &str, completed: usize, total: usize) {
        let should_send = completed == total
            || completed == 1
            || completed.saturating_sub(self.last_channel_update) >= self.settings.progress.every;

        self.record(phase, completed, total, should_send);
    }

    pub fn finish(&mut self, phase: &str) {
        self.record(phase, 1, 1, true);
    }

    fn record(&mut self, phase: &str, completed: usize, total: usize, send_channel: bool) {
        let progress = RunProgress {
            title: self.title.clone(),
            phase: phase.to_string(),
            completed,
            total,
            updated_at_unix: state::now_unix(),
        };

        let _ = state::save_run_progress(&progress);

        if total == 0 {
            discord::log_info(&format!("{}: {phase}", self.title));
        } else {
            discord::log_info(&format!("{}: {phase} ({completed}/{total})", self.title));
        }

        if !send_channel {
            return;
        }

        let mut updated = false;

        if let Some(interaction) = &self.interaction {
            match discord::update_interaction_embed(
                interaction.application_id,
                interaction.token.clone(),
                &discord::progress_embed(&progress),
            ) {
                Ok(()) => updated = true,
                Err(err) => {
                    discord::log_warn(&format!("Could not update report command progress: {err}"));
                }
            }
        }

        if self.settings.progress.discord
            && let Some(channel_id) = self.settings.channel_for(ReportKind::Ping)
            && discord::send_channel_embed(channel_id, &discord::progress_embed(&progress)).is_ok()
        {
            updated = true;
        }

        if updated {
            self.last_channel_update = completed;
        }
    }
}
