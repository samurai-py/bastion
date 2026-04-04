---
name: bastion/weight-system
version: 1.0.0
description: >
  Calcula a prioridade dinâmica de personas e gerencia ajustes de peso
  (current_weight). Persiste mudanças em USER.md e registra histórico
  com timestamp e justificativa em personas/{slug}/weight-history.md.
triggers:
  - chamada interna do bastion/crisis-mode ao aplicar crisis boost
  - chamada interna do bastion/weekly-review ao sugerir ajustes de peso
  - chamada interna do bastion/self-improving ao registrar promoções/decaimentos
  - usuário solicita explicitamente ajustar o peso de uma persona
  - HEARTBEAT executa a tarefa semanal de revisão de pesos
---

# Skill: bastion/weight-system

## Objetivo

Gerenciar o sistema de pesos dinâmicos das personas do Bastion:

1. **Calcular prioridade** de uma persona dado o contexto atual (deep work, deadline)
2. **Ajustar current_weight** de forma persistida e auditável
3. **Registrar histórico** de todas as mudanças com timestamp e justificativa

---

## Conceitos

### base_weight vs current_weight

| Campo | Descrição | Quando muda |
|---|---|---|
| `base_weight` | Peso fixo definido na criação da persona (0.0–1.0) | Raramente — apenas por edição explícita do usuário |
| `current_weight` | Peso dinâmico atual (0.0–1.0) | Crises, revisões semanais, aprendizados, ajustes manuais |

O `current_weight` é o valor usado em todos os cálculos de prioridade e matching.

### Fórmula de Prioridade

```
priority = current_weight
         + 0.1  × (deep_work)
         + 0.2  × (deadline ≤ 4h)
         + 0.15 × (deadline ≤ 12h)
         + 0.1  × (deadline ≤ 24h)
         + 0.05 × (deadline ≤ 48h)
```

O resultado é sempre clamped em **[0.0, 1.0]**.

Os bônus de deadline são **aditivos** — um deadline de 3h acumula os bônus de ≤4h, ≤12h, ≤24h e ≤48h simultaneamente.

**Exemplos:**

| current_weight | deep_work | deadline | priority |
|---|---|---|---|
| 0.7 | false | none | 0.70 |
| 0.7 | true | none | 0.80 |
| 0.7 | false | 3h | 1.00 (clamped de 1.20) |
| 0.5 | true | 20h | 0.75 |
| 0.9 | false | 50h | 0.90 |

---

## Quando Este Skill é Acionado

### 1. Crisis Boost (bastion/crisis-mode)

Quando o `crisis-mode` detecta uma crise e identifica a persona afetada:

```
adjust_weight(
    persona_slug=<slug da persona em crise>,
    delta=+0.3,
    justification="Crisis boost: <descrição da crise>",
    persistence=UserMdAdapter(...)
)
```

O clamp garante que o resultado nunca ultrapasse 1.0.

---

### 2. Revisão Semanal (bastion/weekly-review)

Toda segunda-feira às 9h, o HEARTBEAT aciona o `weekly-review`, que:

1. Analisa os últimos 50 registros do life_log por persona
2. Compara o padrão de uso com os pesos atuais
3. Sugere ajustes ao usuário
4. Após confirmação, chama `adjust_weight()` para cada persona com sugestão aceita

---

### 3. Self-Improving (bastion/self-improving)

Quando um padrão é promovido ou decaído, o `self-improving` registra a mudança:

```
adjust_weight(
    persona_slug=<slug>,
    delta=<+0.05 para promoção, -0.05 para decaimento>,
    justification="Pattern promoted to HOT: <nome do padrão>",
    persistence=UserMdAdapter(...)
)
```

---

### 4. Ajuste Manual pelo Usuário

O usuário pode solicitar ajuste direto:

```
"Aumenta o peso da persona Tech Lead para 0.95"
"Reduz o peso do Empreendedor em 0.1"
```

Fluxo:
1. Identificar a persona pelo nome ou slug
2. Calcular o delta necessário
3. Confirmar com o usuário: `"Vou ajustar o peso de '{nome}' de {atual} para {novo}. Confirma? (sim/não)"`
4. Após confirmação, chamar `adjust_weight()`

---

## Persistência

### USER.md

O `current_weight` de cada persona é mantido no frontmatter de `USER.md`:

```yaml
personas:
  - slug: "tech-lead"
    name: "Tech Lead"
    current_weight: 0.9
  - slug: "empreendedor"
    name: "Empreendedor"
    current_weight: 0.7
```

O `UserMdAdapter` atualiza este valor automaticamente a cada chamada de `adjust_weight()`.

### personas/{slug}/weight-history.md

Cada ajuste gera uma linha de histórico:

```
# Weight History

- 2025-01-15T10:30:00+00:00 | 0.7000 → 1.0000 | Crisis boost: servidor de produção fora do ar
- 2025-01-22T09:00:00+00:00 | 1.0000 → 0.8500 | Weekly review: uso normalizado após crise
- 2025-01-29T09:00:00+00:00 | 0.8500 → 0.9000 | Pattern promoted to HOT: deploy-checklist
```

Formato de cada linha:
```
- {ISO 8601 timestamp} | {old_weight:.4f} → {new_weight:.4f} | {justification}
```

---

## Arquitetura (Hexagonal)

O skill usa o padrão **Protocol/Adapter** para desacoplar a lógica de negócio da persistência:

```
WeightPersistenceProtocol (porta)
    ├── get_current_weight(slug) → float
    ├── set_current_weight(slug, weight) → None
    └── append_weight_history(slug, entry) → None

UserMdAdapter (adaptador concreto padrão)
    ├── Lê/escreve USER.md (frontmatter YAML)
    └── Appenda em personas/{slug}/weight-history.md
```

Para trocar o backend de persistência (ex: banco de dados), basta implementar um novo adapter que satisfaça o `WeightPersistenceProtocol` — sem alterar `calculate_priority()` ou `adjust_weight()`.

---

## Comandos CLI

> IMPORTANTE: Comando CLI
> Como você é um agente OpenClaw, você deve invocar todas as operações via linha de comando (`exec python3 ...`). Não tente interpretar o código Python nativamente.



```python
from skills.weight_system.weight_system import (
    calculate_priority,
    adjust_weight,
    UserMdAdapter,
    WeightHistoryEntry,
)
from pathlib import Path

# Instanciar o adapter
adapter = UserMdAdapter(
    user_md_path=Path("USER.md"),
    personas_dir=Path("personas"),
)

# Calcular prioridade
priority = calculate_priority(
    current_weight=0.7,
    deep_work=True,
    deadline_hours=6.0,
)
# → 0.95 (0.7 + 0.1 + 0.15)

# Ajustar peso (crisis boost)
new_weight = adjust_weight(
    persona_slug="tech-lead",
    delta=+0.3,
    justification="Crisis boost: deploy crítico em produção",
    persistence=adapter,
)
# → persiste em USER.md + appenda em personas/tech-lead/weight-history.md
```

---

## Edge Cases

### Clamp em [0.0, 1.0]

Qualquer delta que levaria o peso abaixo de 0.0 ou acima de 1.0 é silenciosamente clamped:

```python
adjust_weight("tech-lead", delta=+0.5, ...)  # current=0.9 → new=1.0 (não 1.4)
adjust_weight("tech-lead", delta=-0.5, ...)  # current=0.1 → new=0.0 (não -0.4)
```

### Persona não encontrada em USER.md

Se o slug não existir em USER.md, `UserMdAdapter.get_current_weight()` lança `KeyError`.
O chamador deve tratar este erro e verificar se a persona existe antes de ajustar.

### Arquivo weight-history.md inexistente

O `UserMdAdapter` cria o arquivo automaticamente na primeira escrita, incluindo o cabeçalho `# Weight History`.

### Pasta personas/{slug}/ inexistente

O `UserMdAdapter` cria a pasta automaticamente via `mkdir(parents=True, exist_ok=True)`.
