# Desenho de integração — eventos, Claude e notificações

**Status:** desenho técnico; não implementa despacho nem envio de e-mail.

## 1. Eventos pretendidos

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

A integração Claude deve ser descoberta antes de escrever o workflow de dispatch. É necessário confirmar se ela observa Issues, labels, comentários, workflow_dispatch, webhook externo ou outro evento. Não presumir que um evento GitHub genérico inicia uma sessão Claude.

## 6. Acionamento GPT-6

O fluxo inicial é human-in-the-loop: o e-mail contém o link do PR/Actions; o mantenedor abre o link e envia uma mensagem a GPT-6. Isso inicia a análise nesta conversa. Um agente GPT via API seria uma integração separada, com credenciais, custos, limites e política próprios.

## 7. Sequência segura de implantação

1. Aprovar o protocolo e os templates.
2. Confirmar qual integração dispara Claude e executar um teste manual.
3. Configurar/validar notificações GitHub nativas.
4. Se necessário, escolher provedor de e-mail e cadastrar Secrets.
5. Implementar workflows pequenos, com permissões mínimas, em PR separado.
6. Testar sucesso, falha, evento duplicado, evento de bot, ausência de secret e timeout.
7. Somente depois habilitar despacho automático.
