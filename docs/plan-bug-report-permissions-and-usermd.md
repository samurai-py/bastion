# Plano de Implementação: Correção de Permissões e Refatoração do Onboarding

Este documento descreve os requisitos e o plano de ação passo-a-passo para resolver os problemas mapeados em `docs/bug-report-permissions-and-usermd.md`. Este plano será executado pelo agente de código (Claude Code / Gemini).

## 1. Requisitos

### 1.1. Permissões de Arquivos e Diretórios (Sem Sudo)
- O sistema de inicialização (`installer.sh` e/ou `docker-compose.yml`) deve garantir que os diretórios mapeados no host (`config/`, `personas/`, etc.) pertençam ao usuário não-root correto antes do container iniciar.
- Remover ou substituir a necessidade de `sudo chown` no `installer.sh`, pois isso quebra a automação e fluxos de CI/CD.
- Se o diretório for criado pelo Docker em bind mounts, ele nasce como `root`. A solução é garantir a criação pelo usuário atual com `mkdir -p` **antes** de qualquer comando Docker.

### 1.2. Schema e Versionamento do `USER.md`
- **Novo Schema do USER.md**:
  - Adicionar o campo `user_bio` (Resumo do Usuário / Bio).
  - Adicionar o campo `pain_points_and_goals` (Maiores dores e objetivos).
- Garantir que o `USER.md` seja ignorado pelo git e removido do cache se já estiver sendo rastreado, para que alterações locais do bot não subam para o repositório.

### 1.3. Melhorias no Skill de Onboarding
- **Normalização de Slugs de Personas**:
  - Personas geradas devem possuir um `slug` curto, focado na área da vida (ex: em vez de `saude-faco-academia...`, usar apenas `saude`).
- **Novo Fluxo Conversacional de Onboarding**:
  1. Perguntar ao usuário um resumo sobre si mesmo (Bio). Etapa 3-2 do Onboarding
  2. Perguntar quais são suas maiores dores, desafios e objetivos. Etapa 3-3 do Onboarding
  3. Com essas informações, guiar a criação das personas. Que é a etapa 4
  4. Ao detalhar cada persona, perguntar especificamente *o que o usuário quer que a persona faça*, quais *skills* ela deve ter, qual o **estado atual** do usuário naquela área e quais os **objetivos específicos** para ela. Estas informações devem ser salvas usando protocolos estruturados (ex: YAML no frontmatter ou `SOUL.md`) para permitir acompanhamento e comparação futura.
  5. Após configurar as personas, o fluxo deve perguntar sobre a **Identidade principal do Bot** (nome preferido, comportamento base) e salvar isso no `IDENTITY.md`, com o entendimento de que as personas sobrescrevem a identidade primária quando ativas.

### 1.4. Configuração de Timezone
- O bot precisa ter uma configuração global de `timezone` (fuso horário) que será seguida em todas as operações (especialmente calendário e agenda).
- Essa configuração deve poder ser resgatada facilmente pelas skills.
- **Implementação Flexível**: O agente pode escolher a melhor abordagem para configurar isso (ex: detectar e definir no `installer.sh`, perguntar no fluxo de onboarding, ler do `.env`, etc.) e decidir o melhor arquivo para persistir esse dado (ex: `USER.md` ou variáveis de ambiente).

### 1.5. Engine de Localização (i18n) e Idioma
- **Código e Prompts**: O código-fonte das skills, chaves estruturais (YAML/JSON) e *System Prompts* (arquivos `SKILL.md`) devem ser escritos nativamente em **Inglês**.
- **Mensagens Estáticas**: Devem ser extraídas para uma estrutura de localização. Cada skill deverá ter uma pasta `locales/` com arquivos `.json` (ex: `en.json`, `pt-BR.json`) contendo os textos hardcoded que são enviadas proativamente ao usuário (ex: perguntas iniciais do onboarding).
- **Engine Core**: O framework deve prover um utilitário (helper) que lê o campo `language` do `USER.md` e carrega a string correta da pasta `locales/` da skill correspondente (com fallback para inglês).
- **Textos Dinâmicos (LLM) e Prompts (SKILL.md)**:
  - Arquivos Markdown como `SKILL.md` são lidos como strings pelo framework antes de serem enviados à LLM. Logo, o framework pode usar interpolação (ex: `.format(user_language="pt-BR")` no Python) para injetar variáveis.
  - O sistema deve injetar dinamicamente nos prompts (ou no fim deles) a instrução de responder no idioma do usuário (ex: `Always respond and interact with the user in {user_language}`).

---

## 2. Plano de Execução (Passo-a-Passo)

### Fase 1: Correção de Permissões e Infraestrutura
1. **Revisar `installer.sh`**:
   - Modificar o script de instalação para garantir que os comandos `mkdir -p config/workspace`, `mkdir -p config/identity` e `mkdir -p personas` sejam chamados no início do script, com o usuário local (antes de chamar o `docker compose`).
   - Remover as linhas de `sudo chown -R 1000:1000` (ou envolvê-las numa checagem de modo que não parem a execução, ou preferencialmente, focar em `mkdir` prévio).
2. **Revisar `docker-compose.yml`**:
   - Validar se o container do `openclaw` ou equivalente já está rodando com `--user 1000:1000` (ou `user: "${UID}:${GID}"` no compose file) para evitar a criação de arquivos `root` dentro do container, e aplicar caso necessário.
3. **Desvincular `USER.md` do Git**:
   - Executar `git rm --cached USER.md` (e `config/identity/USER.md` se existir) para garantir que o arquivo não seja mais commitado por acidente.

### Fase 2: Atualização do Schema e Fluxo do Onboarding
1. **Modificar os Models/Schemas**:
   - Atualizar a estrutura de dados (provavelmente no `skills/onboarding/` usando Pydantic, classes baseadas em dataclasses, ou lógicas de parse de markdown) do `USER.md` para suportar `user_bio` e `pain_points_and_goals`.
   - Garantir que a estrutura de dados das Personas (metadados ou `SOUL.md`) também seja atualizada para incluir de forma estruturada os campos `current_state` (estado atual) e `specific_goals` (objetivos específicos) de cada persona.
2. **Atualizar o Fluxo de Interação (Onboarding)**:
   - Identificar o arquivo principal do fluxo (ex: `skills/onboarding/totp.py` ou equivalentes que lidam com diálogos iniciais).
   - Inserir a coleta de informações: "Resumo do usuário" e "Maiores dores/objetivos" no início do diálogo.
   - Usar essas respostas como contexto para gerar o `USER.md` inicial.
3. **Implementar a Normalização de Slugs**:
   - Atualizar a função de geração de slugs para usar o LLM (com prompt limitando a 1-3 palavras simples) ou um regex focado. Se o slug gerado passar de 20 caracteres, ele deve ser truncado ou resumido apropriadamente antes de criar os arquivos `personas/{slug}/...`.
4. **Implementar Configuração de Identidade**:
   - Adicionar uma etapa final no fluxo de onboarding onde o agente pergunta como deve ser chamado e como deve agir em modo padrão.
   - Salvar o resultado em `config/identity/IDENTITY.md` ou `IDENTITY.md` (conforme padrão do framework).

### Fase 3: Localização
1. **Criar Engine de i18n no Core**:
   - Desenvolver um utilitário (ex: em `bastion/utils/i18n.py`) contendo uma função que carrega os JSONs de `locales/` da skill correspondente e retorna a string baseado no `language` do `USER.md`.
2. **Refatorar Prompts (SKILL.md e Strings)**:
   - Adicionar variáveis de formatação, como `{user_language}`, no carregamento dos system prompts (ex: leitura do `SKILL.md`).
   - Onde o system prompt é instanciado e enviado ao modelo, incluir: `Always interact and respond to the user in {user_language}` (passando a linguagem atual definida pelo usuário).
3. **Extrair Mensagens Estáticas**:
   - Na skill de onboarding (e outras afetadas), criar a pasta `locales/` com `en.json` e `pt-BR.json` (exemplo).
   - Substituir os textos fixos nos fluxos Python (ex: as perguntas do fluxo de onboarding) por chamadas à função da engine de localização.
4. Os arquivos estáticos atuais (AGENTS.md, SOUL.md e as demais SKILL.md das skills built-in no nosso código, devem ser colocadas em ingles tb se ainda não tiverem sido)

### Fase 4: Validação (com pytest)
1. **Testes do Installer**:
   - Rodar um teste limpando o diretório `config/` e `personas/`, executar o script `installer.sh` e confirmar que não pede senha `sudo` e que os diretórios pertencem ao usuário atual.
2. **Testes do Onboarding (pytest)**:
   - Configurar/Escrever os cenários de teste utilizando **pytest** (ex: adaptando os testes existentes em `skills/onboarding/tests/` ou criando novos cenários simples). Não é necessário criar uma infraestrutura de testes robusta ou usar TestSprite, apenas garanta a cobertura básica dos novos fluxos usando pytest.
   - Simular o fluxo de onboarding, verificando se as perguntas de bio, dores, timezone e identidade estão aparecendo em ordem lógica e se as traduções (`i18n`) estão sendo aplicadas.
   - Verificar as saídas geradas durante o teste:
     - O `USER.md` possui os novos campos populados corretamente? O YAML está formatado perfeitamente sem quebras?
     - O timezone global foi salvo onde especificado?
     - O slug das personas está normalizado (curto e conciso)?
     - Os campos `current_state` e `specific_goals` das personas estão estruturados?
     - O `IDENTITY.md` foi gerado adequadamente?

---

> **Nota para o Agente de Código:**
> Ao iniciar a execução deste plano, siga as fases sequencialmente. Valide cada etapa através de `grep`, `read_file` e executando os testes com **pytest**. Adicione os novos cenários de fluxo utilizando a infraestrutura de testes com pytest presente no repositório. Garanta que a formatação YAML no `USER.md` não misture variáveis usando a biblioteca de YAML apropriada (ex: `ruamel.yaml` ou `yaml` seguro do Python) em vez de simples concatenação de strings para evitar corrompimento de estrutura.