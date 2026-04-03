---
name: bastion/proactive
version: "1.0.0"
description: >
  Monitora proativamente personas inativas e CVEs de skills instaladas.
  Gera sugestões de retomada para personas sem atividade há ≥ 3 dias e
  emite alertas imediatos quando CVEs são detectados via ClawHub API.
triggers:
  - HEARTBEAT a cada 6h (alerta de inatividade)
  - HEARTBEAT a cada 24h (alerta de CVE)
  - "/proactive"
  - "verificar inatividade"
  - "verificar CVEs"
  - "checar segurança das skills"
---

# Proactive — Alertas de Inatividade e CVE

## Quando este skill é ativado

1. **Automático (inatividade)**: o HEARTBEAT executa a verificação de inatividade a cada 6 horas.
2. **Automático (CVE)**: o HEARTBEAT executa a verificação de CVEs a cada 24 horas.
3. **Manual**: o usuário envia `/proactive` ou solicita verificação explícita.

---

## Comportamento 1 — Alerta de Inatividade

### Objetivo

Detectar personas que não tiveram nenhuma interação registrada no `life_log` há 3 ou mais dias
e gerar uma sugestão de retomada personalizada para cada uma.

### Fluxo

```
HEARTBEAT (a cada 6h) ou trigger manual
        │
        ▼
Carregar lista de personas ativas de USER.md
        │
        ▼
Para cada persona:
  life_log.get_persona_summary(persona, days=3)
        │
        ▼
Verificar se last_interaction é None ou há ≥ 3 dias
        │
        ├── Persona ativa recentemente → ignorar
        │
        └── Persona inativa há ≥ 3 dias → gerar sugestão de retomada
                │
                ▼
        Enviar sugestão ao usuário
```

### Formato da sugestão de retomada

```
😴 Sua persona **{Nome da Persona}** está inativa há {N} dias.

Última atividade: {data_ultima_interacao} (ou "nunca" se não houver registro)

💡 Quer retomar? Aqui estão algumas ideias para começar:
   • {sugestão_baseada_no_domínio_da_persona}
   • {sugestão_baseada_no_histórico_de_intents}
   • Ou simplesmente me diga o que está acontecendo com {domínio_da_persona}.
```

### Regras de geração da sugestão

- Usar o `domain` da persona para contextualizar a sugestão
- Se houver histórico no `life_log`, usar os intents mais frequentes como base
- Se não houver histórico, usar o domínio e as `trigger_keywords` da persona
- Nunca enviar mais de uma sugestão por persona por ciclo de 6h
- Não enviar sugestão para personas com `current_weight < 0.1` (personas praticamente desativadas)

---

## Comportamento 2 — Alerta de CVE

### Objetivo

Verificar se alguma das skills instaladas possui CVEs conhecidos via ClawHub API e alertar
o usuário imediatamente caso algum seja detectado, antes de qualquer outra mensagem.

### Fluxo

```
HEARTBEAT (a cada 24h) ou trigger manual
        │
        ▼
Carregar lista de skills instaladas (globais + por persona)
        │
        ▼
Para cada skill instalada:
  clawhub_api.check_cve(skill_name)
        │
        ▼
Nenhum CVE encontrado → registrar verificação no log, nenhuma ação
        │
        └── CVE(s) encontrado(s) → emitir alerta imediato ao usuário
```

### Formato do alerta de CVE

```
🚨 **ALERTA DE SEGURANÇA** — CVE detectado em skill instalada

Skill afetada: `{skill_name}`
CVE: {cve_id}
Severidade: {severity} ({CRITICAL / HIGH / MEDIUM / LOW})
Descrição: {description}

⚠️ Recomendação: desinstale ou atualize esta skill imediatamente.
Quer que eu desinstale `{skill_name}` agora? (sim/não)
```

### Regras do alerta de CVE

- O alerta deve ser enviado **antes de qualquer outra mensagem** na próxima interação
- Se múltiplas skills tiverem CVEs, listar todas em um único alerta consolidado
- Registrar a detecção no `life_log` com timestamp, skill afetada e CVE ID
- Não bloquear o uso do Bastion — apenas alertar e aguardar decisão do usuário
- Skills `bastion/*` também devem ser verificadas (não são isentas de CVE)

---

## Edge Cases

| Situação | Comportamento |
|----------|---------------|
| Nenhuma persona ativa em USER.md | Não executar verificação de inatividade; registrar no log |
| life_log vazio (sem histórico) | Tratar todas as personas como inativas desde a criação |
| Persona nunca teve interação | Considerar inativa desde a data de criação (ou desde sempre) |
| ClawHub API indisponível | Registrar falha no log; não emitir alerta falso; tentar novamente no próximo ciclo |
| Skill sem CVEs conhecidos | Nenhuma ação; registrar verificação bem-sucedida no log |
| Usuário já foi alertado sobre o mesmo CVE | Não repetir o alerta; apenas lembrar se o CVE ainda não foi resolvido após 24h |
| Persona em crise ativa | Não enviar sugestão de retomada para persona em crise (ela já está ativa) |
| Múltiplos CVEs na mesma skill | Listar todos os CVEs da skill em um único bloco de alerta |

---

## Dependências

- `skills/life-log` — `get_persona_summary(persona, days=3)` para verificar inatividade
- `ClawHub API` — `check_cve(skill_name)` para verificar CVEs de skills instaladas
- `USER.md` — lista de personas ativas e seus `current_weight`
- `personas/{slug}/skills.json` — lista de skills instaladas por persona
- `skills/` — lista de skills globais instaladas
