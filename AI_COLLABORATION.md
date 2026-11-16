# InkSoC — condução técnica e colaboração

**Fonte normativa:** [protocolo operacional](docs/collaboration/PROTOCOL.md)  
**Contexto de continuidade:** [SESSION_CONTEXT.md](docs/collaboration/SESSION_CONTEXT.md)

## Responsabilidades

| Participante | Responsabilidade |
|---|---|
| Mantenedor humano | Define prioridades, valida no dispositivo real quando necessário e decide merges/publicações |
| GPT-6 | Conduz investigação, arquitetura, implementação em branches, testes disponíveis, PRs, auditoria de diffs e relatórios |
| GitHub | Fonte de verdade para código, histórico, branches, PRs e evidências |
| GitHub Actions | Executa builds e testes associados a SHAs |

O projeto não depende de Claude Code nem de créditos de outro agente executor.

## Fluxo

1. Consultar o estado atual do repositório e do Graphify.
2. Formular hipótese e critérios de aceitação.
3. Criar branch a partir do HEAD confirmado de `main`.
4. Implementar uma mudança lógica por vez.
5. Executar verificações e testes não destrutivos.
6. Revisar o diff e os riscos de regressão/segurança.
7. Abrir PR com SHA-base, resumo, testes, Actions e limitações.
8. Solicitar validação do mantenedor no Safari/iPhone/iPad quando o ambiente real for indispensável.
9. Integrar somente com autorização específica para merge.

## Regras

- Nunca editar `main` diretamente, fazer force-push ou reescrever histórico.
- Não apagar dados reais nem alterar segredos/configurações administrativas sem autorização específica.
- Não enfraquecer testes nem ocultar falhas.
- Não declarar build, instalação ou smoke test como compatibilidade funcional.
- Não simular métricas indisponíveis.
- Não anexar arquivos pessoais ou transcrições de conversa ao repositório público.
- Manter registros técnicos mínimos, sintéticos e anonimizados.
- Conferir resultados de Actions no SHA exato do diff revisado.

## Relatórios

Toda entrega deve informar:
1. branch, SHA-base, SHA final e PR;
2. arquivos alterados e resumo do diff;
3. comandos/testes executados e resultados;
4. links de Actions vinculados ao SHA;
5. testes não executados e limitações;
6. riscos, regressões possíveis e próximo passo.

Use os estados **identificado**, **diagnosticado**, **implementado**, **compilado**, **testado**, **validado** e **confirmado funcionalmente** com rigor. Não confundir um estado com o seguinte.
