# InkSoC — Protocolo de colaboração por eventos

**Status:** proposta v1 para revisão  
**Baseline:** `d923a3eae86669d807b15d131400636e8137b74d`  
**Escopo:** documentação e contrato operacional; nenhum runtime, segredo, workflow de publicação ou configuração administrativa é alterado.

## 1. Objetivo

Organizar ciclos sucessivos entre GPT-6, Claude e GitHub. O processo não exige trabalho contínuo: cada participante atua quando recebe uma entrega ou evento concluído. O GitHub conserva o estado verificável e o mantenedor humano recebe notificações para reativar a sessão GPT-6 quando necessário.

## 2. Papéis

- **Mantenedor:** define prioridades, fornece autorizações e decide todo merge em `main`.
- **GPT-6:** investiga, compara alternativas, define especificações, audita diffs e evidências e propõe o próximo trabalho.
- **Claude:** implementa a tarefa especificada em branch isolada, executa os testes disponíveis e registra resultados.
- **GitHub:** mantém Issues, branches, PRs, commits, Actions, artefatos e histórico.
- **GitHub Actions:** executa validações determinísticas e publica resultados vinculados a SHAs.

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
2. GPT-6 publica uma especificação em Issue ou documento versionado, com critérios de aceitação.
3. Após autorização aplicável, Claude implementa apenas o escopo aprovado em branch própria.
4. Claude abre/atualiza PR em draft e registra arquivos, commits, decisões, testes e pendências.
5. GitHub Actions valida o SHA do PR; resultados devem permanecer associados ao commit exato.
6. Claude atualiza o relatório e `PROJECT_LOG.md` com fatos verificáveis.
7. Uma notificação informa ao mantenedor que há resultado novo, com links para PR e Actions.
8. O mantenedor abre o resultado e aciona GPT-6 nesta conversa; GPT-6 relê o estado do GitHub e realiza auditoria independente.
9. GPT-6 registra conclusão, solicita revisão ou publica a próxima tarefa.
10. Merge continua dependendo de autorização humana explícita.

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

Todo relatório de CI deve incluir workflow, run ID/URL, SHA, conclusão e jobs relevantes. Verde em um workflow não prova compatibilidade geral.

## 7. Notificações e acionamento

A notificação tem dois níveis diferentes:

1. **Notificação GitHub:** e-mails de Issues/PRs dependem das preferências de notificações e da participação/inscrição do usuário. E-mails de execuções Actions dependem das preferências do GitHub e não devem ser presumidos para qualquer execução iniciada por bot.
2. **Notificação dedicada:** se for necessário garantir e-mail para todo resultado do Claude, será preciso implementar um canal explícito (por exemplo, provedor SMTP/API) com credencial em GitHub Actions Secrets. O provedor, destinatário e segredo precisam ser configurados pelo mantenedor; nunca gravar credenciais no repositório.

O link no e-mail não desperta automaticamente esta sessão do ChatGPT. O mantenedor abre o link e envia uma mensagem para iniciar a auditoria. A análise desta sessão é distinta de um agente GPT autônomo via API.

## 8. Integração com Claude

O mecanismo exato de despacho automático para Claude ainda precisa ser identificado e validado no ambiente do mantenedor. O protocolo não presume que criar Issue, comentário ou evento `workflow_run` por si só acione Claude.

Antes de automatizar o despacho:
- identificar a integração efetivamente usada para Claude;
- confirmar quais eventos ela observa;
- confirmar permissões mínimas;
- impedir que comentários do bot disparem loops;
- exigir ID da tarefa e SHA-base;
- tornar o despacho idempotente.

Até essa validação, a tarefa deve ser encaminhada pelo mecanismo já utilizado pelo mantenedor, com link para a Issue/PR.

## 9. Proteções e limites

- Nunca editar `main` diretamente.
- Branch e PR separados para cada mudança lógica.
- Nunca fazer merge sem autorização explícita do mantenedor.
- Não alterar runtime, dependências, licenças ou workflows de publicação neste protocolo.
- Não reduzir/remover testes para obter CI verde.
- Não iniciar outra execução para a mesma tarefa enquanto houver uma execução ativa ou resultado sem auditoria.
- Limitar tentativas automáticas de correção; após o limite, marcar `BLOCKED`.
- Não expor tokens, segredos, conteúdo privado ou dados de terceiros em logs.
- Toda automação deve usar permissões mínimas e não pode publicar artefatos de PR em `main`.

## 10. Critério de conclusão

Uma rodada termina quando o relatório de Claude, os resultados de Actions e a auditoria GPT-6 estão associados ao mesmo ID de tarefa e aos SHAs correspondentes, e a próxima ação está registrada como `READY`, `BLOCKED`, `ACCEPTED` ou `CANCELLED`.

A aceitação técnica não implica integração. A decisão final de merge pertence ao mantenedor.
