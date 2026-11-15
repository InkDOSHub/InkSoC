# InkSoC — Roadmap técnico orientado por evidências

Este roadmap organiza a sequência de trabalho; não autoriza automaticamente a implementação de todas as etapas. Cada etapa exige escopo, diagnóstico e aprovação do mantenedor.

## Princípios de priorização

1. Corrigir problemas reproduzíveis que bloqueiam inicialização ou execução.
2. Fortalecer testes que detectem regressões antes de ampliar compatibilidade.
3. Auditar limites de arquitetura com risco elevado (memória, scheduler, loader, ABI, JIT, sincronização).
4. Só então ampliar a matriz de aplicativos e otimizações.
5. Evitar grandes refatorações sem necessidade demonstrada.

## Etapa 0 — Estabilizar governança e estado atual

- [ ] Revisar e aprovar o protocolo multicêntrico em `AI_COLLABORATION.md`.
- [ ] Conferir os heads atuais e checks dos PRs #2, #3 e #4.
- [ ] Para cada PR, comparar o SHA testado com o SHA atual do head.
- [ ] Revisar o diff e os riscos antes de qualquer merge.
- [ ] Configurar proteção de `main` no GitHub: PR obrigatório, checks obrigatórios e restrição de push direto, se o plano/permissões permitirem.
- [ ] Atualizar este roadmap e o log com os resultados reais.

**Gate:** nenhuma tarefa nova de runtime deve se apoiar em status antigo ou SHA não confirmado.

## Etapa 1 — Contratos de inicialização e APIs fundamentais

- [ ] Revisar a correção de `RTL_USER_PROCESS_PARAMETERS` e `GetStartupInfoA/W` do PR #4.
- [ ] Confirmar layout de estruturas contra fontes upstream compatíveis e arquitetura x64.
- [ ] Verificar ponteiros A/W, ownership, lifetime e nulidade das strings.
- [ ] Ampliar testes para valores padrão, campos opcionais, processos filhos e casos sem console, conforme aplicável.
- [ ] Confirmar que o smoke test falha antes da correção e passa depois dela.
- [ ] Investigar a paridade de inicialização Wine sem reativar indiscriminadamente os DllMain.

**Gate:** build e testes direcionados passam no SHA exato; revisão independente sem bloqueadores; aprovação humana.

## Etapa 2 — Harness de compatibilidade reproduzível

- [ ] Revisar o tratamento de falhas de download, extração e spawn no PR #2.
- [ ] Garantir que cada aplicativo produza um resultado individual e classificável.
- [ ] Distinguir instalação, inicialização, execução, saída, timeout e falha de infraestrutura.
- [ ] Preservar logs e artefatos por aplicativo.
- [ ] Criar critérios de sucesso funcionais por aplicativo, sem inferir sucesso apenas pelo instalador.

**Gate:** testes do harness passam; resultados da matriz são interpretáveis e reproduzíveis.

## Etapa 3 — Auditoria de arquitetura de alto risco

Executar auditorias separadas, cada uma com relatório, invariantes e testes:

- [ ] CPU, JIT, SSE, x87, exceções e transições para interpretador.
- [ ] Loader PE, relocations, imports, TLS, inicialização de DLL e WoW64.
- [ ] TEB/PEB, estruturas NT, NLS, parâmetros de processo e estado de thread.
- [ ] MMU, TLB, swap/OPFS, proteção de memória e concorrência de acesso.
- [ ] Scheduler, waits, timers, locks, eventos, semáforos e deadlocks.
- [ ] Workers, shared memory, multi-core e comportamento em Safari/WebKit iOS.
- [ ] Segurança, dependências, proveniência e consistência de licenças.

**Gate:** nenhuma mudança ampla sem hipótese verificável, teste de regressão e plano de rollback.

## Etapa 4 — Compatibilidade de aplicações

- [ ] Manter uma matriz de aplicativos com versão, origem do instalador e ambiente.
- [ ] Separar falha de download de falha do engine.
- [ ] Registrar instalação, launch, execução, interface, I/O, rede e encerramento.
- [ ] Repetir testes críticos em builds identificados por SHA.
- [ ] Comparar single-core e multi-core quando o comportamento envolver concorrência.

**Gate:** declarar compatibilidade somente para versão, cenário e ambiente efetivamente testados.

## Etapa 5 — Desempenho e robustez

- [ ] Estabelecer benchmarks reproduzíveis antes de otimizar.
- [ ] Medir regressões de memória, latência, throughput e responsividade.
- [ ] Testar dispositivos e navegadores suportados, incluindo iPhone/iPad.
- [ ] Executar testes prolongados e cenários de pressão de memória.
- [ ] Documentar limites conhecidos.

**Gate:** melhoria mensurável sem perda nos testes funcionais e de estabilidade.

## Critérios para alterar prioridades

Uma etapa pode ser reordenada se houver bug crítico reproduzível, risco de segurança, bloqueio de build ou evidência nova. Registrar o motivo em `PROJECT_LOG.md`; não alterar prioridade apenas por preferência do agente.
