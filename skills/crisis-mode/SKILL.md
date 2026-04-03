---
name: bastion/crisis-mode
version: 1.0.0
description: >
  Detecta situações de crise e executa o sacrifice algorithm para liberar
  tempo de Deep Work, replanejando automaticamente a agenda da persona afetada.
triggers:
  - "/crise"
  - linguagem de urgência extrema (confiança > 0.8)
---

# Crisis Mode

## Quando este skill é ativado

O Crisis Mode é ativado em duas situações:

1. **Trigger explícito**: o usuário envia `/crise` em qualquer mensagem.
2. **Detecção automática**: a mensagem contém linguagem de urgência extrema e o
   classificador interno retorna `confidence > 0.8`. Exemplos de linguagem que
   ativam a detecção: "urgente", "emergência", "sistema caiu", "prazo hoje",
   "tudo parado", "não pode esperar".

---

## Fluxo completo

```
Mensagem recebida
      │
      ▼
detect_crisis(message)
      │
      ├── is_crisis=False → encerrar, processar normalmente
      │
      └── is_crisis=True
              │
              ▼
        Identificar persona afetada
              │
              ▼
        sacrifice_algorithm(persona_slug, current_weight, tasks)
              │
              ├── fallback=True (< 2h disponíveis)
              │       │
              │       └── Notificar usuário com opções disponíveis
              │           (sem executar nada)
              │
              └── fallback=False
                      │
                      ▼
                Cancelar/mover tarefas selecionadas
                      │
                      ▼
                Notificar usuário com resumo do replanejamento
                      │
                      ▼
                record_crisis_event(persona_slug, result)
                      │
                      ▼
                Appenda evento em personas/{slug}/MEMORY.md
```

---

## Sacrifice Algorithm — detalhes

### 1. Boost de peso

```
new_weight = min(current_weight + 0.3, 1.0)
```

O peso da persona em crise é elevado em 0.3, respeitando o limite máximo de 1.0.

### 2. Filtro de tarefas sacrificáveis

Uma tarefa é sacrificável se satisfizer **ambos** os critérios:

- `movable = True` — a tarefa pode ser movida ou cancelada
- `priority < new_weight * 0.6` — a prioridade da tarefa é baixa o suficiente

### 3. Seleção de tarefas

As tarefas sacrificáveis são ordenadas por prioridade crescente (menor prioridade
primeiro). O algoritmo seleciona tarefas até liberar **≥ 2 horas de Deep Work**.

### 4. Fallback

Se a soma das horas das tarefas sacrificáveis for **< 2h**, o algoritmo retorna
`fallback=True` com a lista de opções disponíveis, **sem executar nenhuma ação**.
O usuário é notificado e pode decidir o que fazer.

---

## Notificação ao usuário

### Caso normal (replanejamento executado)

```
🚨 Modo Crise ativado para [Persona]

Peso elevado: [old_weight] → [new_weight]

Tarefas movidas/canceladas para liberar [X]h de Deep Work:
  • [Tarefa 1] — [duração]h
  • [Tarefa 2] — [duração]h

Você tem [X]h livres para focar no problema urgente.
```

### Caso fallback (horas insuficientes)

```
⚠️ Modo Crise — horas insuficientes

Não foi possível liberar 2h de Deep Work com as tarefas disponíveis.
Tarefas que poderiam ser movidas (total: [X]h):
  • [Tarefa 1] — [duração]h
  • [Tarefa 2] — [duração]h

Nenhuma ação foi executada. O que você gostaria de fazer?
```

---

## Registro em MEMORY.md

Após cada execução (com ou sem fallback), o evento é registrado em
`personas/{slug}/MEMORY.md` com:

- Timestamp ISO 8601
- Status: EXECUTED ou FALLBACK
- Novo peso da persona
- Horas liberadas
- Lista de tarefas sacrificadas (ou opções disponíveis no fallback)

---

## Edge cases

| Situação | Comportamento |
|----------|---------------|
| Nenhuma tarefa `movable=True` | Fallback imediato com lista vazia |
| Todas as tarefas têm prioridade alta | Fallback com lista vazia |
| `current_weight` já é 1.0 | Boost não altera o peso (permanece 1.0) |
| Mensagem contém `/crise` + outras palavras | Trigger explícito tem precedência (confidence=1.0) |
| Persona não identificada | `affected_persona=None`, usuário é perguntado qual persona |

---

## Dependências

- `crisis_mode.py` — lógica computacional (detect_crisis, sacrifice_algorithm, record_crisis_event)
- `personas/{slug}/MEMORY.md` — arquivo de memória da persona (criado automaticamente se não existir)
