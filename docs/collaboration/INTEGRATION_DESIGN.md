# Desenho de integração — eventos, Claude e notificações

**Status:** desenho técnico de uma possível automação **futura**; não implementa despacho nem envio de e-mail.

## 0. Estado atual (o que existe hoje)

O fluxo vigente é humano-assistido e está descrito em `PROTOCOL.md`: GPT-6 planeja e abre uma thread, o mantenedor transfere a tarefa para Claude Code (sessão na nuvem com o repositório clonado, acesso por Git e pelo GitHub Connector), GitHub Actions executa os workflows existentes, e o mantenedor leva os resultados de volta a GPT-6 para auditoria independente.

Hoje **não** existe:
- Claude Code GitHub Action instalada no InkSoC;
- workflow que acione Claude ou GPT-6;
- efeito de comentários `@claude` em PRs ou Issues;
- despacho por evento, idempotência de despacho ou notificação por e-mail dedicada.

Tudo o que segue descreve requisitos para uma eventual automação e não deve ser lido como funcionalidade implementada.

## 1. Eventos pretendidos (futuro)

| Evento | Resultado esperado | Consumidor |
|---|---|---|
| Tarefa passa para READY | Despachar uma implementação, uma única vez | Adaptador Claude |
| PR/commit do Claude atualizado | Executar validações aplicáveis | GitHub Actions |
| Workflow de validação concluído | Consolidar conclusão e links | Relatório de ciclo |
| Relatório marcado como pronto | Notificar mantenedor | GitHub Notifications ou provedor dedicado |
| GPT-6 registra auditoria | Criar próxima tarefa ou solicitar correção | GitHub / adaptador Claude |

## 2. Idempotência e prevenção de loops

Cada despacho deve usar chave idempotente composta por ID da tarefa, SHA-base e tipo de operação. Repetir um webhook não pode criar duas implementações. Comentários produzidos por bots não devem ser tratados como comandos humanos. Uma execução não deve disparar a si mesma indefinidamente.

## 3. Contrato mínimo de evento

- `task_id`
- `event_type`
- `repository`
- `issue_or_pr`
- `base_sha`
- `head_sha`, quando houver
- `actor`
- `timestamp`
- `idempotency_key`
- `result_url`, quando houver

## 4. Notificação

### Opção A — notificações nativas do GitHub
Usar Issues/PRs como local de registro e configurar Watch/Participating e preferências de e-mail do GitHub. Validar na conta do mantenedor que os eventos desejados realmente chegam por e-mail. Não assumir que todo workflow iniciado por um bot gera e-mail.

### Opção B — e-mail explícito via Actions
Usar um provedor SMTP/API escolhido pelo mantenedor, com credenciais armazenadas em Actions Secrets. O workflow deve enviar apenas metadados e links, não logs secretos. A credencial não pode aparecer em argumentos, artefatos ou saída do job. Incluir tratamento de falha de envio e não bloquear a validação técnica por falha de notificação.

A escolha do provedor e a configuração de Secrets são pré-requisitos para implementar a opção B.

## 5. Acionamento de Claude

**Hoje:** manual. O mantenedor transfere a tarefa para uma sessão do Claude Code na nuvem; nenhum evento do GitHub inicia Claude.

**Possibilidade futura:** Claude Code GitHub Action (acionada por menção em PR/Issue ou por workflow). Pré-requisitos antes de adotar:
- chave de API da Anthropic armazenada como Secret do repositório, configurada pelo mantenedor;
- custos próprios, separados da assinatura usada nas sessões atuais;
- revisão das permissões do workflow (mínimas, sem push em `main`);
- proteção contra loops (comentários de bot não acionam execução) e idempotência (seção 2);
- PR separado, com autorização explícita do mantenedor.

## 6. Acionamento GPT-6

**Hoje:** human-in-the-loop. O mantenedor volta à conversa com GPT-6 levando os resultados e links (PR, commits, Actions); GPT-6 relê o estado do GitHub e audita. Um agente GPT via API seria uma integração separada, com credenciais, custos, limites e política próprios.

## 7. Sequência segura de implantação (se a automação for aprovada no futuro)

1. Aprovar o protocolo e os templates.
2. Decidir se a Claude Code GitHub Action será adotada (Secret, custo, permissões) e testá-la manualmente em PR separado.
3. Configurar/validar notificações GitHub nativas.
4. Se necessário, escolher provedor de e-mail e cadastrar Secrets.
5. Implementar workflows pequenos, com permissões mínimas, em PR separado.
6. Testar sucesso, falha, evento duplicado, evento de bot, ausência de secret e timeout.
7. Somente depois habilitar despacho automático.
