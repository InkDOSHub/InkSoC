# InkSoC — Contexto operacional de sessão

> Documento de passagem compacto. Não é histórico completo nem fonte superior ao estado atual do GitHub.
> Atualizado em: 2026-11-15 (revisão às 16:47 UTC). `main` observado nesta revisão: `9b9c0944aa5ed75482bc0089d6e96e2da069b047`; a base original deste PR era `4776d79412c66b867391ee3e389a0d1a861ba586`.

## 1. Projeto em uma frase

Engine x86-64 própria em Rust/WebAssembly, com JIT, suporte a código 32-bit/WoW64 e componentes Wine, executada no browser com foco em WebKit/iPhone/iPad.

## 2. Fonte de verdade — consultar antes de agir

1. `main`, commits, branches, PRs e GitHub Actions atuais: estado real e prioritário.
2. `docs/collaboration/PROTOCOL.md`: fluxo humano-assistido e regras de governança.
3. `PROJECT_LOG.md`: histórico resumido; pode conter registros antigos que precisam ser corrigidos em tarefa documental própria.
4. `ROADMAP.md`: prioridades técnicas, não autorização automática.
5. `VALIDATION_MATRIX.md`: níveis e critérios de validação.
6. Código, testes e workflows da área afetada.

Este arquivo é apenas um índice operacional. SHAs e estados aqui registrados são snapshots; sempre revalidar no GitHub.

## 3. Papéis e fluxo real

- **Mantenedor:** define prioridade, transfere tarefas entre agentes e autoriza merges.
- **GPT-6:** investiga GitHub, lê código/diffs/Actions, planeja, especifica, audita e prepara instruções.
- **Claude Code:** executor numa máquina na nuvem com repositório clonado; recebe tarefa manualmente e implementa em branch autorizada.
- **GitHub Actions:** builds e testes automatizados vinculados ao SHA.
- **GitHub:** memória persistente do projeto.

Fluxo: GPT-6 verifica estado → escreve tarefa delimitada → mantenedor transfere para Claude → Claude implementa e abre/atualiza PR draft → Actions valida → mantenedor traz evidências → GPT-6 audita diretamente no GitHub.

Não existe Claude Code GitHub Action instalada; `@claude` em comentário não inicia execução. Não há despacho automático nem fluxo de e-mail dedicado.

## 4. Economia de créditos e contexto

- GPT-6 faz a investigação e auditoria diretamente no GitHub sempre que possível.
- Claude usa o modelo econômico escolhido pelo mantenedor (Haiku 5.5) como executor, não como agente de exploração.
- Uma tarefa por vez, escopo fechado, arquivos permitidos/proibidos e comandos exatos.
- Não pedir leituras, buscas, builds ou testes já realizados e ainda válidos para o mesmo SHA.
- Proibir refatorações, melhorias oportunistas e expansão de escopo.
- Se houver conflito, bloqueio ou ambiguidade, Claude para e relata; não improvisa.
- Relatório mínimo: branch, SHA, arquivos alterados, comandos/resultados, links de Actions e limitações.
- Economizar tokens sem reduzir testes, revisão independente ou segurança.

## 5. Regras invioláveis

1. Nunca editar `main` diretamente; usar branch isolada e PR.
2. Nunca fazer merge sem autorização explícita e específica do mantenedor.
3. Não fazer force-push nem reescrever histórico sem autorização.
4. Uma mudança lógica por PR; não ampliar escopo.
5. Não remover/enfraquecer testes para obter CI verde.
6. Distinguir identificado, diagnosticado, implementado, compilado, testado, validado e confirmado funcionalmente.
7. Toda evidência de CI deve ser conferida no SHA exato.
8. Classificar falhas: CODE, TEST, INFRA, NETWORK, TIMEOUT, ENVIRONMENT ou UNKNOWN.
9. Build/instalação concluídos não provam compatibilidade funcional.
10. Registrar decisões duradouras e marcos verificáveis no GitHub.

## 6. Trabalho em andamento no snapshot

### PR #6 — build WASM stable + multi-thread em PRs
- PR: https://github.com/InkDOSHub/InkSoC/pull/6
- Branch: `ci/pr-engine-build-stable-mt`; head atual observado: `8782e11f17821a967013a6d3dcc968d9c1c6a64b`.
- Base atual observada: `main` = `9b9c0944aa5ed75482bc0089d6e96e2da069b047`.
- Aberto, draft, não mergeado; diff restrito a `.github/workflows/engine.yml` (43 adições, 10 remoções), 4 commits à frente e 0 atrás da base atual.
- Review GPT-6 anterior: ACCEPTED para CI no SHA `84074d8`; essa evidência não cobre o novo head.
- Nova run #43737980582: iniciada em evento `pull_request` e vinculada ao SHA exato `8782e11`. Na última consulta, job build estava em andamento na etapa stable; build multi-thread e upload ainda pendentes. Não declarar validado até a conclusão desta run e conferência de publish skipped.
- Nenhuma validação funcional do engine no navegador foi feita; merge depende de autorização explícita.

### PR #10 — harmonização do guia de colaboração
- PR: https://github.com/InkDOSHub/InkSoC/pull/10
- Branch: `docs/harmonize-ai-collaboration`; head observado: `534ce17148b12cab80a63ff78071da201189dc49`.
- Altera somente `AI_COLLABORATION.md`, alinhando o documento ao protocolo operacional atual e substituindo referências antigas a Claude Opus por Claude Code.
- Auditoria GPT-6: ACCEPTED para o escopo documental; aberto, draft, não mergeado.

### PR #8 — este documento de contexto
- PR: https://github.com/InkDOSHub/InkSoC/pull/8
- Branch: `docs/session-context`; head observado após sincronização não destrutiva com `main`: `e6e99a4d30785a40be430d5d49b6b7a79749f069`.
- Aberto, draft, não mergeado; contém somente `docs/collaboration/SESSION_CONTEXT.md` (93 adições).
- Auditoria de conteúdo concluída; revisão registrada na conversa do PR. Estado de mergeabilidade deve ser revalidado no GitHub.

### PR #9 — correção do PROJECT_LOG
- PR: https://github.com/InkDOSHub/InkSoC/pull/9
- Branch: `docs/project-log-pr7-merge`; head observado após sincronização não destrutiva com `main`: `3108dea01a06837164b7f16317e202e652c7fd4f`.
- Altera somente `PROJECT_LOG.md`, adicionando registro de que PR #7 foi integrado via squash em `4776d79412c66b867391ee3e389a0d1a861ba586` (13 adições, nenhuma remoção).
- Conteúdo auditado e aceito em comentário de revisão; PR aberto, draft e não mergeado.

### Outras pendências documentais identificadas
- `AI_COLLABORATION.md` ainda descreve Claude Opus e um protocolo antigo; `docs/collaboration/PROTOCOL.md` descreve o fluxo real humano-assistido. Harmonizar em tarefa documental separada, sem misturar com CI/runtime.

## 7. Como iniciar uma nova sessão

Mensagem sugerida para a nova conversa:

> Vamos continuar o InkSoC. Leia primeiro `docs/collaboration/SESSION_CONTEXT.md` no GitHub, depois consulte o estado atual de main, PRs, branches e Actions. Use o GitHub como fonte de verdade, atualize o contexto operacional mentalmente e continue da próxima ação autorizada. Não repita análises já concluídas nem presuma que os SHAs do snapshot continuam atuais.

## 8. Atualização deste arquivo

Atualizar somente em marcos relevantes (merge, novo PR prioritário, mudança de protocolo, decisão arquitetural ou mudança do fluxo de trabalho). Manter curto; substituir estados antigos em vez de acumular diário. Sempre registrar data e SHA-base observado. Não copiar logs extensos nem declarar testes sem evidência.
