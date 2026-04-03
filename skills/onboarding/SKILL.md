---
name: bastion/onboarding
version: 1.0.0
description: >
  Fluxo de configuração inicial guiado do Bastion. Coleta informações do usuário,
  cria personas automaticamente, configura pesos iniciais, realiza setup TOTP e
  gera o arquivo USER.md com o perfil completo.
triggers:
  - "/start"
  - primeira mensagem de um usuário sem USER.md configurado
---

# Skill: bastion/onboarding

## Objetivo

Guiar o novo usuário por um fluxo de configuração inicial completo, ao final do qual o Bastion estará pronto para uso com personas criadas, pesos configurados e autenticação TOTP ativa.

Este skill é acionado automaticamente quando:
- O usuário envia `/start`
- O usuário envia qualquer mensagem e `USER.md` ainda não possui `totp_configured: true`

---

## Fluxo Principal

### Etapa 1 — Boas-vindas e coleta de nome

Enviar ao usuário:

```
👋 Olá! Sou o Bastion, seu Life OS pessoal.

Vou te guiar por uma configuração rápida para que eu possa te ajudar da melhor forma.

Primeiro: qual é o seu nome?
```

Aguardar resposta. Armazenar como `user.name`.

**Validação:** nome não pode ser vazio. Se vazio, repetir a pergunta.

---

### Etapa 2 — Trabalho principal

Enviar:

```
Prazer, {user.name}! 🙌

O que você faz? Me conta seu trabalho ou ocupação principal.
(Ex: "Desenvolvedor de software", "Empreendedor", "Designer UX", "Estudante de medicina")
```

Aguardar resposta. Armazenar como `user.occupation`.

---

### Etapa 3 — Negócio ou empreendimento

Enviar:

```
Você tem um negócio próprio ou empreendimento além do seu trabalho principal?
(Responda "sim" ou "não", ou descreva brevemente se quiser)
```

Aguardar resposta. Armazenar como `user.has_business` (boolean) e `user.business_description` (string, opcional).

---

### Etapa 4 — Áreas de vida

Enviar:

```
Quais áreas da sua vida você quer que eu ajude a gerenciar?

Exemplos de áreas:
• Trabalho / Carreira
• Negócio / Empreendimento
• Saúde e bem-estar
• Família
• Finanças pessoais
• Estudos / Aprendizado
• Projetos pessoais
• Relacionamentos

Liste as áreas que fazem sentido para você, uma por linha ou separadas por vírgula.
```

Aguardar resposta. Parsear a lista de áreas informadas. Armazenar como `user.life_areas: list[str]`.

**Validações (ver Edge Cases):**
- Se a lista estiver vazia → repetir a pergunta (Edge Case A)
- Se houver áreas duplicadas → deduplicar silenciosamente (Edge Case B)

Confirmar com o usuário antes de prosseguir:

```
Entendido! Vou criar personas para estas áreas:

{lista numerada das áreas}

Está correto? (sim/não — ou me diga o que ajustar)
```

Se o usuário pedir ajuste, voltar à coleta de áreas. Se confirmar, avançar.

---

### Etapa 5 — Criação automática de personas

Para **cada área** em `user.life_areas`, criar uma persona automaticamente:

1. Gerar `slug` a partir do nome da área (lowercase, hífens, sem acentos).
   - Ex: "Saúde e bem-estar" → `saude-bem-estar`
   - Ex: "Negócio / Empreendimento" → `negocio-empreendimento`

2. Inferir `base_weight` inicial com base na ordem de prioridade implícita:
   - Primeira área informada: `0.8`
   - Segunda área: `0.7`
   - Terceira em diante: `0.6`
   - Mínimo: `0.5`

3. Inferir `domains`, `trigger_keywords` e `clawhub_skills` sugeridos com base no nome da área (ver tabela de inferência abaixo).

4. Criar o arquivo `personas/{slug}/SOUL.md` com o frontmatter YAML completo.

**Tabela de inferência por área (exemplos):**

| Área (contém) | domains | trigger_keywords | clawhub_skills sugeridos |
|---|---|---|---|
| trabalho / carreira | `["work", "career"]` | `["reunião", "tarefa", "projeto", "deadline", "entrega"]` | `google-calendar`, `notion-tasks` |
| negócio / empreendimento | `["business", "entrepreneurship"]` | `["cliente", "venda", "receita", "produto", "startup"]` | `google-calendar`, `notion-tasks`, `web-search` |
| saúde / bem-estar | `["health", "wellness"]` | `["treino", "dieta", "sono", "médico", "exercício"]` | `web-search` |
| família | `["family"]` | `["família", "filho", "cônjuge", "casa", "compromisso familiar"]` | `google-calendar` |
| finanças | `["finance", "money"]` | `["gasto", "investimento", "conta", "orçamento", "dinheiro"]` | `web-search` |
| estudos / aprendizado | `["learning", "education"]` | `["estudo", "curso", "livro", "aula", "prova"]` | `web-search`, `notion-tasks` |
| projetos pessoais | `["personal-projects"]` | `["projeto", "ideia", "hobby", "criação"]` | `github-integration`, `notion-tasks` |
| relacionamentos | `["relationships"]` | `["amigo", "relacionamento", "social", "encontro"]` | `google-calendar` |

Para áreas não mapeadas, usar `domains: ["{slug}"]`, `trigger_keywords: ["{nome da área}"]`, `clawhub_skills: []`.

**Formato do `personas/{slug}/SOUL.md`:**

```yaml
---
name: "{Nome da Área}"
slug: "{slug}"
base_weight: {valor}
current_weight: {mesmo valor do base_weight}
domains: [...]
trigger_keywords: [...]
clawhub_skills: [...]
---

Você é a persona {Nome da Área} de {user.name}.

Seu domínio é {descrição do domínio}. Responda de forma {tom de voz padrão: direto e prático}.
Foque em ajudar com tarefas, decisões e informações relacionadas a {domínio}.
```

Informar o usuário sobre o progresso:

```
✅ Criando suas personas...

{para cada persona criada}
• {Nome da Área} ({slug}) — criada
```

---

### Etapa 6 — Sugestão e instalação de skills do ClawHub

Para cada persona criada que tenha `clawhub_skills` não vazio:

```
Para a persona "{Nome da Área}", sugiro instalar estas skills do ClawHub:

{lista de skills sugeridas com descrição breve}

Deseja instalar? (sim / não / escolher)
```

- Se "sim": instalar todas as skills listadas para essa persona (verificar badge Verified + rating ≥ 4.0 + 50+ reviews conforme política de segurança do AGENTS.md).
- Se "não": pular, registrar `clawhub_skills: []` no SOUL.md da persona.
- Se "escolher": listar cada skill individualmente para confirmação.

Repetir para cada persona com sugestões pendentes.

---

### Etapa 7 — Configuração de pesos iniciais

Exibir resumo dos pesos inferidos:

```
Configurei os pesos iniciais das suas personas com base na ordem que você informou:

{lista: Nome da Área → peso}

Os pesos determinam qual persona tem prioridade quando você me enviar uma mensagem.
Você pode ajustar agora ou a qualquer momento com o comando /personas.

Quer ajustar algum peso? (sim/não)
```

- Se "sim": para cada persona, perguntar o novo peso (0.0 a 1.0). Validar que está no intervalo.
- Se "não": manter os pesos inferidos.

Persistir os pesos em `USER.md` e nos respectivos `personas/{slug}/SOUL.md`.

---

### Etapa 8 — Setup TOTP

```
🔐 Agora vamos configurar sua autenticação de dois fatores.

Isso garante que só você consiga usar o Bastion, mesmo que alguém tenha acesso ao seu Telegram/WhatsApp.

Você precisará do app Authy (ou Google Authenticator) no seu celular.

Pronto para continuar? (sim/não)
```

Se "não": informar que o TOTP pode ser configurado depois com `/setup-totp`, mas que o Bastion ficará sem autenticação até lá. Prosseguir para Etapa 9.

Se "sim":

1. Gerar TOTP secret via `pyotp.random_base32()`.
2. Gerar URI para QR code: `pyotp.TOTP(secret).provisioning_uri(name=user.name, issuer_name="Bastion")`.
3. Renderizar o QR code (como imagem ou link para geração de QR).
4. Enviar ao usuário:

```
📱 Escaneie o QR code abaixo com o Authy:

{QR code}

Ou adicione manualmente com a chave: {secret}

Após escanear, digite o código de 6 dígitos que aparecer no app:
```

5. Aguardar o código de 6 dígitos do usuário.
6. Validar com `pyotp.TOTP(secret).verify(code)`.
   - **Válido:** prosseguir para Etapa 9.
   - **Inválido:** ver Edge Case C.

7. Salvar o secret **apenas** na variável de ambiente `BASTION_TOTP_SECRET` no arquivo `.env`. Nunca em USER.md ou em qualquer arquivo versionado.

---

### Etapa 9 — Geração do USER.md

Gerar o arquivo `USER.md` com o perfil completo:

```yaml
---
name: "{user.name}"
occupation: "{user.occupation}"
has_business: {true/false}
business_description: "{user.business_description ou ''}"
authorized_user_ids:
  - "{telegram_user_id ou whatsapp_user_id do usuário atual}"
totp_configured: {true/false}
personas:
{para cada persona}
  - slug: "{slug}"
    name: "{Nome da Área}"
    base_weight: {valor}
    current_weight: {valor}
onboarding_completed_at: "{ISO 8601 timestamp}"
---

# Perfil de {user.name}

**Ocupação:** {user.occupation}
{se has_business: **Negócio:** {business_description}}

## Personas Ativas

{lista de personas com nome, slug e peso}

## Configuração

- TOTP: {configurado / não configurado}
- Onboarding concluído em: {data}
```

---

### Etapa 10 — Mensagem de conclusão

```
🎉 Tudo pronto, {user.name}!

Aqui estão suas personas:

{lista numerada: Nome da Área (slug) — peso atual}

A partir de agora, responderei de acordo com o contexto de cada área da sua vida.

Alguns comandos úteis:
• /personas — ver e editar suas personas
• /pesos — ajustar pesos de prioridade
• /crise — ativar modo crise para replanejamento urgente
• /connect-app — conectar o app mobile
• /help — ver todos os comandos disponíveis

Como posso te ajudar hoje?
```

---

## Edge Cases

### Edge Case A — Usuário informa 0 áreas de vida

**Situação:** O usuário responde à Etapa 4 com uma mensagem vazia, apenas espaços, ou com conteúdo que não contém nenhuma área identificável.

**Comportamento:**

```
Hmm, não consegui identificar nenhuma área. 🤔

Para que eu possa criar suas personas, preciso de pelo menos uma área da sua vida que você queira gerenciar.

Por exemplo: "Trabalho, Saúde, Família" ou apenas "Trabalho".

Quais áreas você quer que eu ajude?
```

Repetir a pergunta até receber pelo menos 1 área válida. Não avançar para a Etapa 5 com lista vazia.

---

### Edge Case B — Área duplicada

**Situação:** O usuário informa a mesma área mais de uma vez (ex: "Trabalho, trabalho, Saúde").

**Comportamento:** Deduplicar silenciosamente antes de exibir a lista de confirmação. Não informar o usuário sobre a duplicata — apenas mostrar a lista já deduplicada na confirmação da Etapa 4.

Critério de deduplicação: comparação case-insensitive após normalização (remover acentos, trim).

Exemplo:
- Input: `"Trabalho, TRABALHO, Saúde, saúde e bem-estar"`
- Após dedup: `["Trabalho", "Saúde e bem-estar"]`

Se duas áreas forem semanticamente similares mas textualmente diferentes (ex: "Saúde" e "Bem-estar"), **não** deduplicar — criar personas separadas. A deduplicação é apenas textual.

---

### Edge Case C — Usuário não confirma o TOTP (código inválido ou ausente)

**Situação:** O usuário digita um código TOTP incorreto na Etapa 8.

**Comportamento:**

```
❌ Código incorreto. Vamos tentar novamente.

Certifique-se de que o app Authy está sincronizado e use o código atual (ele muda a cada 30 segundos).

📱 Escaneie o QR code novamente:

{QR code — mesmo secret, nova renderização}

Digite o código de 6 dígitos:
```

Repetir indefinidamente até o usuário confirmar com sucesso ou digitar `/cancelar`.

Se o usuário digitar `/cancelar`:

```
Ok! O TOTP não foi configurado. Você pode configurar depois com /setup-totp.

⚠️ Atenção: sem TOTP, qualquer pessoa com acesso ao seu Telegram/WhatsApp poderá usar o Bastion.
```

Prosseguir para Etapa 9 com `totp_configured: false`.

**Não há limite de tentativas durante o onboarding** — o usuário pode tentar quantas vezes precisar. O limite de tentativas (`BASTION_MAX_AUTH_ATTEMPTS`) se aplica apenas às sessões normais de autenticação, não ao setup inicial.

---

## Notas de Implementação

- O secret TOTP gerado deve ser salvo **exclusivamente** em `.env` como `BASTION_TOTP_SECRET=<valor>`. Nunca em USER.md, nunca em arquivo versionado.
- O `authorized_user_ids` em USER.md deve ser preenchido com o ID real do usuário no canal (Telegram user_id, WhatsApp number, etc.) capturado automaticamente da sessão atual.
- Todos os arquivos `personas/{slug}/SOUL.md` devem ser criados antes de avançar para a Etapa 6.
- O onboarding é idempotente: se interrompido e reiniciado, deve detectar o estado atual (quais etapas já foram concluídas) e retomar a partir da etapa pendente.
- Após o onboarding, o skill `bastion/persona-engine` assume o gerenciamento contínuo de personas.
