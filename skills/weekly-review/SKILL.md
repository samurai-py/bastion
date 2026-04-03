---
name: bastion/weekly-review
version: "1.0.0"
description: >
  Executa a revisão semanal de todas as personas ativas: agrega interações dos
  últimos 7 dias via life_log, calcula métricas de uso, compara com os pesos
  atuais e apresenta um relatório com sugestões de ajuste de peso para confirmação
  do usuário antes de aplicar qualquer mudança.
triggers:
  - HEARTBEAT toda segunda-feira às 9h
  - "/weekly-review"
  - "revisão semanal"
  - "revisar pesos"
  - "como estão minhas personas"
---

# Weekly Review — Revisão Semanal de Personas

## Quando este skill é ativado

1. **Automático**: o HEARTBEAT executa este skill toda segunda-feira às 9h.
2. **Manual**: o usuário envia `/weekly-review` ou solicita uma revisão semanal.

---

## Fluxo completo

```
HEARTBEAT (segunda, 9h) ou trigger manual
        │
        ▼
Carregar lista de personas ativas de USER.md
        │
        ▼
Para cada persona ativa:
  life_log.get_persona_summary(persona, days=7)
        │
        ▼
Calcular métricas de uso por persona
        │
        ▼
Comparar padrão de uso com current_weight de cada persona
        │
        ▼
Gerar relatório com sugestões de ajuste
        │
        ├── Nenhuma sugestão → informar que os pesos estão adequados
        │
        └── Há sugestões → apresentar relatório ao usuário
                │
                ▼
          Aguardar confirmação do usuário
                │
                ├── Usuário confirma tudo → aplicar todos os ajustes via weight-system
                ├── Usuário confirma parcialmente → aplicar apenas os confirmados
                └── Usuário recusa → não aplicar nenhum ajuste
```

---

## Passo 1 — Coletar dados do life_log

Para cada persona ativa listada em `USER.md`, chamar:

```
life_log.get_persona_summary(persona="{slug}", days=7)
```

O resumo retorna:
- `total_interactions`: número total de interações nos últimos 7 dias
- `intents_used`: lista de intents executados com contagem (ex: `{"code_review": 12, "planning": 3}`)
- `tools_used`: lista de tools chamadas com contagem (ex: `{"github": 8, "calendar": 2}`)
- `active_hours`: lista de horas do dia com maior atividade (ex: `[9, 10, 14, 15]`)
- `last_interaction`: timestamp da última interação

Se uma persona não tiver nenhuma interação nos últimos 7 dias, registrar `total_interactions=0`.

---

## Passo 2 — Calcular métricas de uso

Para cada persona, calcular:

| Métrica | Cálculo |
|---------|---------|
| **Taxa de uso** | `total_interactions / total_interactions_todas_personas` |
| **Intent dominante** | intent com maior contagem |
| **Tool dominante** | tool com maior contagem |
| **Janela de atividade** | horas com ≥ 20% das interações da persona |
| **Dias desde última interação** | `hoje - last_interaction` em dias |

---

## Passo 3 — Comparar com pesos atuais

Para cada persona, comparar a taxa de uso com o `current_weight`:

### Critérios de sugestão de aumento de peso

Sugerir **aumento** de `current_weight` se:
- `taxa_de_uso > current_weight + 0.15` (persona sendo usada muito mais do que o peso sugere)
- `total_interactions >= 20` na semana (uso consistente e expressivo)

Valor sugerido: `min(current_weight + 0.1, 1.0)`

### Critérios de sugestão de redução de peso

Sugerir **redução** de `current_weight` se:
- `taxa_de_uso < current_weight - 0.2` (persona sendo usada muito menos do que o peso sugere)
- `total_interactions <= 3` na semana (uso muito baixo)
- `current_weight > 0.3` (não reduzir personas já com peso baixo)

Valor sugerido: `max(current_weight - 0.1, 0.0)`

### Sem sugestão

Manter o peso atual se nenhum critério acima for atendido.

---

## Passo 4 — Gerar relatório

Montar o relatório em linguagem clara e acessível para o usuário:

```
📊 Revisão Semanal — {data_inicio} a {data_fim}

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

{Para cada persona com interações:}

🧠 {Nome da Persona}
   Interações: {total_interactions} esta semana
   Mais usada para: {intent_dominante}
   Ferramenta favorita: {tool_dominante}
   Horários de pico: {janela_de_atividade}
   Peso atual: {current_weight}
   {Se há sugestão:}
   💡 Sugestão: ajustar peso para {novo_peso} ({motivo})

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

{Se há personas sem interações:}
😴 Personas sem atividade esta semana:
   • {persona_1} (último uso: {dias} dias atrás)
   • {persona_2} (último uso: {dias} dias atrás)

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

{Se há sugestões de ajuste:}
Encontrei {N} sugestão(ões) de ajuste de peso.
Quer que eu aplique as sugestões acima? (sim / não / escolher)

{Se não há sugestões:}
✅ Os pesos das suas personas estão bem calibrados para esta semana.
Nenhum ajuste necessário.
```

**Regras de linguagem do relatório:**
- Usar linguagem simples, sem jargões técnicos
- Substituir "current_weight" por "peso" ou "prioridade"
- Substituir "intent" por "tipo de tarefa" ou "o que foi feito"
- Substituir "tool" por "ferramenta"
- Datas no formato `DD/MM/YYYY`

---

## Passo 5 — Aguardar confirmação antes de aplicar

**Nunca aplicar ajustes sem confirmação explícita do usuário.**

### Opções de resposta do usuário

| Resposta | Ação |
|----------|------|
| `sim` / `confirmar` / `aplicar tudo` | Aplicar todos os ajustes sugeridos |
| `não` / `cancelar` / `manter` | Não aplicar nenhum ajuste |
| `escolher` / `selecionar` | Listar cada sugestão individualmente para confirmação |

### Fluxo de confirmação individual (quando usuário responde "escolher")

Para cada sugestão, perguntar:
> "Ajustar peso de **{Nome da Persona}** de {peso_atual} para {novo_peso}? (sim/não)"

Aguardar resposta antes de passar para a próxima.

---

## Passo 6 — Aplicar ajustes confirmados via weight-system

Para cada ajuste confirmado, chamar:

```
weight_system.adjust_weight(
    persona_slug="{slug}",
    delta={novo_peso - peso_atual},
    justification="Revisão semanal: taxa de uso {taxa_de_uso:.0%} vs peso {peso_atual} — {motivo}"
)
```

O `weight-system` persiste o novo `current_weight` em `USER.md` e registra a mudança
com timestamp e justificativa em `personas/{slug}/weight-history.md`.

Após aplicar todos os ajustes confirmados, confirmar ao usuário:

```
✅ Ajustes aplicados:
   • {Persona 1}: {peso_antigo} → {peso_novo}
   • {Persona 2}: {peso_antigo} → {peso_novo}

Os pesos foram atualizados. Até a próxima segunda! 👋
```

---

## Edge cases

| Situação | Comportamento |
|----------|---------------|
| Nenhuma persona ativa em USER.md | Informar que não há personas configuradas e sugerir o onboarding |
| life_log vazio (primeira semana) | Informar que ainda não há dados suficientes e que a revisão será mais útil na próxima semana |
| Todas as personas sem interações | Apresentar relatório de inatividade e sugerir retomada, sem sugestões de peso |
| Usuário não responde em 24h | Não aplicar nenhum ajuste; registrar que a revisão foi apresentada mas não confirmada |
| Ajuste resultaria em peso < 0.0 ou > 1.0 | Clamp automático pelo weight-system; informar o valor final real ao usuário |
| Persona em crise ativa | Não sugerir redução de peso para persona com crise ativa, independentemente da taxa de uso |
| Empate entre personas na taxa de uso | Manter pesos atuais; não sugerir ajuste quando a diferença for < 0.05 |

---

## Dependências

- `skills/life-log` — `get_persona_summary(persona, days=7)` para coletar dados de uso
- `skills/weight-system` — `adjust_weight(persona_slug, delta, justification)` para aplicar ajustes
- `USER.md` — lista de personas ativas e seus `current_weight`
- `personas/{slug}/weight-history.md` — histórico de ajustes (escrito pelo weight-system)
