# InkSoC — Colaboração entre agentes e mantenedor

**Status:** documento complementar ao protocolo operacional  
**Fonte normativa:** [`docs/collaboration/PROTOCOL.md`](docs/collaboration/PROTOCOL.md)  
**Última harmonização:** 2026-11-15

Este arquivo resume responsabilidades, contratos de execução e critérios de revisão. Em caso de divergência, prevalece `docs/collaboration/PROTOCOL.md`, que descreve o fluxo real e as transições do ciclo de trabalho. O estado atual do repositório, dos PRs e das Actions deve sempre ser consultado diretamente no GitHub.

## 1. Participantes e responsabilidades

| Participante | Responsabilidade | Limites |
|---|---|---|
| Mantenedor humano | Define prioridades e escopo, transfere tarefas entre agentes e autoriza merges | A decisão final de integração permanece humana |
| GPT-6 — coordenador técnico | Investiga o GitHub, analisa arquitetura e diffs, planeja tarefas, define critérios e faz auditoria independente | Não afirma ter executado builds/testes sem evidência; não faz merge sem autorização explícita |
| Claude Code — executor | Recebe a tarefa do mantenedor, implementa em máquina na nuvem com o repositório clonado e entrega branch/PR e evidências | Não altera `main`, não amplia escopo, não enfraquece testes e não faz merge sem autorização |
| GitHub | Mantém código, branches, commits, PRs, Issues, Actions e artefatos | É a fonte persistente de estado e evidência |
| GitHub Actions | Executa os workflows configurados e registra resultados associados a SHAs | Um check verde prova somente o escopo efetivamente executado |

O fluxo atual é humano-assistido. O mantenedor transporta a tarefa e o resultado entre GPT-6 e Claude Code. Não existe Claude Code GitHub Action instalada no repositório; comentários `@claude` não iniciam execução. Nenhum workflow atual aciona GPT-6 ou Claude Code.

## 2. Fluxo operacional

1. GPT-6 consulta o estado atual do GitHub e identifica a próxima tarefa autorizada.
2. GPT-6 registra uma especificação delimitada, com ID, baseline, arquivos permitidos/proibidos, invariantes, testes e critérios de aceitação.
3. O mantenedor transfere manualmente a tarefa para Claude Code.
4. Claude Code implementa em branch isolada, sem force-push ou alteração direta de `main`, e entrega PR normalmente em draft.
5. GitHub Actions executa os workflows existentes; resultados devem ser associados ao SHA exato.
6. O mantenedor traz os links e evidências para a revisão.
7. GPT-6 inspeciona diretamente diff, commits, PR e Actions; o relato do executor não substitui essa auditoria.
8. GPT-6 registra ACCEPTED, REVISION_REQUIRED ou BLOCKED e propõe a próxima ação.
9. Merge depende sempre de autorização explícita e específica do mantenedor.

Detalhes do ciclo, estados, notificações, acesso do Claude ao GitHub e eventuais integrações futuras estão no [protocolo operacional](docs/collaboration/PROTOCOL.md).

## 3. Contrato para tarefas de implementação

Toda tarefa enviada ao executor deve conter:

- ID e prioridade;
- problema, impacto e evidências;
- hipótese causal separada de fatos confirmados;
- resultado esperado;
- SHA-base;
- arquivos/componentes permitidos e áreas proibidas;
- invariantes que não podem mudar;
- plano e comandos exatos de build/test;
- critérios de aceitação verificáveis;
- riscos e condições para parar.

O executor deve devolver, de forma concisa:

1. branch, SHA final e PR;
2. arquivos alterados e resumo do diff;
3. comandos executados e resultados;
4. links de Actions e artefatos;
5. testes não executados e limitações;
6. riscos ou bloqueios encontrados.

Se houver conflito, ambiguidade ou necessidade de ampliar o escopo, deve parar e relatar, sem improvisar.

## 4. Regras de segurança e qualidade

- Nunca editar `main` diretamente.
- Nunca fazer merge sem autorização explícita.
- Uma mudança lógica por branch/PR sempre que possível.
- Não fazer force-push nem reescrever histórico sem autorização específica.
- Não remover, enfraquecer ou contornar testes para obter CI verde.
- Não alterar runtime, ABI, memória, JIT, loader, scheduler, sincronização ou dependências fora do escopo aprovado.
- Não editar arquivos gerados manualmente; localizar a fonte geradora.
- Não classificar build ou instalação concluídos como compatibilidade funcional.
- Não declarar validação sem workflow/run, SHA, resultado e cobertura.
- Preservar logs e classificar falhas como CODE, TEST, INFRA, NETWORK, TIMEOUT, ENVIRONMENT ou UNKNOWN.
- Revalidar o diff após qualquer atualização da branch com `main`.

## 5. Estados de evidência

Usar as distinções definidas no protocolo:

- **Identificado:** observação ou indício, sem causa confirmada.
- **Diagnosticado:** causa sustentada por código, reprodução ou evidência.
- **Implementado:** código alterado; não implica build.
- **Compilado:** build passou para o SHA indicado.
- **Testado:** teste nomeado foi executado.
- **Validado:** todos os critérios de aceitação da tarefa passaram.
- **Confirmado funcionalmente:** comportamento observado no ambiente de destino nos cenários declarados.
- **Inconclusivo:** faltam evidências ou o ambiente impediu a conclusão.

Nunca promover automaticamente um estado para o seguinte.

## 6. Checklist da revisão independente

O revisor deve examinar o diff e verificar, conforme a área afetada:

- aderência à especificação e ausência de alterações fora do escopo;
- correção lógica, tratamento de erros, memória, concorrência e ABI;
- consumidores, invariantes e efeitos em single-core/multi-core;
- compatibilidade x86-64, WoW64 e navegadores-alvo, quando aplicável;
- regressões e qualidade dos testes novos (inclusive se detectariam a falha anterior);
- segurança, dependências e licenças;
- correspondência entre o relatório e os logs reais;
- vínculo de cada resultado de CI ao SHA auditado.

A revisão deve apontar bloqueadores concretos. Aceitação técnica não substitui checks automatizados nem autorização humana.

## 7. Documentos relacionados

- [Protocolo operacional](docs/collaboration/PROTOCOL.md) — fonte normativa do fluxo humano-assistido.
- [Contexto de sessão](docs/collaboration/SESSION_CONTEXT.md) — resumo para retomada em novas conversas; snapshots precisam ser revalidados.
- [Registro do projeto](PROJECT_LOG.md) — histórico de marcos verificados.
- [Roadmap](ROADMAP.md) — prioridades técnicas.
- [Matriz de validação](VALIDATION_MATRIX.md) — níveis e critérios de evidência.

Decisões arquiteturais duradouras devem ser registradas em `DECISIONS.md` ou ADRs em `docs/decisions/`, com contexto, alternativas, decisão, consequências e data.

## 8. Critério de sucesso

O processo é bem-sucedido quando mantém escopo pequeno, reduz regressões, conserva evidências reproduzíveis, explicita o que foi e não foi testado e preserva com o mantenedor a decisão final de integração.
