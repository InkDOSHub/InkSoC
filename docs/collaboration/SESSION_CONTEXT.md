# InkSoC — Contexto operacional de sessão

> Documento de passagem compacto. Não é histórico completo nem fonte superior ao estado atual do GitHub.
> Atualizado em: 2026-11-15. `main` observado: `9b9c0944aa5ed75482bc0089d6e96e2da069b047`.
> Os estados e SHAs abaixo são snapshots; revalidar antes de qualquer ação.

## 1. Projeto em uma frase

Engine x86-64 própria em Rust/WebAssembly, com JIT, suporte a código 32-bit/WoW64 e componentes Wine, executada no browser com foco em WebKit/iPhone/iPad.

## 2. Fonte de verdade — consultar antes de agir

1. `main`, commits, branches, PRs e GitHub Actions atuais.
2. `docs/collaboration/PROTOCOL.md`: fluxo humano-assistido e regras de governança.
3. `PROJECT_LOG.md`: histórico resumido; pode conter registros antigos.
4. `ROADMAP.md`: prioridades técnicas, não autorização automática.
5. `VALIDATION_MATRIX.md`: níveis e critérios de validação.
6. Código, testes e workflows da área afetada.

Este arquivo é um índice operacional, não um substituto do GitHub.

## 3. Papéis e fluxo real

- **Mantenedor:** define prioridades, transfere tarefas entre agentes e autoriza merges.
- **GPT-6:** investiga GitHub, lê código/diffs/Actions, planeja, especifica e audita.
- **Claude Code:** executor numa máquina na nuvem com repositório clonado; recebe tarefas manualmente e implementa em branch autorizada.
- **GitHub Actions:** executa builds e testes automatizados vinculados a SHAs.
- **GitHub:** memória persistente e fonte de verdade do projeto.

Fluxo: GPT-6 verifica estado → define tarefa delimitada → mantenedor transfere para Claude → Claude implementa em branch e abre/atualiza PR draft → Actions valida → GPT-6 audita diretamente no GitHub.

Não existe Claude Code GitHub Action instalada; comentário `@claude` não inicia execução. Não há despacho automático nem fluxo de e-mail dedicado.

## 4. Economia de créditos e contexto

- GPT-6 faz investigação e auditoria diretamente no GitHub sempre que possível.
- Claude Haiku 5.5 atua como executor de tarefas delimitadas, não como agente de exploração.
- Uma tarefa por vez, com escopo, arquivos permitidos/proibidos e comandos definidos.
- Não repetir leituras, buscas, builds ou testes ainda válidos para o mesmo SHA.
- Sem refatorações oportunistas ou expansão de escopo.
- Diante de conflito, bloqueio ou ambiguidade, parar e relatar.
- Relatório mínimo: branch, SHA, arquivos alterados, comandos/resultados, links de Actions e limitações.
- Economizar tokens sem reduzir testes, revisão independente ou segurança.

## 5. Regras invioláveis

1. Nunca editar `main` diretamente; usar branch isolada e PR.
2. Nunca fazer merge sem autorização explícita e específica do mantenedor.
3. Não fazer force-push nem reescrever histórico sem autorização.
4. Uma mudança lógica por PR; não ampliar escopo.
5. Não remover/enfraquecer testes para obter CI verde.
6. Distinguir identificado, diagnosticado, implementado, compilado, testado, validado e confirmado funcionalmente.
7. Conferir evidências de CI no SHA exato.
8. Classificar falhas: CODE, TEST, INFRA, NETWORK, TIMEOUT, ENVIRONMENT ou UNKNOWN.
9. Build/instalação concluídos não provam compatibilidade funcional.
10. Registrar decisões duradouras e marcos verificáveis no GitHub.

## 6. Estado dos trabalhos no snapshot

### PR #6 — build WASM stable + multi-thread em PRs

- PR: https://github.com/InkDOSHub/InkSoC/pull/6
- Branch: `ci/pr-engine-build-stable-mt`
- Head: `8782e11f17821a967013a6d3dcc968d9c1c6a64b`; base `main`: `9b9c0944aa5ed75482bc0089d6e96e2da069b047`.
- Aberto, draft, não mergeado; diff restrito a `.github/workflows/engine.yml` (43 adições, 10 remoções); `mergeable: true`.
- Run #43737980582, vinculada ao SHA exato: workflow SUCCESS; job `build` SUCCESS; builds stable e multi-thread SUCCESS; upload SUCCESS; job `publish` SKIPPED (esperado em `pull_request`).
- Artefato `engine-wasm`: 1.164.229 bytes; SHA-256 `ea51a491097ea84a1dce9b6e09f7d19cde454c69a707ee06e19678ff4d36e095`.
- Auditoria GPT-6 ACCEPTED registrada no PR e vinculada ao head `8782e11`.
- Evidência valida somente o escopo de CI/build e geração do artefato. Não houve validação funcional do engine no navegador. Merge depende de autorização explícita.

### PR #8 — este documento de contexto

- PR: https://github.com/InkDOSHub/InkSoC/pull/8
- Branch: `docs/session-context`; head antes desta atualização: `0d62d7e6b311f3be7c0245486d4fea5569880712`.
- Aberto, draft, não mergeado; um arquivo (`docs/collaboration/SESSION_CONTEXT.md`); `mergeable: true`.
- Esta atualização registra o resultado final da run #43737980582 e sincroniza os snapshots dos PRs abertos.
- Após este commit, o head do PR #8 mudará; revisar o novo diff e atualizar a auditoria do próprio PR.

### PR #9 — correção do PROJECT_LOG

- PR: https://github.com/InkDOSHub/InkSoC/pull/9
- Branch: `docs/project-log-pr7-merge`; head observado: `3108dea01a06837164b7f16317e202e652c7fd4f`.
- Aberto, draft, não mergeado; altera somente `PROJECT_LOG.md` (13 adições, 0 remoções); `mergeable: true`.
- Registra que PR #7 foi integrado via squash em `4776d79412c66b867391ee3e389a0d1a861ba586`.

### PR #10 — harmonização do guia de colaboração

- PR: https://github.com/InkDOSHub/InkSoC/pull/10
- Branch: `docs/harmonize-ai-collaboration`; head observado: `534ce17148b12cab80a63ff78071da201189dc49`.
- Aberto, draft, não mergeado; altera somente `AI_COLLABORATION.md` (92 adições, 128 remoções); `mergeable: true`.
- Alinha o guia ao protocolo operacional atual, substituindo referências antigas a Claude Opus por Claude Code.

### PR #7 — protocolo de colaboração

- Integrado em 2026-11-15 via squash merge.
- Merge commit em `main`: `4776d79412c66b867391ee3e389a0d1a861ba586`.

## 7. Como iniciar uma nova sessão

Mensagem sugerida:

> Vamos continuar o InkSoC. Leia primeiro `docs/collaboration/SESSION_CONTEXT.md` no GitHub, depois consulte o estado atual de main, PRs, branches e Actions. Use o GitHub como fonte de verdade, revalide SHAs e continue da próxima ação autorizada. Não repita análises ainda válidas nem presuma que os snapshots continuam atuais.

## 8. Atualização deste arquivo

Atualizar em marcos relevantes (merge, novo PR prioritário, mudança de protocolo, decisão arquitetural ou mudança do fluxo de trabalho). Manter curto; substituir estados antigos em vez de acumular diário. Registrar data e SHA-base observado. Não copiar logs extensos nem declarar testes sem evidência.
