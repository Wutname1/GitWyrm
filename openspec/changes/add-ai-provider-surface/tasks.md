# Tasks

## 1. Provider state

- [ ] 1.1 Read the configured provider/model from BYO-AI settings
- [ ] 1.2 Verified-connection check (a stored-but-stale token is NOT "configured");
      needs-reconnect state distinct from not-set-up

## 2. Desk surfaces

- [ ] 2.1 Titlebar chip: "✦ <provider> · <model>" / amber "reconnect" / muted
      "AI · not set up"; click opens Settings → AI
- [ ] 2.2 Rail (configured): "▶ Run this task with AI" primary, "Ask about this change"
      secondary, identity line "Runs with <provider> · <model> · uses your <provider>
      plan — change in Settings"
- [ ] 2.3 "Prefer your own editor?" collapsed section holding Copy/opencode/VS Code and
      the preview (retitled "What the AI reads"); open state preserved across re-renders
- [ ] 2.4 Rail (not configured): identical to the pre-AI baseline, plus one dismissible
      dashed "Run tasks right here — Connect an AI" card at the rail bottom
- [ ] 2.5 Hide (don't disable) every AI entry point when not configured

## 3. Main window + status bars

- [ ] 3.1 Spec card primary: "✦ Run next task with AI" when configured (opens the Desk
      and starts); copy handoff demoted to secondary
- [ ] 3.2 Desk status bar AI segment: ready / working / needs-answer; omitted when
      not configured

## 4. Verify

- [ ] 4.1 Flip provider on/off and stale: every surface follows, nothing dead-ends
- [ ] 4.2 No jargon audit: no "prompt"/"tokens"/"context"/model IDs in primary copy
