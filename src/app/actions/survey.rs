impl HestiaApp {
    fn submit_feedback_survey(&mut self, payload_json: String) {
        let Some(survey) = feedback_survey() else {
            return;
        };
        if FEEDBACK_SURVEY_SERVER_URL.trim().is_empty() {
            self.report_error_message(
                "feedback survey server URL is not configured".to_string(),
                Some("Could not submit feedback"),
            );
            return;
        }

        let pending_path = Self::feedback_survey_pending_path(survey.version);
        if let Err(err) = persistence::write_atomic_bytes(&pending_path, payload_json.as_bytes()) {
            self.report_error_message(
                format!("failed to save pending survey payload: {err:#}"),
                Some("Could not submit feedback"),
            );
            return;
        }

        self.state.mark_feedback_survey_submit_pending(survey);
        self.feedback_survey_submitting = true;
        self.feedback_survey_answers.clear();
        self.feedback_survey_message.clear();
        self.feedback_survey_privacy_expanded = false;
        self.save_state();

        let _ = self.feedback_survey_submit_tx.send(FeedbackSurveySubmitRequest {
            version: survey.version.to_string(),
            payload_json,
            pending_path,
            discard_on_failure: false,
        });
    }

    fn retry_pending_feedback_survey_on_launch(&mut self) {
        let Some(survey) = feedback_survey() else {
            return;
        };

        let pending_path = Self::feedback_survey_pending_path(survey.version);
        if !pending_path.is_file() {
            return;
        }

        if self
            .state
            .feedback_survey
            .surveys
            .get(&survey.key())
            .is_some_and(|state| state.submitted || state.skipped || state.submit_discarded)
        {
            let _ = fs::remove_file(&pending_path);
            return;
        }

        let payload_json = match fs::read_to_string(&pending_path) {
            Ok(payload_json) => payload_json,
            Err(err) => {
                self.log_action(
                    "Survey",
                    &format!("discarded unreadable pending feedback payload: {err:#}"),
                );
                let _ = fs::remove_file(&pending_path);
                self.state.discard_pending_feedback_survey(survey);
                self.save_state();
                return;
            }
        };

        self.state.mark_feedback_survey_submit_pending(survey);
        self.save_state();
        self.feedback_survey_submitting = true;
        let _ = self.feedback_survey_submit_tx.send(FeedbackSurveySubmitRequest {
            version: survey.version.to_string(),
            payload_json,
            pending_path,
            discard_on_failure: true,
        });
        self.log_action("Survey", "retrying pending feedback payload");
    }

    fn consume_feedback_survey_events(&mut self) {
        while let Ok(event) = self.feedback_survey_submit_rx.try_recv() {
            self.feedback_survey_submitting = false;
            match event {
                FeedbackSurveySubmitEvent::Submitted {
                    version,
                    pending_path,
                } => {
                    let _ = fs::remove_file(&pending_path);
                    if let Some(survey) = feedback_survey().filter(|survey| survey.version == version) {
                        self.state.mark_feedback_survey_submitted(survey);
                    }
                    self.save_state();
                    self.log_action("Survey", &format!("submitted feedback for {version}"));
                    self.set_message_ok("Feedback submitted");
                }
                FeedbackSurveySubmitEvent::Failed {
                    version,
                    pending_path,
                    error,
                    discard_on_failure,
                } => {
                    self.log_action(
                        "Survey",
                        &format!("feedback submit failed for {version}: {error}"),
                    );
                    if discard_on_failure {
                        let _ = fs::remove_file(&pending_path);
                        if let Some(survey) =
                            feedback_survey().filter(|survey| survey.version == version)
                        {
                            self.state.discard_pending_feedback_survey(survey);
                        }
                        self.save_state();
                        self.log_action(
                            "Survey",
                            &format!("discarded pending feedback payload for {version}"),
                        );
                    }
                }
            }
        }
    }

    fn feedback_survey_pending_path(version: &str) -> PathBuf {
        Self::runtime_temp_root().join(format!(
            "survey-{}.json",
            Self::sanitize_feedback_survey_file_component(version)
        ))
    }

    fn sanitize_feedback_survey_file_component(value: &str) -> String {
        let mut sanitized = String::with_capacity(value.len().max(1));
        for ch in value.chars() {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                sanitized.push(ch);
            } else {
                sanitized.push('_');
            }
        }
        if sanitized.is_empty() {
            "unknown".to_string()
        } else {
            sanitized
        }
    }
}
