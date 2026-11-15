## Atualização — 2026-11-15 — Protocolo de colaboração por eventos

- Agente executor: GPT-6 (auditoria e documentação).
- Agente revisor: pendente de revisão independente.
- Issue/PR: PR draft de infraestrutura de colaboração, a ser aberto nesta branch.
- Branch: `docs/collaboration-cycle-v1`.
- Baseline verificada: `main` em `d923a3eae86669d807b15d131400636e8137b74d`.
- Alteração: adicionados `docs/collaboration/PROTOCOL.md`, `TASK_TEMPLATE.md`, `AUDIT_TEMPLATE.md` e `INTEGRATION_DESIGN.md`. Definem ciclo por eventos, estados, contratos de evidência, auditoria cruzada, idempotência, limites de automação e alternativas de notificação.
- Estado: **implementado (documentação)**; não compilado nem testado por Actions nesta etapa.
- Evidências observadas: PRs #2, #3, #4 e #6 estavam abertos/draft na consulta; PR #6 head `83b81ca3fbaaf646d8a501962fda6541d3792594` tem workflow `engine` run [#43728544126](https://github.com/InkDOSHub/InkSoC/actions/runs/43728544126) concluído com sucesso. Comparações com main: PR #2 ahead 5/behind 5; #3 ahead 2/behind 2; #4 ahead 8/behind 2; #6 ahead 2/behind 1. Esses PRs estão divergentes e precisam de revisão/rebase controlado antes de eventual integração.
- Riscos / pendências: branch `main` observada sem proteção (`protected: false`); `.github/workflows/engine.yml` em main concede `contents: write` e faz commit/push de artefatos em main. O PR #6 propõe separar build de PR e publicação, mas ainda não está integrado. O mecanismo que efetivamente dispara Claude não foi identificado; e-mail dedicado depende de provedor/secret e não foi implementado. O endpoint de branch protection não pôde ser consultado por permissão da integração.
- Próxima ação autorizada: revisar este PR draft; depois identificar e validar a integração Claude e decidir o canal de notificação. Nenhuma alteração em runtime, workflow de publicação, configuração administrativa ou main foi feita.

# InkSoC — Log técnico e passagem de contexto

Este arquivo é o registro resumido do estado do projeto para o mantenedor e para agentes de IA. Atualizar somente com fatos verificáveis. O histórico completo permanece nos commits, PRs, Issues e execuções do GitHub Actions.

## Snapshot inicial — 2026-11-15

### Baseline da branch principal

- Repositório: [InkDOSHub/InkSoC](https://github.com/InkDOSHub/InkSoC)
- Branch principal: `main`
- SHA observado: `425ab35c2905d51cbb107aa483233553253c556a`
- Descrição do projeto segundo o README: engine x86-64 própria em Rust/WebAssembly com JIT, suporte a código 32-bit/WoW64 e componentes Wine; execução no browser, com foco em WebKit/iPhone/iPad.
- Build de engine publicado no histórico: `479721d8b254e6e44036c1109747f8f2a0598951`, associado ao commit de origem `b686482fc2e1e592d1fdb1564f689c70edf296c8`. O workflow de Pages associado foi concluído com sucesso. Isso comprova o fluxo de publicação/build correspondente, não compatibilidade geral de aplicativos.

### Estado dos PRs observado no snapshot

| PR | Tema | Estado observado | Evidência / ressalva |
|---|---|---|---|
| [#2](https://github.com/InkDOSHub/InkSoC/pull/2) | Robustez do runner de compatibilidade | Aberto, draft | Check de sintaxe Python e JSON passou; suíte completa de aplicativos não foi comprovada por esse check. |
| [#3](https://github.com/InkDOSHub/InkSoC/pull/3) | Auditoria documental da inicialização Wine | Aberto, draft | Documentação; não altera comportamento executável. |
| [#4](https://github.com/InkDOSHub/InkSoC/pull/4) | STARTUPINFO / Desktop / smoke test | Aberto, draft | O workflow `startup-smoke`, execução [#43726419180](https://github.com/InkDOSHub/InkSoC/actions/runs/43726419180), foi observado como `completed/success`. Antes de usar isso como evidência do head atual do PR, conferir que o SHA testado coincide exatamente com o head SHA atual. O smoke test cobre somente os cenários declarados no próprio teste. |

Nenhum desses PRs deve ser considerado integrado apenas por estar aberto ou ter checks verdes. Verificar estado e SHA atuais antes de agir.

### Configuração de proteção

No momento do snapshot, a consulta à API do GitHub indicou que a branch `main` não tinha proteção habilitada nem checks obrigatórios. Recomenda-se ao mantenedor configurar proteção de branch/regraset no GitHub, exigindo PR e checks adequados. Esta documentação não altera as configurações administrativas do repositório.

## Decisões operacionais vigentes

1. Não editar `main` diretamente.
2. Trabalhar em branches isoladas e abrir PR para `main`.
3. Não fazer merge sem autorização explícita do mantenedor.
4. Separar implementação, compilação, teste, validação e confirmação funcional.
5. Tratar GitHub Actions como fonte de evidência de build/teste, sempre vinculada ao SHA testado.
6. Separar falhas do código de falhas de infraestrutura, rede, download, timeout e indisponibilidade de dependências.
7. Manter revisão independente entre agentes de IA.

Ver também:
- [AI_COLLABORATION.md](AI_COLLABORATION.md) — papéis, comunicação e regras.
- [ROADMAP.md](ROADMAP.md) — ordem de trabalho e gates.
- [VALIDATION_MATRIX.md](VALIDATION_MATRIX.md) — critérios e evidências de teste.

## Modelo de atualização deste log

Acrescentar entradas no topo desta seção, sem apagar o histórico anterior:

```markdown
### YYYY-MM-DD — <assunto>
- Agente executor:
- Agente revisor:
- Issue/PR:
- Branch e SHA:
- Alteração:
- Evidência de build/teste:
- Estado: identificado | diagnosticado | implementado | compilado | testado | validado | confirmado funcionalmente | inconclusivo
- Riscos / pendências:
- Próxima ação autorizada:
```

Não registrar como concluído algo que ainda está em andamento. Não inventar datas, resultados, SHAs, links ou aprovação.
