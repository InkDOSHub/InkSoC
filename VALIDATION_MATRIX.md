# InkSoC — Matriz de validação

Esta matriz define o que cada tipo de validação comprova e o que não comprova. Atualizar com links de Actions e SHAs exatos.

## Níveis de validação

| Nível | Verificação | O que comprova | O que não comprova |
|---|---|---|---|
| V0 | Revisão estática do diff | Escopo, consistência e riscos visíveis no código | Que compila ou executa |
| V1 | Formatação, lint, sintaxe, JSON/config | Validade estrutural das entradas verificadas | Funcionamento do engine |
| V2 | Build do engine | Compilação para o target e configuração indicados | Execução correta de programas |
| V3 | Teste unitário / smoke test | Cenário e asserts explicitamente cobertos | Compatibilidade geral |
| V4 | Teste de integração no InkSoC | Fluxo integrado específico no runtime testado | Todos os aplicativos ou dispositivos |
| V5 | Compatibilidade de aplicativo | Aplicativo, versão, configuração e ambiente testados | Compatibilidade universal ou de versões diferentes |
| V6 | Teste de estabilidade/performance | Métricas e duração registradas no ambiente descrito | Ausência absoluta de falhas |

## Gates mínimos por tipo de alteração

| Tipo de mudança | Mínimo exigido antes de solicitar aprovação |
|---|---|
| Documentação apenas | Revisar links, nomes de arquivos e afirmações contra o estado atual |
| Runner / scripts de teste | Sintaxe, validação de manifestos, testes de falhas e um caso controlado de execução |
| Rust engine / NT API | Build WASM release, teste direcionado, regressão relevante e revisão independente |
| Loader / PE / DLL / TEB / PEB | Build, testes de estrutura/layout e inicialização, casos positivos e negativos |
| CPU / JIT / SSE / x87 / exceções | Build, testes de instruções e flags, comparação JIT vs interpretador quando possível |
| Memória / MMU / swap | Build, limites/alinhamento, pressão de memória, persistência e concorrência |
| Scheduler / locks / multi-core | Testes concorrentes, timeout/wait, deadlock e repetição para detectar flakiness |
| Browser / workers / WebKit | Testes de integração em browser e dispositivo alvo; não inferir iOS a partir de Linux |
| Dependências / licenças | Proveniência, versão, licença, compatibilidade e impacto no bundle |

O gate mínimo não impede testes adicionais quando o risco exigir.

## Regras para registrar resultados

Toda evidência deve conter:

- nome do workflow e job;
- URL da execução;
- commit SHA efetivamente testado;
- branch e evento que dispararam o workflow;
- status/conclusão;
- comandos e versões relevantes;
- artefatos/logs;
- asserts e cenários cobertos;
- limitações conhecidas.

**Atenção:** um check verde no SHA anterior não valida automaticamente commits posteriores. Conferir o SHA do workflow contra o head SHA atual do PR.

## Matriz inicial observada — 2026-11-15

| Área | Evidência observada | Estado inicial | Limite |
|---|---|---|---|
| Build/publicação de engine | [Pages run #43723372650](https://github.com/InkDOSHub/InkSoC/actions/runs/43723372650) — concluído com sucesso | Passou para o commit do workflow | Não prova compatibilidade de aplicativos |
| Runner de compatibilidade | PR #2 registra check de sintaxe Python e JSON concluído com sucesso | Passou para os checks descritos no PR | Não prova execução funcional da matriz inteira |
| Inicialização / STARTUPINFO | [startup-smoke run #43726419180](https://github.com/InkDOSHub/InkSoC/actions/runs/43726419180) — observado como concluído com sucesso | Passou para o SHA executado nessa run | Conferir SHA exato; smoke test limitado a STARTUPINFO A/W, desktop, handles e linha de comando |
| Auditoria de Wine startup | PR #3 contém documentação de auditoria | Documentado | Não é teste executável nem paridade integral |

## Template de resultado por tarefa

```markdown
### <ID> — <nome>
- PR:
- Branch:
- Head SHA:
- Workflow/job:
- URL da execução:
- SHA testado:
- Build: passou / falhou / não executado
- Testes direcionados: passou / falhou / não executado
- Regressões: passou / falhou / não executado
- Integração: passou / falhou / não executado
- Ambiente:
- Cenários cobertos:
- Cenários não cobertos:
- Falhas externas / limitações:
- Estado final:
- Revisor independente:
- Aprovação do mantenedor:
```

## Classificação de falhas

- **CODE:** erro reproduzível no engine ou na alteração.
- **TEST:** teste incorreto, instável ou insuficiente.
- **INFRA:** runner, dependência, máquina ou serviço indisponível.
- **NETWORK:** download, DNS, HTTP, rate limit ou rede.
- **TIMEOUT:** limite de tempo atingido; não presumir causa.
- **ENV:** diferença de sistema operacional, browser ou configuração.
- **UNKNOWN:** evidência insuficiente para classificar.

Uma falha externa não deve ser contabilizada como bug do engine sem evidência. Da mesma forma, falha de infraestrutura não transforma um teste incompleto em sucesso.
