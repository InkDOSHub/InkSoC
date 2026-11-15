# Tarefa de implementação — <ID>

- **Estado:** PROPOSED
- **Prioridade:** <P0/P1/P2/P3>
- **Predecessora:** <ID ou nenhuma>
- **Executor:** Claude
- **Revisor independente:** GPT-6
- **SHA-base:** <commit completo>
- **Issue/PR:** <URL>

## Problema e impacto
<Descrição objetiva do comportamento observado e seu impacto.>

## Evidências atuais
- <arquivo:linha, log, teste, workflow e SHA>
- **Fatos confirmados:**
- **Hipóteses:**
- **Incógnitas:**

## Resultado esperado
<Comportamento que deve existir após a implementação.>

## Escopo permitido
- <arquivos, módulos e alterações permitidos>

## Fora do escopo
- <arquivos, módulos e alterações proibidos>

## Invariantes
- Não alterar main diretamente.
- Não fazer merge.
- Não remover ou enfraquecer testes.
- Não ampliar escopo sem nova autorização.
- Preservar compatibilidade e contratos não relacionados.

## Plano proposto
1. <passo>
2. <passo>
3. <passo>

## Testes e comandos
- <teste existente>
- <teste novo>
- <build stable/multi-thread, quando aplicável>

## Critérios de aceitação
- [ ] <critério objetivo verificável>
- [ ] Resultado associado ao SHA exato.
- [ ] Testes e limitações relatados sem extrapolação.
- [ ] Diff não contém alterações fora do escopo.

## Riscos e rollback
- **Riscos:** <lista>
- **Rollback:** <como reverter sem afetar outras mudanças>

## Relatório obrigatório de Claude
1. ID da tarefa, branch, commits e SHA final.
2. Arquivos alterados e resumo do diff.
3. Decisões técnicas e alternativas descartadas.
4. Comandos executados e resultados exatos.
5. URLs de Actions e artefatos.
6. Testes não executados e motivo.
7. Classificação das falhas: CODE, TEST, INFRA, NETWORK, TIMEOUT, ENVIRONMENT ou UNKNOWN.
8. Riscos remanescentes e próxima ação sugerida.

**Não marcar como validado sem evidência para todos os critérios de aceitação.**
