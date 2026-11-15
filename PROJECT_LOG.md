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
