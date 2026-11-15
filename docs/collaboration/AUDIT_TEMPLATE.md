# Auditoria independente — <ID>

- **Estado:** AUDIT_PENDING
- **Executor:** <agente>
- **Revisor:** GPT-6
- **SHA-base:** <commit>
- **SHA auditado:** <commit completo>
- **PR:** <URL>
- **Actions:** <URLs e run IDs>

## 1. Resumo
<Conclusão curta, sem confundir implementação com validação.>

## 2. Correspondência com a especificação
- [ ] Escopo atendido.
- [ ] Nenhuma alteração proibida.
- [ ] Critérios de aceitação conferidos individualmente.

## 3. Revisão técnica
- Lógica e casos-limite:
- Memória, ownership e lifetime:
- Concorrência e sincronização:
- ABI, estruturas e compatibilidade:
- Tratamento de falhas:
- Segurança e dependências:

## 4. Testes e evidências
| Verificação | SHA | Run/artefato | Resultado | Cobertura real |
|---|---|---|---|---|
| <nome> | <SHA> | <URL> | <resultado> | <o que prova> |

## 5. Achados
| Severidade | Arquivo/linha | Achado | Evidência | Ação |
|---|---|---|---|---|
| <bloqueador/alto/médio/baixo> | <local> | <descrição> | <referência> | <correção> |

## 6. Limitações e incógnitas
<Declarar explicitamente o que não foi possível confirmar.>

## 7. Decisão da auditoria
- **ACCEPTED:** critérios técnicos atendidos; não autoriza merge.
- **REVISION_REQUIRED:** listar correções específicas.
- **BLOCKED:** indicar dependência/decisão humana necessária.

## 8. Próxima tarefa
<ID, objetivo, escopo proposto e critério de conclusão.>
