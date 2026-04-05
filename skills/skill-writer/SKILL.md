---
name: bastion/skill-writer
version: "2.0.0"
description: >
  Guia o usuário na criação de novas skills personalizadas (SKILL.md).
  Verifica o ClawHub antes de criar, escreve o arquivo com estrutura obrigatória,
  salva no caminho correto conforme o escopo e testa com um caso de uso real.
  Também busca e instala skills do repositório awesome-openclaw-skills para personas,
  aplicando política de qualidade e scan de segurança Sage antes de cada instalação.
triggers:
  - "criar skill"
  - "nova skill"
  - "escrever skill"
  - "quero ensinar"
  - "novo comportamento"
  - "skill personalizada"
  - "/skill-writer"
  - "configurar skills para persona"
  - "skills para persona"
  - "instalar skills"
  - "/skills-persona"
---

# Skill Writer — Roteiro de Criação de Skills

## Objetivo

Guiar o usuário na criação de uma nova skill (SKILL.md) de forma conversacional,
garantindo que o arquivo gerado tenha estrutura completa e seja salvo no caminho correto.

---

## Fluxo Conversacional

### Passo 1 — Entender a Necessidade

Faça as três perguntas abaixo, uma de cada vez, aguardando a resposta antes de prosseguir:

1. **O quê**: "O que você quer que essa skill faça? Descreva o comportamento esperado."
2. **Quando**: "Quando ela deve ser ativada? Quais palavras-chave ou situações disparam esse comportamento?"
3. **Output**: "Qual é o resultado esperado? O que o agente deve entregar ao final?"

Registre as respostas como:
- `skill_purpose`: o que a skill faz
- `skill_triggers`: quando é ativada (lista de keywords/frases)
- `skill_output`: o que entrega ao final

---

### Passo 2 — Verificar o ClawHub

Antes de criar uma skill nova, verifique se já existe uma equivalente no ClawHub:

1. Busque no ClawHub por skills com propósito similar ao `skill_purpose`
2. Se encontrar uma skill equivalente → vá para o **Passo 3a**
3. Se não encontrar → vá para o **Passo 3b**

---

### Passo 3a — Skill Equivalente Existe no ClawHub

Se uma skill equivalente já existe no ClawHub:

1. Apresente a skill encontrada ao usuário:
   - Nome, descrição, avaliação, número de reviews, badge Verified
2. Sugira a instalação em vez de criar uma nova:
   > "Encontrei a skill `{nome}` no ClawHub que faz exatamente isso (⭐ {rating} · {reviews} reviews · Verified).
   > Quer que eu instale ela em vez de criar uma nova?"
3. Se o usuário confirmar → instale a skill seguindo a política de instalação do AGENTS.md
4. Se o usuário preferir criar mesmo assim → continue para o **Passo 3b**

---

### Passo 3b — Criar Nova Skill

Se não existe equivalente no ClawHub (ou o usuário preferiu criar):

#### 3b.1 — Definir o Escopo

Pergunte ao usuário:
> "Essa skill é para uso exclusivo de uma persona específica ou para todo o Bastion?"

- **Privada** (persona específica): salvar em `personas/{slug}/SKILL.md`
- **Global** (todo o Bastion): salvar em `skills/{nome}/SKILL.md`

Se o usuário não souber o escopo:
> "Se a skill só faz sentido para a persona '{persona_ativa}', é privada.
> Se qualquer persona pode usar, é global. Qual prefere?"

#### 3b.2 — Gerar o SKILL.md

Monte o arquivo com a estrutura obrigatória:

```markdown
---
name: {namespace}/{slug}
version: "1.0.0"
description: >
  {skill_purpose}
triggers:
  - {trigger_1}
  - {trigger_2}
  ...
---

# {Nome da Skill}

## Objetivo

{skill_purpose}

## Instruções Passo a Passo

1. {passo_1}
2. {passo_2}
3. {passo_3}
...

## Exemplos de Uso

### Exemplo 1 — {cenário}

**Input do usuário:** "{exemplo_input}"

**Comportamento esperado:**
{exemplo_output}

### Exemplo 2 — {cenário_2}

**Input do usuário:** "{exemplo_input_2}"

**Comportamento esperado:**
{exemplo_output_2}

## Edge Cases

- **Skill já existe localmente**: Se já existe um arquivo no caminho de destino, perguntar ao usuário se deseja sobrescrever ou criar uma versão nova (ex: `v2`).
- **Usuário não sabe o escopo**: Explicar a diferença entre privada e global e sugerir com base no contexto da conversa.
- **Skill com mesmo nome no ClawHub**: Avisar que o nome já existe no ClawHub e sugerir um nome alternativo para evitar conflito futuro.
- **Triggers muito genéricos**: Se os triggers forem palavras muito comuns (ex: "ok", "sim"), alertar que podem causar ativações indesejadas e sugerir triggers mais específicos.
- **Output indefinido**: Se o usuário não souber descrever o output, fazer perguntas de clarificação antes de prosseguir.
```

**Regras de nomenclatura:**
- Para skills privadas: `name: personas/{slug}/{skill-slug}`
- Para skills globais: `name: bastion/{skill-slug}` ou `name: {namespace}/{skill-slug}`
- O `slug` usa apenas letras minúsculas, números e hífens

#### 3b.3 — Salvar no Caminho Correto

**Regra de caminho (obrigatória):**

| Escopo | Caminho |
|--------|---------|
| Privada (persona específica) | `personas/{slug}/SKILL.md` |
| Global (todo o Bastion) | `skills/{nome}/SKILL.md` |

Onde:
- `{slug}` é o slug da persona (ex: `tech-lead`, `empreendedor`)
- `{nome}` é o nome da skill em kebab-case (ex: `weekly-review`, `code-reviewer`)

Antes de salvar, confirme com o usuário:
> "Vou salvar a skill em `{caminho}`. Confirma? (sim/não)"

---

### Passo 4 — Testar com Caso de Uso Real

Após salvar o arquivo, acione a skill com um caso de uso real:

1. Peça ao usuário um exemplo concreto de uso:
   > "Para validar a skill, me dê um exemplo real de mensagem que deveria ativá-la."
2. Execute o fluxo da skill com esse input
3. Apresente o resultado ao usuário:
   > "A skill foi ativada com o input '{input}' e produziu: {output}"
4. Pergunte se o resultado está correto:
   > "O resultado está como esperado? (sim/não/ajustar)"
5. Se não estiver correto → ajuste o SKILL.md e repita o teste

---

### Passo 5 — Publicar no ClawHub (Opcional)

Após validação bem-sucedida, pergunte:
> "Essa skill pode ser útil para outros usuários do Bastion? Quer publicá-la no ClawHub?"

Se o usuário confirmar:
1. Verifique se o nome não conflita com skills existentes no ClawHub
2. Guie o processo de publicação:
   - Adicionar `author`, `license` e `repository` ao frontmatter
   - Criar `README.md` com documentação pública
   - Submeter via `clawhub publish {caminho}`
3. Informe que a skill passará por revisão antes de aparecer no marketplace

---

## Edge Cases Globais

### Skill já existe localmente

Se já existe um arquivo no caminho de destino:
> "Já existe uma skill em `{caminho}`. Quer sobrescrever, criar `{nome}-v2` ou cancelar?"

### Usuário não sabe o escopo

Se o usuário não conseguir definir se a skill é privada ou global:
1. Explique: "Skills privadas ficam dentro da pasta da persona e só ela usa. Skills globais ficam em `skills/` e qualquer persona pode usar."
2. Sugira com base no contexto: se a skill usa keywords muito específicas da persona ativa, provavelmente é privada.

### Skill com mesmo nome no ClawHub

Se o nome escolhido já existe no ClawHub:
> "O nome `{nome}` já existe no ClawHub. Para evitar conflito futuro, sugiro usar `{sugestão}`. Aceita?"

### Triggers muito genéricos

Se os triggers incluírem palavras muito comuns:
> "O trigger '{trigger}' é muito genérico e pode ativar a skill em contextos indesejados. Sugiro usar '{sugestão_mais_específica}'. Quer ajustar?"

### Usuário quer criar skill para outra persona

Se o usuário quiser criar uma skill privada para uma persona diferente da ativa:
1. Confirme o slug da persona de destino
2. Salve em `personas/{slug-destino}/SKILL.md`
3. Informe que a skill só estará disponível quando essa persona estiver ativa
