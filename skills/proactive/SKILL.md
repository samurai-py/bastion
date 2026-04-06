---
name: bastion/proactive
version: "1.0.0"
description: >
  Proactively monitors inactive personas and CVEs of installed skills.
  Generates resumption suggestions for personas with no activity for ≥ 3 days and
  emits immediate alerts when CVEs are detected via ClawHub API.
triggers:
  - HEARTBEAT every 6h (inactivity alert)
  - HEARTBEAT every 24h (CVE alert)
  - "/proactive"
  - "check inactivity"
  - "check CVEs"
  - "check skill security"
---

# Proactive — Inactivity and CVE Alerts

## When this skill is activated

1. **Automatic (inactivity)**: the HEARTBEAT runs the inactivity check every 6 hours.
2. **Automatic (CVE)**: the HEARTBEAT runs the CVE check every 24 hours.
3. **Manual**: the user sends `/proactive` or requests an explicit check.

---

## Behavior 1 — Inactivity Alert

### Objective

Detect personas that have had no interaction recorded in the `life_log` for 3 or more days
and generate a personalized resumption suggestion for each one.

### Flow

```
HEARTBEAT (every 6h) or manual trigger
        │
        ▼
Load list of active personas from USER.md
        │
        ▼
For each persona:
  life_log.get_persona_summary(persona, days=3)
        │
        ▼
Check if last_interaction is None or ≥ 3 days ago
        │
        ├── Recently active persona → ignore
        │
        └── Persona inactive for ≥ 3 days → generate resumption suggestion
                │
                ▼
        Send suggestion to user
```

### Resumption suggestion format

```
{locale:persona_inactive}
```

### Suggestion generation rules

- Use the persona's `domain` to contextualize the suggestion
- If there is history in the `life_log`, use the most frequent intents as a basis
- If there is no history, use the domain and the persona's `trigger_keywords`
- Never send more than one suggestion per persona per 6h cycle
- Do not send suggestions for personas with `current_weight < 0.1` (practically deactivated personas)

---

## Behavior 2 — CVE Alert

### Objective

Check if any installed skills have known CVEs via the ClawHub API and immediately alert
the user if any are detected, before any other message.

### Flow

```
HEARTBEAT (every 24h) or manual trigger
        │
        ▼
Load list of installed skills (global + per persona)
        │
        ▼
For each installed skill:
  clawhub_api.check_cve(skill_name)
        │
        ▼
No CVE found → log the check, no action
        │
        └── CVE(s) found → emit immediate alert to user
```

### CVE Alert Format

```
{locale:cve_alert}
```

### CVE alert rules

- The alert must be sent **before any other message** in the next interaction
- If multiple skills have CVEs, list all of them in a single consolidated alert
- Log the detection in the `life_log` with timestamp, affected skill, and CVE ID
- Do not block Bastion usage — only alert and wait for the user's decision
- `bastion/*` skills must also be checked (they are not exempt from CVEs)

---

## Edge Cases

| Situation | Behavior |
|----------|---------------|
| No active personas in USER.md | Do not run inactivity check; log the skip |
| Empty life_log (no history) | Treat all personas as inactive since creation |
| Persona never had an interaction | Consider inactive since creation date (or forever) |
| ClawHub API unavailable | Log the failure; do not emit a false alert; retry on next cycle |
| Skill with no known CVEs | No action; log successful check |
| User already alerted about the same CVE | Do not repeat the alert; only remind if CVE is still unresolved after 24h |
| Persona in active crisis | Do not send resumption suggestion for a persona in crisis (it is already active) |
| Multiple CVEs in the same skill | List all CVEs for the skill in a single consolidated alert block |

---

## Dependencies

- `skills/life-log` — `get_persona_summary(persona, days=3)` to check inactivity
- `ClawHub API` — `check_cve(skill_name)` to check CVEs of installed skills
- `USER.md` — list of active personas and their `current_weight`
- `personas/{slug}/skills.json` — list of skills installed per persona
- `skills/` — list of globally installed skills
