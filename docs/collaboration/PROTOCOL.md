# InkSoC — protocolo de desenvolvimento e validação

**Status:** protocolo operacional vigente  
**Atualizado:** 2026-11-16

## 1. Objetivo

Conduzir o InkSoC com entregas pequenas, verificáveis e seguras. O GitHub é a fonte de verdade do código, histórico, branches, Pull Requests e resultados automatizados. Este protocolo reflete a condução técnica direta por GPT-6; não depende de outro agente executor ou de créditos externos.

## 2. Papéis

- **Mantenedor humano:** define prioridades, autoriza o escopo geral, testa no Safari/iPhone/iPad quando necessário e decide sobre merges e publicações.
- **GPT-6 — responsável técnico:** investiga arquitetura e código, define tarefas e critérios, implementa em branches, cria/atualiza PRs, acompanha Actions, revisa os próprios diffs com verificações adicionais e relata honestamente o que não pôde executar.
- **GitHub Actions:** executa os workflows configurados; cada resultado vale somente para o SHA e o escopo testados.

GPT-6 não tem acesso implícito ao Safari, aos arquivos locais do mantenedor, ao runtime do iPhone ou a ambientes externos não conectados. Não afirmar testes locais/dispositivo que não tenham sido executados.

## 3. Ciclo de trabalho

1. Confirmar HEAD de `main`, branches, PRs relacionados, estado de workflows e Graphify/contexto técnico.
2. Formular hipótese causal separando fatos observados de inferências.
3. Delimitar uma alteração lógica, arquivos afetados, invariantes, riscos e critérios de aceitação.
4. Criar branch a partir do HEAD verificado de `main`; implementar sem alterar diretamente a branch principal.
5. Executar testes e verificações disponíveis. Não enfraquecer ou remover testes para obter resultado verde.
6. Revisar o diff, procurar regressões, validar a origem dos dados e conferir dependências/licenças.
7. Abrir ou atualizar PR com resumo, SHA-base, evidências e limitações.
8. Conferir Actions no SHA exato e atualizar o mantenedor.
9. Após validação funcional no dispositivo quando aplicável e autorização específica, integrar a mudança.
10. Registrar marcos técnicos de forma concisa e anonimizada.

## 4. Escopo da autorização e operações protegidas

A autorização do mantenedor para conduzir o projeto permite investigação, edição de código/documentação em branches, execução de testes não destrutivos e criação de PRs.

Exigem autorização específica adicional:
- merge em `main` e publicação manual;
- exclusão de dados do usuário ou de armazenamento persistente;
- reescrita de histórico, force-push ou remoção de branches/PRs;
- alterações em segredos, tokens, permissões, configurações administrativas, faturamento ou infraestrutura;
- operações destrutivas ou testes que possam corromper dados reais.

Nunca presumir autorização para uma dessas operações por causa de uma autorização geral de desenvolvimento.

## 5. Contrato de evidência

Usar os seguintes estados sem promoção automática:
- **Identificado:** observação ou indício, sem causa confirmada.
- **Diagnosticado:** causa sustentada por código, reprodução ou evidência.
- **Implementado:** código alterado; não implica build.
- **Compilado:** build passou no SHA indicado.
- **Testado:** teste nomeado foi executado.
- **Validado:** critérios de aceitação passaram.
- **Confirmado funcionalmente:** comportamento observado no ambiente de destino, nos cenários declarados.
- **Inconclusivo:** faltam evidências ou o ambiente impediu a conclusão.

Todo relatório de CI deve indicar workflow, run ID/URL, SHA, conclusão e jobs relevantes. Um check verde prova apenas o que o workflow realmente executou.

Classificar falhas como **CODE**, **TEST**, **INFRA**, **NETWORK**, **TIMEOUT**, **ENVIRONMENT** ou **UNKNOWN**.

## 6. Confiabilidade e segurança

- Preservar dados, compatibilidade e comportamento existente; alterações devem ter escopo e plano de regressão.
- Não contornar erros de rede, memória, arquivos ou inicialização com falsos estados de sucesso.
- Não apresentar métricas indisponíveis como zero ou valores simulados como medidas reais.
- Tratar arquivos do usuário como dados não confiáveis; validar tamanho, formato, origem e limites de memória.
- Proteger handoffs entre apps com identificadores não previsíveis, expiração, leitura/consumo controlados e mensagens de erro explícitas.
- Respeitar isolamento entre origens e CSP; não abrir acesso geral a armazenamento privado entre aplicativos.
- Fixar dependências e manter proveniência/licenças de código de terceiros.
- Testes de estresse devem ter limites de memória, tempo e volume, e não podem preencher deliberadamente todo o armazenamento persistente do usuário.

## 7. Privacidade e registros públicos

O repositório é público. Portanto:
- Não publicar transcrições de conversas, resumos pessoais, dados de saúde, localização, contatos, identificadores de conta, e-mails, tokens, credenciais, caminhos locais ou nomes privados de arquivos.
- Contexto de sessão e logs devem conter somente fatos técnicos necessários, anonimizados e minimizados.
- Usar dados e arquivos sintéticos em Issues, PRs, testes e screenshots.
- Não enviar arquivos reais do usuário para serviços externos ou anexá-los ao GitHub.
- Não afirmar que o histórico Git está anonimizado sem auditá-lo: uma correção no arquivo atual não remove versões anteriores já publicadas.
- Se um segredo for encontrado, não reproduzi-lo no relatório; interromper o uso, avaliar exposição e orientar rotação/revogação pelo canal seguro.

## 8. Arquitetura e integração com o InkDOS Hub

Antes de criar um novo visualizador/editor, verificar as soluções existentes no ecossistema InkDOS Hub. Reutilizar contratos e componentes quando forem compatíveis, preservando licenças, isolamento e experiência em Safari.

A integração de arquivos deve distinguir:
- seleção/abertura;
- leitura e visualização;
- edição;
- salvamento/exportação;
- sincronização segura com o drive C: virtual.

A existência de um launcher ou handoff em outro aplicativo não comprova que o InkSoC possa acessar diretamente seus dados. Definir uma ponte explícita, com permissões e ciclo de vida documentados.

## 9. Critério de conclusão

Uma tarefa termina quando:
- o diff corresponde ao escopo;
- testes e Actions estão vinculados ao SHA exato;
- limitações e riscos foram declarados;
- validação funcional foi obtida quando necessária, ou marcada como pendente;
- a próxima ação está clara.

Aceitação técnica não substitui a autorização humana para merge/publicação.
