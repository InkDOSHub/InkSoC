# InkSoC — Protocolo de colaboração multicêntrica por IA

**Status:** proposta para revisão via PR  
**Branch de origem:** `main`  
**Baseline:** `425ab35c2905d51cbb107aa483233553253c556a`  
**Idioma operacional:** português para relatórios; inglês pode ser usado em nomes técnicos, código e mensagens de commit.

Este documento define como diferentes agentes de IA e o mantenedor humano colaboram no InkSoC. Ele não concede autonomia para aprovar, integrar ou publicar alterações.

## 1. Modelo de responsabilidades

| Participante | Responsabilidade principal | Não pode fazer |
|---|---|---|
| Mantenedor humano | Define prioridades, fornece contexto, decide escopo e aprova PRs/merges | Delegar a aprovação final sem revisão |
| GPT-6 — planejamento e auditoria | Analisar arquitetura, investigar causas, elaborar especificações, critérios de aceitação e revisão independente | Alegar que compilou/executou algo sem evidência; aprovar o próprio trabalho como validação final |
| Claude Opus — implementação | Implementar tarefas especificadas, escrever testes, executar comandos no ambiente disponível e preparar branches/PRs | Alterar `main`, ampliar escopo sem autorização, remover testes para obter CI verde ou declarar sucesso sem logs |
| GitHub Actions — validação automatizada | Compilar, executar testes e registrar resultados reproduzíveis | Ser interpretado como prova de compatibilidade geral quando executa apenas um teste específico |

Os modelos são colaboradores técnicos, não autoridades hierárquicas. Qualquer agente pode apontar erro no trabalho de outro. Evidência reproduzível prevalece sobre opinião ou confiança do modelo.

## 2. Fonte única de verdade e comunicação

Antes de qualquer trabalho, cada agente deve ler, nesta ordem:

1. `AI_COLLABORATION.md`
2. `PROJECT_LOG.md`
3. `ROADMAP.md`
4. `VALIDATION_MATRIX.md`
5. README e documentação da área afetada
6. Código, testes, workflows e histórico de PRs relacionados à tarefa

A comunicação entre agentes ocorre por artefatos versionados no GitHub:

- **Especificação:** registrada na descrição do PR ou em `docs/specs/<assunto>.md`.
- **Implementação:** branch dedicada e commits pequenos.
- **Evidência:** links de Actions, logs, artefatos e comandos executados.
- **Revisão:** comentários no PR e relatório de auditoria.
- **Estado do projeto:** atualização de `PROJECT_LOG.md` e `VALIDATION_MATRIX.md`, quando houver mudança comprovada.

Não usar conversa privada entre modelos como única fonte de contexto. Toda decisão que afete arquitetura, compatibilidade, segurança ou comportamento deve ser registrada no repositório.

## 3. Ciclo obrigatório de trabalho

1. **Intake:** descrever sintoma, impacto, reprodução, ambiente e resultado esperado.
2. **Diagnóstico:** identificar código e evidências; separar fato, hipótese e incógnita.
3. **Especificação:** delimitar arquivos, APIs, invariantes, riscos, testes e critérios de aceitação.
4. **Autorização:** o mantenedor confirma escopo antes de mudanças de runtime/arquitetura.
5. **Implementação:** uma tarefa lógica por branch; não misturar refatoração oportunista.
6. **Validação:** executar verificações estáticas, build e testes direcionados; acrescentar regressão.
7. **Revisão independente:** outro agente revisa diff, casos-limite e evidências.
8. **PR:** documentar o que mudou, o que não mudou, riscos, resultados e limitações.
9. **Decisão humana:** merge somente após aprovação explícita do mantenedor.
10. **Pós-merge:** atualizar log, matriz de validação e roadmap com o SHA integrado.

## 4. Regras invioláveis para evitar regressões

- Nunca fazer commit, push, rebase destrutivo ou edição direta em `main`.
- Toda alteração entra por branch e Pull Request direcionado a `main`.
- Nunca fazer merge sem autorização explícita do mantenedor, mesmo que todos os checks estejam verdes.
- Uma correção por branch/PR sempre que possível.
- Não combinar mudança funcional, refatoração ampla e atualização de dependências no mesmo PR.
- Não alterar APIs, ABI, layouts de estruturas, scheduler, memória, JIT, loader ou sincronização sem mapear consumidores e invariantes.
- Não reativar inicialização completa de DLLs Wine, redirecionar stubs NT ou introduzir dependência de wineserver sem análise específica e testes.
- Não editar arquivos gerados (por exemplo, `gl-funcs.js`) manualmente; localizar e alterar a fonte geradora.
- Não remover, enfraquecer ou contornar testes para obter CI verde.
- Não classificar instalador concluído como aplicativo funcional.
- Não classificar build bem-sucedido como compatibilidade funcional.
- Não afirmar validação sem apontar workflow, execução, commit SHA, resultado e escopo coberto.
- Se não houver ambiente para executar algo, declarar explicitamente **não executado**.
- Em caso de falha, preservar logs e classificar: código, teste, ambiente, rede/download, timeout ou inconclusivo.
- Se surgir necessidade de ampliar escopo, parar e pedir autorização antes de implementar.

## 5. Estados de evidência

Usar exatamente estas distinções nos relatórios:

- **Identificado:** há indício ou observação, ainda sem causa confirmada.
- **Diagnosticado:** causa sustentada por código, reprodução ou evidência técnica.
- **Implementado:** código alterado; não implica que compilou.
- **Compilado:** build terminou com sucesso para o SHA indicado.
- **Testado:** teste específico executou; informar qual.
- **Validado:** todos os critérios de aceitação definidos para a tarefa passaram.
- **Confirmado funcionalmente:** comportamento foi observado no ambiente de destino e nos cenários declarados.
- **Não validado / inconclusivo:** faltam evidências ou o ambiente impediu conclusão.

Nunca promover automaticamente um estado para o seguinte.

## 6. Contrato de uma tarefa para Claude Opus

Toda tarefa de implementação deve receber uma especificação com:

- ID e prioridade;
- problema e evidências;
- hipótese causal, com nível de confiança;
- resultado esperado;
- arquivos e componentes permitidos;
- arquivos/áreas proibidos;
- invariantes que não podem mudar;
- plano de implementação;
- testes novos e existentes;
- comandos exatos de build/test;
- critérios de aceitação;
- riscos e plano de rollback.

Ao terminar, Claude deve devolver:

1. resumo das alterações;
2. lista de arquivos e commits;
3. explicação de decisões;
4. comandos executados e seus resultados;
5. links dos workflows e artefatos;
6. testes que não conseguiu executar;
7. riscos conhecidos e dúvidas;
8. PR em draft, sem merge.

## 7. Contrato da revisão independente

O agente revisor deve ler o diff e verificar, no mínimo:

- correspondência com a especificação;
- alterações fora do escopo;
- erros de lógica, memória, concorrência, ABI e tratamento de falhas;
- compatibilidade com x86-64, WoW64 e navegadores suportados, quando aplicável;
- efeitos em single-core e multi-core;
- regressões nos testes existentes;
- qualidade dos novos testes (incluindo se falhariam antes da correção);
- segurança, dependências e licenças;
- se o relatório descreve fielmente os logs.

O revisor deve apontar bloqueadores concretos, não apenas dizer que o código parece bom. A revisão não substitui os checks automatizados nem a aprovação humana.

## 8. Política de branches e PRs

- Branches com prefixos: `fix/`, `feat/`, `test/`, `docs/`, `refactor/`.
- PRs devem ter escopo pequeno, descrição reproduzível e checklist de validação.
- Manter PRs em draft enquanto houver testes pendentes ou dúvidas relevantes.
- Atualizar a branch com a `main` apenas de modo não destrutivo e revisar novamente o diff.
- Não fechar ou substituir PR de outro agente sem explicar o motivo.
- Não empilhar dependências entre PRs sem declarar a ordem de integração.

## 9. Limites do ambiente

GitHub Actions é o ambiente de build/teste do projeto. Uma IA que apenas inspeciona o repositório não pode inferir que o build local passou. O resultado oficial é o workflow executado para o SHA exato.

Quando um workflow não cobre o comportamento real (por exemplo, um smoke test de API não comprova compatibilidade de aplicativos comerciais), o relatório deve dizer isso expressamente.

## 10. Registro de decisões

Decisões arquiteturais duradouras devem ser adicionadas a `DECISIONS.md` ou a um ADR em `docs/decisions/`, contendo contexto, alternativas, decisão, consequências e data. Não criar decisão arquitetural implícita apenas em comentário de código.

## 11. Critério de sucesso do processo

O processo é bem-sucedido quando reduz mudanças não relacionadas, torna os resultados reproduzíveis, deixa claro o que foi e não foi testado, permite revisão cruzada entre modelos e mantém a decisão final com o mantenedor humano.
