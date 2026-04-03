---
name: bastion/self-improving
version: 1.0.0
description: >
  Fork do ivangdavila/self-improving com consciência de personas.
  Aprende padrões de comportamento por persona ao longo do tempo usando
  memória tiered (HOT/WARM/COLD), promoção/decaimento automático com
  consciência de peso, resolução de conflitos por precedência e isolamento
  completo de namespace entre personas.
triggers:
  - HEARTBEAT executa análise semanal do life_log (a cada 7 dias)
  - padrão de comportamento observado 3+ vezes em 7 dias para uma persona
  - persona entra em modo crise (crisis-mode detecta is_crisis=true)
  - dois padrões conflitam durante matching de persona
  - bastion/weekly-review solicita análise de padrões acumulados
---

# Skill: bastion/self-improving

## Objetivo

Aprender padrões de comportamento por persona ao longo do tempo, melhorando
progressivamente as respostas sem que o usuário precise repetir preferências.

---

## Tiered Memory (mantido do original)

Cada persona tem três camadas de memória:

| Tier | Arquivo | Tamanho | Carregamento |
|------|---------|---------|--------------|
| **HOT** | `personas/{slug}/memory.md` | ≤ 100 linhas | Sempre — injetado no contexto |
| **WARM** | `personas/{slug}/index.md` | Ilimitado | Sob demanda (busca semântica) |
| **COLD** | `personas/{slug}/archive/` | Ilimitado | Raramente — busca explícita |

---

## Regras de Promoção com Consciência de Peso

### Promoção para HOT (Requirement 12.1)

Um padrão é promovido para HOT quando:
- Observado **3 ou mais vezes** em um janela de **7 dias**

### Bloqueio por Peso Baixo (Requirement 12.2)

Se `current_weight < 0.3` para uma persona:
- O padrão **não é promovido para HOT global**
- Permanece em WARM até que o peso da persona aumente
- Justificativa registrada: `"Weight gate: current_weight=X.XXXX < 0.3"`

### Prioridade em Crise (Requirement 12.3)

Quando uma crise é detectada pelo `bastion/crisis-mode`:
- Os padrões da **persona em crise** têm prioridade sobre todas as outras
- O gate de peso é **bypassado** para a persona em crise
- Justificativa registrada: `"Crisis priority: N occurrences (crisis override — weight gate bypassed)"`

---

## Conflict Resolution (Requirement 12.4)

Quando dois padrões conflitam, a ordem de precedência é:

```
1. Mais específico  (maior valor de specificity)
2. Mais recente     (maior updated_at)
3. Maior peso       (maior persona_weight)
```

Se todos os critérios empatarem, o padrão `pattern_a` vence (determinístico).

**Exemplo:**

```python
from skills.self_improving.promotion import conflict_resolution

winner = conflict_resolution(pattern_a, pattern_b)
# → retorna o Pattern vencedor com log do critério usado
```

---

## Registro de Promoções e Decaimentos (Requirement 12.5)

Toda promoção ou decaimento é registrado em `personas/{slug}/weight-history.md`:

```
# Weight History

- 2025-01-15T10:30:00+00:00 | PROMOTED WARM → HOT | pattern:deploy-checklist | Promotion criteria met: 4 occurrences in last 7 days, current_weight=0.9000
- 2025-01-22T09:00:00+00:00 | DECAYED HOT → WARM  | pattern:deploy-checklist | Pattern not accessed in 14 days
- 2025-01-29T09:00:00+00:00 | PROMOTED WARM → HOT | pattern:deploy-checklist | Crisis priority: 3 occurrences (crisis override — weight gate bypassed)
```

Formato de cada linha:
```
- {ISO 8601 timestamp} | {action} | pattern:{id} | {justification}
```

---

## Isolamento de Namespace (Requirement 12.6)

**Garantia:** Operações em `personas/{slug-a}/` **nunca** tocam `personas/{slug-b}/`.

O `FileSystemAdapter` deriva todos os caminhos de `self._personas_dir / persona_slug`.
Não há operação que aceite dois slugs diferentes na mesma chamada de escrita.

---

## Arquitetura (Hexagonal)

```
PromotionPersistenceProtocol (porta)
    ├── get_pattern(persona_slug, pattern_id) → Pattern | None
    ├── save_pattern(pattern) → None
    ├── get_current_weight(persona_slug) → float
    └── append_promotion_history(persona_slug, timestamp, pattern_id, action, justification) → None

FileSystemAdapter (adaptador concreto padrão)
    ├── Lê/escreve personas/{slug}/memory.md (HOT tier)
    ├── Lê current_weight de USER.md
    └── Appenda em personas/{slug}/weight-history.md
```

Para trocar o backend (ex: banco de dados), basta implementar um novo adapter
que satisfaça o `PromotionPersistenceProtocol` — sem alterar `promote_pattern()`,
`decay_pattern()` ou `conflict_resolution()`.

---

## Interface Python

```python
from pathlib import Path
from datetime import datetime, timezone
from skills.self_improving.promotion import (
    Pattern,
    MemoryTier,
    FileSystemAdapter,
    promote_pattern,
    decay_pattern,
    conflict_resolution,
)

adapter = FileSystemAdapter(
    personas_dir=Path("personas"),
    user_md_path=Path("USER.md"),
)

# Criar um padrão com ocorrências recentes
pattern = Pattern(
    id="deploy-checklist",
    persona_slug="tech-lead",
    description="Sempre verifica o checklist de deploy antes de fazer push",
    tier=MemoryTier.WARM,
    specificity=3,
    persona_weight=0.9,
    occurrences=[
        datetime(2025, 1, 13, tzinfo=timezone.utc),
        datetime(2025, 1, 14, tzinfo=timezone.utc),
        datetime(2025, 1, 15, tzinfo=timezone.utc),
    ],
)

# Tentar promover para HOT
promoted = promote_pattern(pattern, adapter, is_crisis=False)
# → True se current_weight >= 0.3 e 3+ ocorrências em 7 dias

# Resolver conflito entre dois padrões
winner = conflict_resolution(pattern_a, pattern_b)
# → Pattern vencedor pela ordem: específico > recente > peso

# Decair um padrão
decay_pattern(pattern, MemoryTier.WARM, "Pattern not accessed in 14 days", adapter)
```

---

## Integração com HEARTBEAT

O HEARTBEAT aciona este skill a cada 7 dias via `bastion/weekly-review`:

1. Busca os últimos 50 registros do `life_log` por persona
2. Agrupa por padrão de comportamento
3. Para cada padrão com 3+ ocorrências em 7 dias → chama `promote_pattern()`
4. Para padrões sem acesso há 14+ dias → chama `decay_pattern()`
5. Registra todas as mudanças em `personas/{slug}/weight-history.md`

---

## Edge Cases

### Persona com peso < 0.3

O padrão permanece em WARM. Quando o peso da persona aumentar (via crisis boost
ou weekly-review), a próxima execução do HEARTBEAT pode promovê-lo.

### Crise ativa

Durante uma crise, `is_crisis=True` bypassa o gate de peso. Após a crise,
o comportamento normal é restaurado automaticamente.

### Padrão já em HOT

Chamar `promote_pattern()` em um padrão já HOT é idempotente — atualiza
`updated_at` e `persona_weight`, mas não duplica a entrada no histórico.

### Arquivo memory.md inexistente

O `FileSystemAdapter` cria o arquivo automaticamente na primeira escrita,
incluindo o cabeçalho `# HOT Memory — {slug}`.

### Pasta personas/{slug}/ inexistente

O `FileSystemAdapter` cria a pasta automaticamente via `mkdir(parents=True, exist_ok=True)`.
