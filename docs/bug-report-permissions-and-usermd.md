# Bug Report: Permissões de Escrita no Container e Corrupção do USER.md

**Data:** 2026-04-05  
**Severidade:** Alta  
**Status:** Parcialmente corrigido (permissões), Aberto (USER.md)  
**Ambiente:** Docker (bastion-openclaw), OpenClaw 2026.4.2, Ubuntu Linux, host uid=1000 (mario)

---

## 1. Problema de Permissões — EACCES em Diretórios do Workspace

### Descrição

O container OpenClaw roda como o usuário `node` (uid 1000). Ao tentar criar ou escrever arquivos em `/home/node/.openclaw/workspace/`, o processo falha com `EACCES: permission denied` porque os diretórios no host pertencem a `root:root`.

### Diretórios afetados

| Caminho no host | Caminho no container | Problema |
|-----------------|----------------------|---------|
| `./config/` | `/home/node/.openclaw/` | `root:root` — OpenClaw não consegue criar `TOOLS.md`, `IDENTITY.md` |
| `./personas/` | `/home/node/.openclaw/workspace/personas/` | `root:root` — onboarding não consegue criar subdiretórios de personas |

### Logs de erro observados

```
[heartbeat] failed: EACCES: permission denied, open '/home/node/.openclaw/workspace/TOOLS.md'
[tools] write failed: EACCES: permission denied, mkdir '/home/node/.openclaw/workspace/personas/carreira-em-tech'
[tools] exec failed: elevated is not available right now (runtime=direct).
```

### Causa raiz

O Docker cria os diretórios de bind mount como `root:root` quando eles não existem no host antes do `docker compose up`. O installer não fazia `chown` desses diretórios antes de subir os containers.

Adicionalmente, o `config/workspace/` era populado com arquivos vazios pertencentes a `root:root` pelo próprio Docker na primeira execução.

### Arquivos afetados que o OpenClaw precisa escrever

- `TOOLS.md` — gerado automaticamente pelo OpenClaw no heartbeat
- `IDENTITY.md` — gerado no bootstrap (nome, emoji, vibe do agente)
- `personas/{slug}/SOUL.md` — criado pelo skill de onboarding
- `personas/{slug}/memory.md` — criado pelo skill de onboarding
- `personas/{slug}/heartbeat-state.md` — criado pelo heartbeat

### Correção aplicada

```bash
# Fix imediato (rodar no host)
sudo chown -R 1000:1000 config/ personas/
```

No `installer.sh`, adicionado `chown` em dois pontos:

1. Na etapa "Preparando ambiente" (antes de subir containers):
```bash
mkdir -p "$INSTALL_DIR/config/workspace"
mkdir -p "$INSTALL_DIR/personas"
sudo chown -R 1000:1000 "$INSTALL_DIR/config" "$INSTALL_DIR/personas" 2>/dev/null || \
  chown -R 1000:1000 "$INSTALL_DIR/config" "$INSTALL_DIR/personas" 2>/dev/null || true
```

2. Na etapa "Iniciando Bastion" (antes do `docker compose up`):
```bash
sudo chown -R 1000:1000 "$INSTALL_DIR/config" "$INSTALL_DIR/personas" 2>/dev/null || \
  chown -R 1000:1000 "$INSTALL_DIR/config" "$INSTALL_DIR/personas" 2>/dev/null || true
```

### Problema em aberto

O `chown` requer `sudo`, o que interrompe o installer com prompt de senha em ambientes não interativos (CI/CD, VPS sem sudo configurado). Uma solução mais robusta seria usar `docker run --user` ou um entrypoint que ajuste permissões antes de iniciar o processo principal.

---

## 2. Corrupção do USER.md — Escrita Fora do Padrão

### Descrição

Durante o onboarding, o agente tentou escrever no `USER.md` mas produziu um arquivo malformado: campos duplicados, YAML sem quebras de linha corretas, frontmatter não fechado, e conteúdo de persona misturado com o perfil do usuário.

### Conteúdo corrompido observado (trecho)

```
trigger_keywords: ["cliente", "venda", "receita", "produto", "startup"]
clawhub_skills: ["google-calendar", "notion-tasks", "web-search"]
---

Você é a persona meu negócio na Katana de Mário.
...
totp_configured: false
occupation: "Desenvolvedor de Software (Tech Lead)"
has_business: true
...
---<!-- Este arquivo é gerado automaticamente pelo skill bastion/onboarding. -->
```

### Problemas identificados no arquivo resultante

1. **Campos duplicados**: `language` aparece duas vezes
2. **Frontmatter não fechado corretamente**: `totp_configured: false` colado na mesma linha que o slug da persona
3. **Conteúdo de SOUL.md de persona misturado** com o USER.md
4. **Comentários HTML colados** sem quebra de linha após o fechamento do frontmatter (`---`)
5. **Campo `totp_configured`** ausente ou mal posicionado — o agente não conseguirá ler o estado correto do TOTP

### Schema correto do USER.md

```yaml
---
name: "Mário"
language: "pt-BR"
authorized_user_ids:
  - "1648069744"
totp_configured: false
personas:
  - slug: "carreira-em-tech"
    name: "Carreira em Tech"
    base_weight: 0.8
    current_weight: 0.8
  - slug: "meu-negocio-na-katana"
    name: "meu negócio na Katana"
    base_weight: 0.7
    current_weight: 0.7
  - slug: "saude"
    name: "Saúde"
    base_weight: 0.6
    current_weight: 0.6
---

<!-- Este arquivo é gerado automaticamente pelo skill bastion/onboarding. -->
<!-- O campo authorized_user_ids é imutável pelo agente — gerenciado apenas pelo installer. -->
```

### Causa raiz provável

O skill de onboarding tentou escrever o USER.md enquanto o erro de permissão nas personas estava ocorrendo. O agente fez múltiplas tentativas de escrita parcial, concatenando conteúdo de diferentes arquivos (SOUL.md das personas + USER.md) em uma única operação de escrita mal-sucedida.

O `USER.md` no host é mapeado como `rw` e pertence ao usuário `mario` (não `root`), então a escrita foi parcialmente bem-sucedida — mas o conteúdo estava corrompido porque o agente perdeu o contexto de qual arquivo estava escrevendo.

### Ação necessária

1. **Corrigir manualmente o USER.md** com o schema correto (ver acima)
2. **Corrigir o slug da persona de saúde** — o agente gerou o slug `saude-faco-academia-e-preciso-que-vc-tb-aja-como-um-nutricionista` (muito longo, derivado da descrição literal). O slug correto deve ser `saude`
3. **Reiniciar o onboarding** após corrigir as permissões e o USER.md

### USER.md corrigido para aplicar

```yaml
---
name: "Mário"
language: "pt-BR"
authorized_user_ids:
  - "1648069744"
totp_configured: false
personas:
  - slug: "carreira-em-tech"
    name: "Carreira em Tech"
    base_weight: 0.8
    current_weight: 0.8
  - slug: "meu-negocio-na-katana"
    name: "meu negócio na Katana"
    base_weight: 0.7
    current_weight: 0.7
  - slug: "saude"
    name: "Saúde"
    base_weight: 0.6
    current_weight: 0.6
---

<!-- Este arquivo é gerado automaticamente pelo skill bastion/onboarding. -->
<!-- O campo authorized_user_ids é imutável pelo agente — gerenciado apenas pelo installer. -->
```

---

## 3. Problema Secundário — Slug de Persona Gerado Incorretamente

### Descrição

O agente gerou o slug `saude-faco-academia-e-preciso-que-vc-tb-aja-como-um-nutricionista` a partir da descrição literal do usuário ("saúde (Faço academia e preciso que vc tb aja como um nutricionista)").

### Impacto

- Diretório de persona com nome absurdamente longo
- Referências no USER.md inconsistentes
- Dificuldade de manutenção manual

### Correção necessária no skill de onboarding

O skill deve normalizar slugs: usar apenas a palavra-chave principal da área de vida, não a descrição completa. Exemplos:

| Input do usuário | Slug incorreto | Slug correto |
|-----------------|----------------|--------------|
| "saúde (Faço academia e preciso que vc tb aja como um nutricionista)" | `saude-faco-academia-e-preciso-que-vc-tb-aja-como-um-nutricionista` | `saude` ou `nutricionista` |
| "meu negócio na Katana" | `meu-negocio-na-katana` | `katana` ou `negocios` ou `CEO Katana` |
| "Carreira em Tech e hoje sou Tech Lead" | `carreira-em-tech` | `carreira-em-tech` ou `Desenvolvedor (Tech Lead)` |

---

## 4. Resumo de Ações Necessárias

| # | Ação | Responsável | Prioridade |
|---|------|-------------|------------|
| 1 | `sudo chown -R 1000:1000 config/ personas/` no host | Operador | Imediata |
| 2 | Corrigir `USER.md` com schema correto (ver seção 2) | Operador | Imediata |
| 3 | Reiniciar onboarding no Telegram após correções | Operador | Imediata |
| 4 | Corrigir skill de onboarding para gerar slugs normalizados | Dev | Alta |
| 5 | Investigar solução sem `sudo` para o `chown` no installer | Dev | Média |

## Observações de quem tá escrevendo esse código

- Seria bom termos o resumo do perfil do cliente no USER.md? Tipo um campo "Resumo do Usuário" ou "Bio do usuário". Se sim, o Onboarding tem que se preparar para perguntar isso antes de escrever as personas, deixando o usuario falar livremente (Bom botar um limite de caracteres).
- Também seria bom perguntar como ele quer ajuda principalmente, suas maiores dores e objetivos.
- Essas duas perguntas podem guiar a escrita das personas.
- Ao escrever as personas mesmo, tem que perguntar oq o usuario quer que cada persona faça e suas habilidades, e aí ela faz a parte de escrever skills ou pesquisa-las no hub.
- Depois de configurar as personas, precisamos que o usuário determine a identidade e comportamento do nosso bot, além do nome ou se quer manter o Bastion. (Dúvida, é assim que o IDENTITY é preenchido? Ou ele só muda com o tempo? Ele conflitua com nossas personas? Eu gosto da ideia do Agente principal e as personas como personalidades que ele assume e que podem inclusive sobrescrever a personalidade principal)
- No fim das correções, garanta que o USER.md n está subindo pro github, pq msm no gitignore ele está sendo adicionado ao ser modificado pelo bot.