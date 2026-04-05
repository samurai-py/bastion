---
name: bastion/persona-engine
version: 1.0.0
description: >
  Criação, edição e matching de personas do Bastion. Conduz fluxo conversacional
  para criar novas personas, gera o arquivo personas/{slug}/SOUL.md com frontmatter
  YAML obrigatório, e executa o algoritmo de matching para identificar qual persona
  (ou quais personas) deve estar ativa em cada mensagem recebida.
triggers:
  - "/nova-persona"
  - "/criar-persona"
  - "/editar-persona"
  - mensagem do usuário que solicita criar, editar ou listar personas
  - chamada interna do bastion/onboarding durante criação automática de personas
---

# Skill: bastion/persona-engine

## Objetivo

Gerenciar o ciclo de vida completo de personas: criação via fluxo conversacional,
persistência em `personas/{slug}/SOUL.md`, e matching em tempo real para determinar
qual persona (ou quais personas simultâneas) deve responder a cada mensagem.

---

## Parte 1 — Criação de Persona (Persona Builder)

### Quando acionar

- Usuário envia `/nova-persona` ou `/criar-persona`
- Usuário solicita explicitamente criar uma nova persona ("quero criar uma persona para X")
- Chamada interna do `bastion/onboarding` para cada área de vida informada

### Fluxo de Criação

#### Passo 1 — Nome da persona

Enviar ao usuário:

```
Vamos criar uma nova persona. 🎭

Qual será o nome desta persona?
(Ex: "Tech Lead", "Empreendedor", "Pai de Família", "Atleta")
```

Aguardar resposta. Armazenar como `persona.name`.

**Validação:** nome não pode ser vazio. Se vazio, repetir a pergunta.

---

#### Passo 2 — Domínio de atuação

Enviar:

```
Qual é o domínio de atuação de "{persona.name}"?

Descreva as áreas de conhecimento e responsabilidade desta persona.
(Ex: "código, arquitetura de software, liderança técnica de times")
```

Aguardar resposta. Parsear a resposta em uma lista de domínios.
Armazenar como `persona.domains: list[str]`.

---

#### Passo 3 — Tom de voz

Enviar:

```
Como "{persona.name}" deve se comunicar?

Escolha as características que melhor descrevem o tom de voz:

1. Formalidade: formal / informal
2. Profundidade: direto e objetivo / detalhado e explicativo
3. Estilo: técnico / acessível / motivacional / analítico

Responda livremente ou use os exemplos acima como guia.
(Ex: "informal, direto, técnico" ou "formal e detalhado, com exemplos práticos")
```

Aguardar resposta. Armazenar como `persona.voice_tone: str` (texto livre descritivo).

---

#### Passo 4 — Keywords de ativação

Enviar:

```
Quais palavras ou expressões devem ativar "{persona.name}"?

Estas são as trigger_keywords — quando aparecerem em uma mensagem, esta persona
será considerada para responder.

Liste as keywords separadas por vírgula.
(Ex: "PR, review, deploy, bug, arquitetura, código, refactor")
```

Aguardar resposta. Parsear em lista. Armazenar como `persona.trigger_keywords: list[str]`.

**Validação:** deve ter pelo menos 1 keyword. Se vazio, repetir a pergunta.

---

#### Passo 5 — Skills do ClawHub

Enviar:

```
Quais skills do ClawHub são relevantes para "{persona.name}"?

Posso sugerir com base no domínio informado, ou você pode listar diretamente.

Sugestões para "{persona.domains}":
{lista de sugestões inferidas — ver tabela de inferência abaixo}

Digite os nomes das skills desejadas, separados por vírgula.
Ou responda "nenhuma" para pular.
```

Aguardar resposta. Armazenar como `persona.clawhub_skills: list[str]`.

Se o usuário confirmar skills, verificar cada uma conforme a política de segurança
(badge Verified + rating ≥ 4.0 + 50+ reviews) antes de instalar.

**Tabela de inferência de skills por domínio:**

| Domínio contém | Skills sugeridas |
|---|---|
| código / software / dev / tech | `github-integration`, `code-review-helper`, `jira-tasks` |
| negócio / empreendimento / startup | `google-calendar`, `notion-tasks`, `web-search` |
| saúde / bem-estar / fitness | `web-search` |
| família / casa | `google-calendar` |
| finanças / dinheiro / investimento | `web-search` |
| estudos / aprendizado / educação | `web-search`, `notion-tasks` |
| projetos / criação / hobby | `github-integration`, `notion-tasks` |
| marketing / redes sociais / conteúdo | `web-search`, `notion-tasks` |
| agenda / calendário / reuniões | `google-calendar` |

Para domínios não mapeados: sugerir `web-search` como padrão mínimo.

---

#### Passo 6 — Peso base

Enviar:

```
Qual é o peso base de "{persona.name}"?

O peso base (0.0 a 1.0) define a prioridade padrão desta persona.
Personas com peso maior têm mais chance de ser ativadas quando há ambiguidade.

• 0.9–1.0: persona principal, alta prioridade
• 0.6–0.8: persona importante, prioridade média-alta
• 0.3–0.5: persona secundária, prioridade média
• 0.1–0.2: persona de baixa prioridade

Digite um valor entre 0.0 e 1.0:
```

Aguardar resposta. Validar que é um número no intervalo [0.0, 1.0].
Armazenar como `persona.base_weight: float`.

**Validação:** se fora do intervalo ou não numérico, informar e repetir a pergunta.

---

#### Passo 7 — Confirmação e geração

Exibir resumo para confirmação:

```
Resumo da nova persona:

🎭 Nome: {persona.name}
🔑 Slug: {persona.slug}
📂 Domínios: {persona.domains}
🗣️ Tom de voz: {persona.voice_tone}
🏷️ Keywords: {persona.trigger_keywords}
🔧 Skills ClawHub: {persona.clawhub_skills ou "nenhuma"}
⚖️ Peso base: {persona.base_weight}

Confirma a criação? (sim/não — ou me diga o que ajustar)
```

- Se "não" ou pedido de ajuste: perguntar qual passo deseja revisar e retornar ao passo correspondente.
- Se "sim": executar a geração (ver seção abaixo) e instalar as skills confirmadas.

---

### Geração do SOUL.md

Após confirmação, criar o arquivo `personas/{slug}/SOUL.md`:

**Caminho:** `personas/{persona.slug}/SOUL.md`

**Conteúdo:**

```yaml
---
name: "{persona.name}"
slug: "{persona.slug}"
base_weight: {persona.base_weight}
current_weight: {persona.base_weight}
domains: {persona.domains}
trigger_keywords: {persona.trigger_keywords}
clawhub_skills: {persona.clawhub_skills}
voice_tone: "{persona.voice_tone}"
created_at: "{ISO 8601 timestamp}"
---

Você é a persona {persona.name}.

Seu domínio abrange: {persona.domains em texto natural}.
Tom de voz: {persona.voice_tone}.

Foque em ajudar com tarefas, decisões e informações relacionadas ao seu domínio.
Mantenha consistência com o tom de voz definido em todas as respostas.
```

Após criar o arquivo, informar o usuário:

```
✅ Persona "{persona.name}" criada com sucesso!

Arquivo gerado: personas/{slug}/SOUL.md
{se skills instaladas: Skills instaladas: {lista}}

Esta persona será ativada automaticamente quando você mencionar: {trigger_keywords}
```

Atualizar `USER.md` adicionando a nova persona à lista `personas`.

---

### Geração do slug

Regras para gerar o `slug` a partir do `persona.name`:

1. Converter para lowercase
2. Remover acentos e caracteres especiais (normalização Unicode NFKD)
3. Substituir espaços e separadores por hífen
4. Remover caracteres que não sejam letras, números ou hífens
5. Remover hífens duplicados
6. Remover hífens no início e no fim

Exemplos:
- `"Tech Lead"` → `tech-lead`
- `"Saúde & Bem-estar"` → `saude-bem-estar`
- `"Pai de Família"` → `pai-de-familia`
- `"Dev/Arquiteto"` → `dev-arquiteto`

**Verificação de unicidade:** se já existir uma pasta `personas/{slug}/`, adicionar sufixo numérico (`-2`, `-3`, etc.).

---

## Parte 2 — Frontmatter YAML Obrigatório do SOUL.md

Todo arquivo `personas/{slug}/SOUL.md` gerado por este skill **deve** conter os seguintes campos no frontmatter YAML:

| Campo | Tipo | Descrição |
|---|---|---|
| `name` | `string` | Nome legível da persona (ex: `"Tech Lead"`) |
| `slug` | `string` | Identificador único em kebab-case (ex: `"tech-lead"`) |
| `base_weight` | `float` | Peso fixo definido na criação, intervalo [0.0, 1.0] |
| `current_weight` | `float` | Peso dinâmico atual; inicializado igual ao `base_weight` |
| `domains` | `list[str]` | Áreas de conhecimento e atuação da persona |
| `trigger_keywords` | `list[str]` | Palavras-chave que ativam esta persona no matching |
| `clawhub_skills` | `list[str]` | Skills do ClawHub instalados para esta persona |

Campos adicionais opcionais (não obrigatórios, mas recomendados):

| Campo | Tipo | Descrição |
|---|---|---|
| `voice_tone` | `string` | Descrição do tom de voz |
| `active_hours` | `object` | Janela de horário preferencial (ver Parte 3) |
| `created_at` | `string` | Timestamp ISO 8601 de criação |

**Exemplo completo de frontmatter válido:**

```yaml
---
name: "Tech Lead"
slug: "tech-lead"
base_weight: 0.9
current_weight: 0.9
domains:
  - code
  - architecture
  - team
trigger_keywords:
  - PR
  - review
  - deploy
  - bug
  - arquitetura
  - refactor
  - código
clawhub_skills:
  - github-integration
  - code-review-helper
  - jira-tasks
voice_tone: "técnico, direto, com exemplos de código quando relevante"
active_hours:
  start: "09:00"
  end: "18:00"
  timezone: "America/Sao_Paulo"
created_at: "2025-01-15T10:30:00-03:00"
---
```

---

## Parte 3 — Algoritmo de Matching

O matching é executado pelo orquestrador a cada mensagem recebida, antes de formular a resposta.

### Entradas

- `message`: texto da mensagem recebida
- `personas`: lista de todas as personas ativas (lidas de `USER.md` + respectivos `SOUL.md`)
- `current_time`: horário atual (para matching por hora do dia)

### Saída

- `active_personas`: lista de personas ativas para esta mensagem, cada uma com seu `current_weight`
- Se lista vazia após matching: aplicar fallback (ver Passo 4)

---

### Passo 1 — Keyword Matching

Para cada persona, verificar se alguma das suas `trigger_keywords` aparece na mensagem.

**Regras:**
- Comparação case-insensitive
- Matching parcial é válido: keyword `"deploy"` ativa se a mensagem contém `"deployar"` ou `"deployed"`
- Stemming básico: remover sufixos comuns antes de comparar (opcional, melhora recall)

**Resultado:** lista de personas com pelo menos 1 keyword correspondente → `keyword_matches: list[Persona]`

---

### Passo 2 — Análise Semântica

Para personas que **não** foram capturadas pelo keyword matching, avaliar se o contexto semântico da mensagem é relevante para o domínio da persona.

**Como avaliar:**
- Comparar o conteúdo da mensagem com os `domains` da persona
- Usar o LLM para classificar a relevância semântica (score 0.0–1.0)
- Threshold mínimo para ativação semântica: `0.6`

**Resultado:** lista adicional de personas ativadas semanticamente → `semantic_matches: list[Persona]`

Combinar: `candidates = keyword_matches ∪ semantic_matches`

---

### Passo 3 — Filtro por Hora do Dia (se configurado)

Para cada persona em `candidates`, verificar se possui `active_hours` configurado no SOUL.md.

**Se `active_hours` está definido:**
- Converter `current_time` para o timezone da persona
- Se o horário atual estiver **fora** da janela `active_hours.start`–`active_hours.end`:
  - Reduzir o `current_weight` da persona em 30% para este matching
  - Não remover da lista — apenas penalizar o peso

**Se `active_hours` não está definido:** nenhum ajuste de peso por horário.

---

### Passo 4 — Ativação Simultânea de Múltiplas Personas

Todas as personas em `candidates` são ativadas **simultaneamente**.

Cada persona ativa contribui com seu `current_weight` (ajustado pelo filtro de horário se aplicável).

**Não há limite de personas simultâneas** — se 3 personas têm keywords correspondentes, as 3 são ativadas.

O orquestrador usa os `current_weight` para ponderar a influência de cada persona na resposta final.

**Resultado:** `active_personas = candidates` com seus respectivos `current_weight`

---

### Passo 5 — Fallback

**Condição de fallback:** `candidates` está vazio após os Passos 1, 2 e 3.

**Comportamento:**
1. Selecionar a persona com o maior `current_weight` entre todas as personas ativas
2. Em caso de empate: selecionar a persona com maior `base_weight`
3. Em caso de empate persistente: selecionar a persona criada mais recentemente (`created_at`)

**Resultado:** `active_personas = [persona_com_maior_peso]`

O fallback garante que sempre haverá pelo menos uma persona ativa para responder.

---

### Pseudocódigo do Algoritmo Completo

```python
def match_personas(message: str, personas: list[Persona], current_time: datetime) -> list[ActivePersona]:
    # Passo 1: keyword matching
    keyword_matches = [
        p for p in personas
        if any(kw.lower() in message.lower() for kw in p.trigger_keywords)
    ]

    # Passo 2: semantic matching para personas não capturadas por keywords
    remaining = [p for p in personas if p not in keyword_matches]
    semantic_matches = [
        p for p in remaining
        if semantic_relevance(message, p.domains) >= 0.6
    ]

    candidates = keyword_matches + semantic_matches

    # Passo 3: ajuste de peso por hora do dia
    active_personas = []
    for persona in candidates:
        weight = persona.current_weight
        if persona.active_hours:
            if not is_within_active_hours(current_time, persona.active_hours):
                weight = weight * 0.7  # penalidade de 30%
        active_personas.append(ActivePersona(persona=persona, weight=weight))

    # Passo 4: retornar todas as personas ativas simultaneamente
    if active_personas:
        return active_personas

    # Passo 5: fallback — persona com maior current_weight
    fallback = max(
        personas,
        key=lambda p: (p.current_weight, p.base_weight, p.created_at)
    )
    return [ActivePersona(persona=fallback, weight=fallback.current_weight)]
```

---

## Parte 4 — Edição de Persona

### Quando acionar

- Usuário envia `/editar-persona` ou solicita editar uma persona existente

### Fluxo

1. Listar personas existentes para o usuário escolher
2. Perguntar qual campo deseja editar (nome, domínios, tom de voz, keywords, skills, peso base)
3. Conduzir o passo correspondente do fluxo de criação para o campo escolhido
4. Confirmar a alteração
5. Atualizar o `personas/{slug}/SOUL.md` com o novo valor
6. Se o nome foi alterado: gerar novo slug, criar nova pasta, mover arquivos, atualizar `USER.md`

---

## Edge Cases

### Edge Case A — Slug já existe

**Situação:** Usuário tenta criar uma persona com nome que gera um slug já existente.
(Ex: já existe `personas/tech-lead/` e o usuário quer criar "Tech Lead 2")

**Comportamento:**
- Gerar slug com sufixo: `tech-lead-2`
- Informar o usuário: `"Já existe uma persona com slug 'tech-lead'. A nova persona será criada como 'tech-lead-2'."`
- Prosseguir normalmente com o slug ajustado

---

### Edge Case B — Nenhuma persona cadastrada (fallback impossível)

**Situação:** O algoritmo de matching tenta aplicar o fallback, mas não há nenhuma persona cadastrada.

**Comportamento:**
- Responder com a personalidade base do Bastion (SOUL.md raiz)
- Sugerir ao usuário criar sua primeira persona: `"Você ainda não tem personas configuradas. Digite /nova-persona para criar sua primeira persona."`

---

### Edge Case C — Keyword muito genérica

**Situação:** O usuário define uma keyword extremamente genérica (ex: "a", "o", "de", "e") que ativaria a persona em praticamente toda mensagem.

**Comportamento:**
- Detectar keywords com menos de 3 caracteres ou que sejam stopwords comuns em português/inglês
- Avisar o usuário: `"A keyword '{kw}' é muito genérica e pode ativar esta persona em quase todas as mensagens. Deseja mantê-la mesmo assim? (sim/não)"`
- Se "sim": aceitar e registrar o aviso no SOUL.md como comentário
- Se "não": remover a keyword e pedir uma substituta

---

### Edge Case D — Peso base fora do intervalo

**Situação:** Usuário digita um valor inválido para o peso base (ex: "1.5", "-0.1", "alto").

**Comportamento:**

```
O peso base deve ser um número entre 0.0 e 1.0.

• 0.9 = alta prioridade
• 0.5 = prioridade média
• 0.2 = baixa prioridade

Digite um valor válido:
```

Repetir até receber um valor válido.

---

### Edge Case E — Múltiplas personas com mesmo peso no fallback

**Situação:** Duas ou mais personas têm exatamente o mesmo `current_weight` e `base_weight` no momento do fallback.

**Comportamento:**
- Usar `created_at` como critério de desempate final: persona mais recente tem prioridade
- Se `created_at` também for igual (improvável): usar ordem alfabética do slug

---

### Edge Case F — Persona sem keywords ativada apenas semanticamente

**Situação:** Uma persona tem `trigger_keywords: []` (lista vazia) e só pode ser ativada por análise semântica.

**Comportamento:**
- Permitir a criação (keywords vazias são válidas)
- Avisar o usuário durante a criação: `"Esta persona não tem keywords definidas e só será ativada por análise semântica. Isso pode resultar em ativações menos precisas."`
- No matching, pular o Passo 1 para esta persona e ir direto ao Passo 2

---

## Output Example

```json
{
  "name": "Tech Lead",
  "slug": "tech-lead",
  "base_weight": 0.9,
  "current_weight": 0.9,
  "domains": ["code", "architecture", "team"],
  "trigger_keywords": ["PR", "review", "deploy", "bug", "arquitetura"],
  "clawhub_skills": ["github-integration", "code-review-helper"],
  "voice_tone": "técnico, direto, com exemplos de código quando relevante",
  "created_at": "2024-01-15T10:30:00Z"
}
```

---

## Notas de Implementação

- O `current_weight` é inicializado igual ao `base_weight` na criação e gerenciado pelo skill `bastion/weight-system` após isso. O `persona-engine` não altera `current_weight` diretamente — apenas lê para matching e fallback.
- O matching é executado pelo orquestrador antes de cada resposta. O resultado (`active_personas`) é injetado no contexto da resposta.
- Personas criadas durante o onboarding (`bastion/onboarding`) seguem o mesmo formato de SOUL.md definido aqui. O onboarding chama este skill internamente para garantir consistência.
- O arquivo `USER.md` deve ser atualizado sempre que uma persona é criada, editada ou removida — mantendo a lista `personas` sincronizada com as pastas em `personas/`.
- Skills do ClawHub instaladas para uma persona são registradas em `personas/{slug}/skills.json` além do frontmatter do SOUL.md.
