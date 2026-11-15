# InkSoC — Protocolo de colaboração humano-assistida

**Status:** proposta v1 para revisão (revisada para o fluxo real em 2026-11-15)  
**Baseline:** `d923a3eae86669d807b15d131400636e8137b74d`  
**Escopo:** documentação e contrato operacional; nenhum runtime, segredo, workflow de publicação ou configuração administrativa é alterado.

## 1. Objetivo

Organizar ciclos sucessivos entre GPT-6, o mantenedor, Claude Code e GitHub. O fluxo atual é **humano-assistido**: nenhum participante é acionado automaticamente por eventos do GitHub. Cada passo começa quando o mantenedor leva a entrega anterior ao próximo participante. O GitHub conserva o estado verificável (branches, commits, PRs, Actions) que serve de evidência para todos.

## 2. Papéis

- **Mantenedor (usuário):** define prioridades, dá autorizações, decide todo merge em `main` e é quem transporta as tarefas e os resultados entre GPT-6 e Claude Code.
- **GPT-6 (coordenador técnico):** analisa o projeto, planeja tarefas, define escopo, critérios de aceitação e riscos, e abre uma thread para encaminhar o trabalho. Depois faz a auditoria independente.
- **Claude Code (executor):** recebe a tarefa do mantenedor e trabalha numa máquina na nuvem com o repositório clonado. Implementa somente o escopo aprovado em branch isolada, abre/atualiza PR e registra resultados.
- **GitHub:** mantém Issues, branches, PRs, commits, Actions, artefatos e histórico.
- **GitHub Actions:** executa os workflows existentes do projeto (build, testes, validações) e publica resultados vinculados a SHAs. Nenhum workflow atual aciona Claude ou GPT-6.

### 2.1 Como Claude Code acessa o GitHub

1. **Git** (clone, fetch, push) com a conta autorizada para a sessão, somente nas branches liberadas para aquela sessão (normalmente `claude/...`, ou a branch de um PR existente quando o mantenedor autoriza explicitamente).
2. **GitHub Connector do Claude**, conectado à conta do mantenedor, para consultar PRs, Actions e logs, abrir PRs e executar outras operações permitidas.

Merge feito por esse acesso só acontece com autorização explícita do mantenedor.

Nenhum agente pode aprovar o próprio trabalho como revisão independente.

## 3. Unidade de trabalho e identidade

Cada ciclo recebe um identificador estável, por exemplo `INKSOC-2026-001`. O identificador deve aparecer na Issue, branch, PR e entrada de log.

Cada entrega deve declarar:
- ID da tarefa e tarefa predecessora;
- SHA-base e SHA do resultado;
- agente executor e agente revisor;
- escopo permitido e áreas proibidas;
- estado atual e próximo estado;
- evidências e limitações.

Não usar apenas nomes de branch ou texto de conversa como identidade da tarefa.

## 4. Estados do ciclo

`PROPOSED → READY → IMPLEMENTING → VALIDATING → REPORTING → AUDIT_PENDING`

A partir da auditoria:
- `REVISION_REQUIRED`: correção solicitada, retorna a `READY` com nova revisão da tarefa;
- `ACCEPTED`: critérios técnicos atendidos, sem autorizar merge;
- `BLOCKED`: depende de decisão, credencial, ambiente ou ação humana;
- `CANCELLED`: encerrado explicitamente.

A transição deve ser registrada no GitHub. Não iniciar implementação enquanto a tarefa não estiver em `READY`.

## 5. Ciclo de execução

1. GPT-6 verifica o estado atual do repositório, os PRs relacionados, os workflows e os SHAs.
2. GPT-6 escreve a tarefa (escopo, critérios de aceitação, riscos) e abre uma thread para encaminhá-la.
3. O mantenedor transfere a tarefa para Claude Code (manualmente; não há despacho automático).
4. Claude Code implementa apenas o escopo aprovado, em branch isolada, na máquina na nuvem com o repositório clonado.
5. Claude Code faz push da branch e abre/atualiza PR em draft, registrando arquivos, commits, decisões, testes e pendências.
6. GitHub Actions executa os workflows existentes; os resultados ficam associados ao SHA exato.
7. Claude Code atualiza o relatório e `PROJECT_LOG.md` (na branch de trabalho) com fatos verificáveis e entrega o resultado ao mantenedor.
8. O mantenedor volta à conversa com GPT-6 levando os resultados e as evidências (links de PR, commits e Actions).
9. GPT-6 faz auditoria independente, relendo o estado real no GitHub (commits, PRs, resultados dos workflows, aderência aos critérios). O relato de Claude não substitui essa verificação.
10. GPT-6 registra conclusão, solicita revisão ou propõe a próxima tarefa.
11. Merge continua dependendo de autorização humana explícita.

## 6. Contrato de evidência

Usar os estados sem promover automaticamente um ao outro:
- **Identificado:** observação ainda sem causa confirmada.
- **Diagnosticado:** causa sustentada por código, reprodução ou evidência.
- **Implementado:** código alterado.
- **Compilado:** build passou no SHA indicado.
- **Testado:** teste nomeado foi executado.
- **Validado:** critérios de aceitação da tarefa passaram.
- **Confirmado funcionalmente:** comportamento observado no ambiente de destino e nos cenários declarados.
- **Inconclusivo:** evidência insuficiente ou ambiente impeditivo.

Todo relatório de CI deve incluir workflow, run ID/URL, SHA, conclusão e jobs relevantes. Verde em um workflow não prova compatibilidade geral. Não declarar teste aprovado sem conferir o resultado do GitHub Actions no SHA exato do commit analisado.

Classificar falhas como **CODE**, **TEST**, **INFRA**, **NETWORK**, **TIMEOUT**, **ENVIRONMENT** ou **UNKNOWN**.

## 7. Notificações e acionamento

No fluxo atual não há notificação automática nem acionamento automático de agentes:

- O mantenedor acompanha os resultados pelo próprio GitHub (PRs, Actions) e pelo relatório que Claude Code entrega na thread.
- E-mails nativos do GitHub dependem das preferências de notificação da conta e não devem ser presumidos, em especial para execuções iniciadas por bot.
- Um link aberto pelo mantenedor não desperta GPT-6 nem Claude Code. Cada etapa começa quando o mantenedor envia uma mensagem ao participante seguinte.

Um canal de e-mail dedicado (provedor SMTP/API com credencial em Actions Secrets) é apenas uma possibilidade futura; ver `INTEGRATION_DESIGN.md`.

## 8. Integração com Claude

Estado atual (verificado em 2026-11-15):

- Não existe Claude Code GitHub Action instalada no InkSoC.
- Nenhum workflow em `.github/workflows` aciona Claude.
- Um comentário `@claude` em PR ou Issue **não** inicia nenhuma execução.
- Os workflows existentes servem a build, testes e validações do projeto, não ao despacho de tarefas.

Claude Code é acionado somente pelo mantenedor, que transfere a tarefa para uma sessão na nuvem (seção 2.1).

**Possibilidade futura (não implementada):** a Claude Code GitHub Action permitiria acionar Claude a partir de PRs/Issues. A adoção exigiria chave de API da Anthropic como Secret do repositório, custos próprios e revisão das permissões de execução, em PR separado e com autorização do mantenedor. Antes disso, o protocolo não presume que nenhum evento do GitHub acione Claude.

## 9. Proteções e limites

- Nunca editar `main` diretamente.
- Toda implementação em branch isolada, entregue por Pull Request; branch e PR separados para cada mudança lógica.
- Nunca fazer merge sem autorização explícita do mantenedor.
- Não ampliar o escopo autorizado.
- Não alterar runtime, dependências, licenças ou workflows de publicação neste protocolo.
- Não reduzir/remover testes para obter CI verde.
- Não iniciar outra execução para a mesma tarefa enquanto houver uma execução ativa ou resultado sem auditoria.
- Limitar tentativas automáticas de correção; após o limite, marcar `BLOCKED`.
- Não expor tokens, segredos, conteúdo privado ou dados de terceiros em logs.
- Toda automação deve usar permissões mínimas e não pode publicar artefatos de PR em `main`.

## 10. Critério de conclusão

Uma rodada termina quando o relatório de Claude, os resultados de Actions e a auditoria GPT-6 estão associados ao mesmo ID de tarefa e aos SHAs correspondentes, e a próxima ação está registrada como `READY`, `BLOCKED`, `ACCEPTED` ou `CANCELLED`.

A aceitação técnica não implica integração. A decisão final de merge pertence ao mantenedor.
