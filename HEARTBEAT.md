# HEARTBEAT — Tarefas Agendadas do Bastion

O OpenClaw lê este arquivo a cada 30 minutos e executa as tarefas cujo intervalo foi atingido.

---

## Tarefas

### calendar-check
- **Intervalo**: a cada 30 minutos
- **Skill**: `bastion/proactive`
- **Ação**: verificar eventos do Google Calendar nos próximos 60 minutos
- **Condição de alerta**: se houver evento com início em ≤ 5 minutos, enviar lembrete imediato ao usuário
- **Formato do lembrete**: `🗓️ Em [X] minutos: [título do evento] — [horário]`

### persona-inactivity-check
- **Intervalo**: a cada 6 horas
- **Skill**: `bastion/proactive`
- **Ação**: verificar no life_log quais personas não têm atividade registrada há 3 ou mais dias
- **Condição de alerta**: para cada persona inativa, gerar uma sugestão de retomada personalizada com base no domínio da persona
- **Formato**: `💤 [Nome da Persona] está inativa há [N] dias. Quer retomar?`

### weekly-review
- **Intervalo**: toda segunda-feira às 9h
- **Skill**: `bastion/weekly-review`
- **Ação**: executar o skill `weekly-review` para todas as personas ativas
- **O que inclui**: agregar interações do life_log dos últimos 7 dias por persona, calcular métricas de uso, comparar com pesos atuais, gerar relatório com sugestões de ajuste de peso
- **Requer confirmação**: sim — apresentar sugestões ao usuário antes de aplicar qualquer ajuste de peso

### life-log-analysis
- **Intervalo**: a cada 7 dias
- **Skill**: `bastion/life-log` + `bastion/self-improving`
- **Ação**: analisar os últimos 50 registros do life_log de cada persona
- **O que inclui**:
  - Extrair padrões de comportamento e preferências
  - Atualizar `personas/{slug}/MEMORY.md` com novos aprendizados
  - Comparar padrão de uso atual com pesos configurados
  - Se o padrão mudou significativamente, sugerir ajustes de peso ao usuário
- **Requer confirmação**: sim — sugestões de ajuste de peso são apresentadas antes de aplicar

### cve-check
- **Intervalo**: a cada 24 horas
- **Skill**: `bastion/proactive`
- **Ação**: verificar CVEs das skills instaladas via ClawHub API
- **Condição de alerta**: se qualquer CVE for detectado em qualquer skill instalada, alertar o usuário **imediatamente** — antes de qualquer outra mensagem na próxima interação
- **Formato do alerta**: `⚠️ CVE detectado na skill [nome]: [descrição]. Recomendo desinstalar ou aguardar patch.`
- **Prioridade**: máxima — este alerta tem precedência sobre qualquer outra mensagem pendente

### validation-metrics-check
- **Intervalo**: a cada 6 horas
- **Skill**: `output-validator`
- **Ação**: ler `config/logs/validation-metrics.json` e calcular taxa de sucesso recente por skill
- **Condição de alerta**: se qualquer skill tiver taxa de sucesso recente abaixo de 90% (com mínimo de 20 amostras na janela), gerar alerta
- **Formato do alerta**: `⚠️ Drift de validação em [skill]: taxa de sucesso = [X]% (últimas [N] execuções). Último erro: [mensagem]`
- **Ação adicional**: se geração de schema falhar para qualquer skill (schema.json ausente e SKILL.md sem exemplo), alertar o usuário
- **Formato do alerta de schema**: `⚠️ Skill [nome] sem schema de validação configurado. Adicione ## Output Example ao SKILL.md.`
- **Prioridade**: normal — exibir na próxima interação após o alerta de CVE (se houver)

---

## Estado

O estado de execução de cada tarefa (último horário de execução) é persistido em `personas/{slug}/heartbeat-state.md` por persona, e no arquivo de estado global do OpenClaw.
