---
gsd_state_version: 1.0
milestone: v1.0
milestone_name: Cognitive Completion + Fabric-Ready Seams
status: ready_to_plan
last_updated: "2026-06-18T12:37:51.958Z"
progress:
  total_phases: 2
  completed_phases: 1
  total_plans: 13
  completed_plans: 12
  percent: 92
---

# State: Bastion v3

## Project Reference

See: `.planning/PROJECT.md` (updated 2026-05-10)

**Core value:** Bastion ajuda Mario a fazer suas metas anuais avançarem — proativo, customizável por linguagem natural, seguro e instalável em minutos.
**Current focus:** Phase 06 — ecosystem-mobile-opcional-p-s-v1-0

## Current Phase

**Phase 5: v1.0 Cognitive Completion + Fabric-Ready Seams** — ✅ PLANEJADO (6 plans em 3 waves; verificação passou). Spec: `.planning/BASTION-V1-COGNITIVE-SPEC.md`. Formalizado no ROADMAP em 2026-06-13 (antes só no spec). Artefatos: `phases/05-*/` (CONTEXT, RESEARCH, PATTERNS, 6 PLAN.md). Escopo aprovado: 6 itens (sem M2). Ordem: BIG-1 primeiro (wave 1, base), depois seams/identidade. Decisões-chave do research: híbrido RAG-leitura/tool-call-escrita; trocar modelo free p/ `meta-llama/llama-3.3-70b-instruct:free`; OTel GenAI semconv 1.0.0; pitfall `needs_approval:false` obrigatório. Próximo passo: `/gsd-execute-phase 5`.

**Plans (Phase 5):**

- Wave 1: 05-01 (BIG-1 — tools no runner/cabinet, tool-loop, modelo) · 05-02 (CONC-1 — busy_timeout + session mutex)
- Wave 2: 05-03 (SEAM-2 — TurnContextProvider + egress por bloco) · 05-04 (M3 — erro no canal + /logs) · 05-05 (SEAM-4 — eventos OTel)
- Wave 3: 05-06 (M1 — identidade por onboarding via SEAM #2)

**Phases 1-4** — ✅ done. Phase 4 cutover-live (v3 sobe FROM scratch 11MB, Telegram ok, multi-persona, privacy gate; soak revelou o gap cognitivo → Phase 5). Registro detalhado: histórico de commits + `.planning/V1-COMPLETION-BACKLOG.md` + `phases/04-*` regenerados.

Follow-ups abertos (não bloqueiam Phase 2):

- MCP-04: OAuth para Composio (`connect.composio.dev` é AuthKit/OAuth — chave estática não serve).
- Gemini thought_signature: tool-use E2E com Gemini-thinking precisa reenviar assinatura proprietária.

Next step: `/gsd-discuss-phase 2` (Cabinet, Privacy Tiers, Contestable Memory, Goal Engine — ver `.planning/specs/cabinet-and-privacy-spec.md`).

## Active Workstream

(none — fresh init)

## Recent Decisions

| Date | Decision | Source |
|------|----------|--------|
| 2026-05-10 | Roteamento de personas via classificação LLM + memória global tageada | Questioning |
| 2026-05-10 | Proatividade em 3 modos (heartbeat + evento + idle), sem intervenção mid-conversation | Questioning |
| 2026-05-10 | skill-writer fica em Phase 3 (depende de memU para padrões) | Questioning |
| 2026-05-10 | Cutover v2 → v3 na Phase 4 (após Docker scratch + installer) | Questioning |
| 2026-05-10 | Personas/skills v2 podem ser reescritas em v3 (compat total não é requisito) | Questioning |
| 2026-05-10 | Source-available com licença restritiva (estilo BSL/Polyform Strict) | Questioning |
| 2026-06-14 | TurnContextProvider: opaque blocks — core never interprets content, format is provider responsibility | 05-03 |
| 2026-06-14 | build_system_prompt uses check_egress per block.max_tier (not persona tier) — prevents LocalOnly leak when persona is CloudOk | 05-03 |
| 2026-06-14 | run_provider_fallback extended with owner+user_input so build_system_prompt covers fallback egress-leak path (T-05-03-03) | 05-03 |
| 2026-06-14 | OTel 0.32 uses SdkTracerProvider (not TracerProvider); no global::shutdown_tracer_provider — use provider.shutdown() directly; with_batch_exporter takes no runtime arg | 05-05 |
| 2026-06-14 | invoke_agent span name is generic — OTel span names immutable after start(); gen_ai.agent.name set via set_attribute after routing | 05-05 |
| 2026-06-17 | D-02 LOCKED: ONE MeshTransport trait serves mesh, mobile, cloud relay as interchangeable implementations | 06-01 |
| 2026-06-17 | D-03: filter_for_mesh calls check_egress sequentially — reuses WR-04, no new privacy primitive invented | 06-01 |
| 2026-06-17 | D-05: no SafeGuard in OSS — privacy mediation is egress gate (WR-04) + OwnerAllowlist tag filter | 06-01 |
| 2026-06-17 | D-07: daemon exposes /events SSE + /auth/exchange + /mesh/pair for Flutter app + mesh peer connectivity | 06-01 |
| 2026-06-17 | D-04 (LOCKED): MESH-03 = write_cabinet_synthesis() neutral mechanism; no auto-trigger from Cabinet; rich inter-owner governance stays closed/Fabric | 06-02 |
| 2026-06-17 | MeshPeer.allowed_tags drives OwnerAllowlist per peer — filter_for_mesh API takes OwnerAllowlist not peer_owner string (plan pseudocode was incorrect) | 06-02 |
| 2026-06-17 | spawn_mesh_sync_job skips first tick at startup to avoid syncing before daemon fully initialized | 06-02 |
| 2026-06-17 | D-08: agentskills.io bidirectional — agentskills-publish (Bastion→hub) + agentskills-install (install-by-conversation) | 06-04 |
| 2026-06-17 | D-09: ClawHub migration = frontmatter rename + skills-ref validate + worked reminder example | 06-04 |
| 2026-06-17 | D-10: Bastion Cloud = closed relay impl of MeshTransport trait; Phase 6 OSS ships only trait boundary + arch doc | 06-04 |
| 2026-06-17 | D-11: only /add-<channel> scaffold skill ships; specific channels (WhatsApp/Discord/Email) are community/future | 06-04 |
| 2026-06-17 | D-06 LOCKED delivered: Flutter cockpit with goals (/goals), DriftIndicator (/drift), ContestableMemoryView (/memories + /contest <id>), mesh static placeholder | 06-03 |
| 2026-06-17 | D-07 delivered: Flutter app connects via /auth/exchange OTC pairing, POST /webhook chat, GET /events SSE with x-bastion-token | 06-03 |
| 2026-06-18 | CR-01 fixed: resolve_owner_or_401 decodes HS256 JWT; sub claim → owner_id; falls back to static owner_map | 06-05 |
| 2026-06-18 | WR-01 fixed: APP_JWT_SECRET unset → fail-closed anyhow::bail!, never start with hardcoded default | 06-05 |
| 2026-06-18 | CR-02 fixed: OtcStore pub type alias + new_otc_store() factory; serve_with_mesh accepts OtcStore; main.rs wired | 06-05 |
| 2026-06-18 | CR-05 fixed: Flutter sendMessage uses {'text': message} / resp.data['reply'] aligned to Rust In/Out structs | 06-05 |
| 2026-06-18 | WR-03 fixed: unified 401 body for expired vs unknown OTC prevents enumeration oracle | 06-05 |
| 2026-06-18 | CR-03 closed: privacy_tier TEXT column in beliefs via idempotent ALTER TABLE; NULL = deny-on-ambiguity; retrieve_tagged/load_core return real tiers | 06-06 |
| 2026-06-18 | CR-04 closed: write_cabinet_synthesis passes Some(CloudOk) explicitly — Cabinet synthesis now crosses filter_for_mesh; no implicit tier promotion | 06-06 |

## Files

| Artifact | Path |
|----------|------|
| Strategy | `STRATEGY.md` (raiz) |
| Project | `.planning/PROJECT.md` |
| Config | `.planning/config.json` |
| Requirements | `.planning/REQUIREMENTS.md` |
| Roadmap | `.planning/ROADMAP.md` |
| State | `.planning/STATE.md` |
| Codebase map | `.planning/codebase/` |

---
*Last updated: 2026-06-18 — 06-06 privacy_tier schema + mesh egress unblocked: CR-03 (column + read path), CR-04 (write_cabinet_synthesis CloudOk). 150 lib tests pass.*
