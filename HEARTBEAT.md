# HEARTBEAT — Bastion Scheduled Tasks

OpenClaw reads this file every 30 minutes and executes tasks whose interval has been reached.

---

## Tasks

### calendar-check
- **Interval**: every 30 minutes
- **Skill**: `bastion/proactive`
- **Action**: check Google Calendar events in the next 60 minutes
- **Alert condition**: if any event starts in ≤ 5 minutes, send an immediate reminder to the user
- **Reminder format**: `🗓️ In [X] minutes: [event title] — [time]`

### persona-inactivity-check
- **Interval**: every 6 hours
- **Skill**: `bastion/proactive`
- **Action**: check memupalace for personas with no recorded activity for 3 or more days
- **Alert condition**: for each inactive persona, generate a personalised re-engagement suggestion based on the persona's domain
- **Format**: `💤 [Persona Name] has been inactive for [N] days. Want to resume?`

### weekly-review
- **Interval**: every Monday at 9am
- **Skill**: `bastion/weekly-review`
- **Action**: run the `weekly-review` skill for all active personas
- **Includes**: fetch the last 7 days of interactions per persona from memupalace via `memory_search`, calculate usage metrics, compare against current weights, generate a report with weight adjustment suggestions
- **Requires confirmation**: yes — present suggestions to the user before applying any weight change

### memory-analysis
- **Interval**: every 7 days
- **Skill**: `bastion/memupalace` + `bastion/self-improving`
- **Action**: analyse the last 50 memupalace records per persona via `memory_search`
- **Includes**:
  - Extract behaviour patterns and preferences
  - Update `personas/{slug}/MEMORY.md` with new learnings
  - Compare current usage pattern against configured weights
  - If the pattern has changed significantly, suggest weight adjustments to the user
- **Requires confirmation**: yes — weight adjustment suggestions are presented before applying

### cve-check
- **Interval**: every 24 hours
- **Skill**: `bastion/proactive`
- **Action**: check CVEs for installed skills via the ClawHub API
- **Alert condition**: if any CVE is detected in any installed skill, alert the user **immediately** — before any other pending message in the next interaction
- **Alert format**: `⚠️ CVE detected in skill [name]: [description]. Recommend uninstalling or waiting for a patch.`
- **Priority**: maximum — this alert takes precedence over all other pending messages

### validation-metrics-check
- **Interval**: every 6 hours
- **Skill**: `output-validator`
- **Action**: read `config/logs/validation-metrics.json` and calculate recent success rate per skill
- **Alert condition**: if any skill has a recent success rate below 90% (with a minimum of 20 samples in the window), generate an alert
- **Alert format**: `⚠️ Validation drift in [skill]: success rate = [X]% (last [N] executions). Last error: [message]`
- **Additional action**: if schema generation fails for any skill (schema.json missing and SKILL.md has no output example), alert the user
- **Schema alert format**: `⚠️ Skill [name] has no validation schema configured. Add ## Output Example to SKILL.md.`
- **Priority**: normal — display in the next interaction after the CVE alert (if any)

---

## State

The execution state of each task (last run timestamp) is persisted in `personas/{slug}/heartbeat-state.md` per persona, and in the OpenClaw global state file.
