---
name: bastion/life-log
version: 1.0.0
description: >
  Registra interações por persona com embeddings vetoriais e permite busca
  semântica (RAG) sobre o histórico. Persiste em SQLite local (padrão) ou
  Supabase (opcional via DB_STRATEGY). Zero dependência externa — funciona
  offline com dados 100% sob controle do usuário.
triggers:
  - ao final de qualquer interação relevante com uma persona ativa
  - quando o orquestrador prepara contexto para uma resposta (busca semântica)
  - chamada interna do bastion/weekly-review para análise dos últimos 7 dias
  - chamada interna do bastion/proactive para detectar personas inativas
  - chamada interna do bastion/self-improving para extrair padrões de uso
---

# Skill: bastion/life-log

## Objetivo

Manter um histórico semântico de interações por persona, permitindo que o
Bastion:

1. **Lembre** de conversas passadas sem que o usuário precise repetir contexto
2. **Busque** os registros mais relevantes para enriquecer respostas (RAG)
3. **Analise** padrões de uso por persona ao longo do tempo
4. **Detecte** personas inativas para sugestões proativas

---

## Comandos CLI

> IMPORTANTE: Comando CLI
> Como você é um agente OpenClaw, você deve invocar todas as operações via linha de comando (`exec python3 ...`). Não tente interpretar o código Python nativamente.



```python
from skills.life_log.factory import Settings, create_adapter
from datetime import datetime, timezone

# Instanciar via factory (lê DB_STRATEGY do ambiente)
settings = Settings.from_env()
adapter = create_adapter(settings)

# Registrar uma interação
interaction_id = await adapter.log_interaction(
    persona="tech-lead",
    intent="code-review",
    tools=["github-integration", "code-review-helper"],
    embedding=[0.1, 0.2, ...],   # gerado pelo LLM configurado
    timestamp=datetime.now(tz=timezone.utc),
)

# Busca semântica (RAG)
similar = await adapter.search_similar(
    query_embedding=[0.1, 0.2, ...],
    persona="tech-lead",          # None para buscar em todas as personas
    limit=5,
    threshold=0.65,
)

# Resumo dos últimos 7 dias
summary = await adapter.get_persona_summary(
    persona="tech-lead",
    days=7,
)
```

---

## Campos Obrigatórios de Cada Registro

| Campo | Tipo | Descrição |
|---|---|---|
| `id` | `str` (UUID) | Identificador único gerado automaticamente |
| `persona` | `str` | Slug da persona ativa na interação |
| `intent` | `str` | Intent executado (ex: "code-review", "deploy-check") |
| `tools` | `list[str]` | Nomes das tools usadas na interação |
| `embedding` | `list[float]` | Vetor de embedding do input (gerado pelo LLM) |
| `timestamp` | `datetime` | Data/hora UTC da interação |

---

## Busca Semântica

### Threshold e Limite

- **threshold padrão:** `0.65` — apenas resultados com similaridade coseno ≥ 0.65
- **limit padrão:** `5` — no máximo 5 resultados por busca
- Resultados ordenados por similaridade decrescente

### Geração de Embeddings

Os embeddings são gerados pelo LLM configurado no OpenClaw (Gemini, OpenAI, Groq).
O skill não gera embeddings diretamente — recebe o vetor já calculado pelo orquestrador.

### Injeção de Contexto no Prompt

O orquestrador usa `search_similar` antes de montar cada resposta:

```
1. Gera embedding do input do usuário
2. Chama search_similar(query_embedding, persona=persona_ativa, limit=5, threshold=0.65)
3. Injeta os registros retornados como contexto histórico no system prompt
4. Responde com contexto enriquecido
```

---

## DB_STRATEGY Pattern

O skill usa o padrão **Protocol/Adapter** (hexagonal) para desacoplar a lógica
de negócio do backend de persistência.

### Trocar de SQLite para Supabase

Basta alterar uma variável no `.env`:

```env
# Padrão — SQLite local, zero dependência externa
DB_STRATEGY=sqlite
SQLITE_PATH=db/life-log.db

# Opcional — Supabase (PostgreSQL + pgvector)
DB_STRATEGY=supabase
SUPABASE_URL=https://seu-projeto.supabase.co
SUPABASE_KEY=sua-chave-anon-ou-service
```

Nenhum código do skill precisa ser alterado. A factory (`create_adapter`) instancia
o adapter correto com base em `DB_STRATEGY`.

### Adapters Disponíveis

| DB_STRATEGY | Adapter | Dependência |
|---|---|---|
| `sqlite` (padrão) | `SQLiteLifeLogAdapter` | Nenhuma (stdlib) |
| `supabase` | `SupabaseLifeLogAdapter` | `supabase-py` + projeto Supabase |

---

## Persistência SQLite

### Localização do Banco

```
db/life-log.db   ← criado automaticamente na primeira escrita
```

O diretório `db/` é criado automaticamente se não existir.

### Schema

```sql
CREATE TABLE interactions (
    id        TEXT PRIMARY KEY,   -- UUID v4
    persona   TEXT NOT NULL,      -- slug da persona
    intent    TEXT NOT NULL,      -- intent executado
    tools     TEXT NOT NULL,      -- JSON array de nomes de tools
    embedding BLOB NOT NULL,      -- float32 little-endian
    timestamp TEXT NOT NULL       -- ISO 8601 UTC
);

CREATE INDEX idx_interactions_persona   ON interactions (persona);
CREATE INDEX idx_interactions_timestamp ON interactions (timestamp);
```

### sqlite-vec

Quando a extensão `sqlite-vec` está instalada, ela é carregada automaticamente
para busca vetorial nativa. Se não estiver disponível, o adapter usa similaridade
coseno em Python puro — funciona offline sem nenhuma dependência nativa.

---

## Arquitetura (Hexagonal)

```
LifeLogProtocol (porta)
    ├── log_interaction(persona, intent, tools, embedding, timestamp) → str
    ├── search_similar(query_embedding, persona, limit, threshold) → list[InteractionRecord]
    └── get_persona_summary(persona, days) → list[InteractionRecord]

SQLiteLifeLogAdapter (adaptador padrão)
    ├── Cria db/life-log.db automaticamente
    ├── Usa sqlite-vec quando disponível
    └── Fallback para cosine similarity em Python puro

SupabaseLifeLogAdapter (adaptador opcional)
    ├── Usa supabase-py + pgvector
    └── Stub — requer implementação completa para produção

Settings + create_adapter() (factory)
    └── Instancia o adapter correto com base em DB_STRATEGY
```

---

## Quando Este Skill é Acionado

### 1. Logging de Interação (após cada resposta)

O orquestrador chama `log_interaction` ao final de cada interação relevante:

```python
await adapter.log_interaction(
    persona=persona_ativa.slug,
    intent=intent_detectado,
    tools=tools_usadas,
    embedding=await llm.embed(user_input),
    timestamp=datetime.now(tz=timezone.utc),
)
```

### 2. Busca Semântica (antes de cada resposta)

O orquestrador enriquece o contexto antes de responder:

```python
context = await adapter.search_similar(
    query_embedding=await llm.embed(user_input),
    persona=persona_ativa.slug,
    limit=5,
    threshold=0.65,
)
# Injeta context no system prompt
```

### 3. Weekly Review (HEARTBEAT — toda segunda às 9h)

```python
summary = await adapter.get_persona_summary(persona=slug, days=7)
# Analisa padrões, sugere ajustes de peso
```

### 4. Detecção de Inatividade (HEARTBEAT — a cada 6h)

```python
for persona in all_personas:
    recent = await adapter.get_persona_summary(persona=persona.slug, days=3)
    if not recent:
        # Gerar sugestão de retomada
```

---

## Edge Cases

### Banco não existe na primeira leitura

O `SQLiteLifeLogAdapter` cria o arquivo e o schema automaticamente antes de
qualquer operação. Não é necessário inicialização manual.

### Embedding com dimensão diferente

`search_similar` compara embeddings usando similaridade coseno. Se os vetores
tiverem dimensões diferentes, um `ValueError` é lançado. Certifique-se de usar
sempre o mesmo modelo de embedding.

### Threshold muito alto (sem resultados)

Se nenhum registro atingir o threshold, `search_similar` retorna lista vazia.
O orquestrador deve tratar este caso e responder sem contexto histórico.

### Persona sem histórico

`get_persona_summary` retorna lista vazia para personas sem interações no período.
Não é um erro — indica persona nova ou inativa.

---

## Output Example

```json
{
  "id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "persona": "tech-lead",
  "intent": "code-review",
  "tools": ["github-integration", "code-review-helper"],
  "timestamp": "2024-01-15T10:30:00Z"
}
```
