# InkSoC — Benchmarks e ambiente de referência

> Índice e documentação de ambiente dos benchmarks experimentais do InkSoC.
> Registro de 15/11/2026. Documento somente de documentação: não altera código de produção, workflows ou configurações do engine.

## Índice

| Documento | Conteúdo |
|---|---|
| [2026-11-15-iphone-17-pro-max-safari.md](2026-11-15-iphone-17-pro-max-safari.md) | Resultados observados (CPU, textura, armazenamento, memória, WebGPU, Rufus, leitura de C:, swap/OPFS), interpretação, limitações e próximos testes. |

## Ambiente de execução informado pelo usuário

Estes dados foram confirmados pelo usuário e registrados como ambiente informado. Não foram verificados de forma independente nesta rodada.

| Item | Valor | Origem |
|---|---|---|
| Dispositivo | Apple iPhone 17 Pro Max | Informado pelo usuário |
| Navegador | Safari | Informado pelo usuário |
| Plataforma | iOS | Informado pelo usuário |
| Ambiente de execução | InkSoC via WebAssembly | Informado pelo usuário |
| Versão do iOS | não informada | Não será inferida |
| Versão do Safari | não informada | Não será inferida |
| RAM física total do aparelho | não informada | Não será inferida |

## Memória: valores que não devem ser confundidos

| Grandeza | Valor | Observação |
|---|---|---|
| RAM real configurada pelo InkSoC (disponível aos programas) | 384 MB | Configuração experimental atual |
| Memória WASM observada | aproximadamente 437–438 MB | Varia conforme a execução |
| Memória-alvo apresentada ao Windows | 8.192 MB (8 GB) | Valor apresentado ao programa guest, não RAM do aparelho |
| Limite de recarga da aba, execução 1 | aproximadamente 769 MB | Observação pontual, não limite garantido |
| Limite de recarga da aba, execução 2 | aproximadamente 1.281 MB | Observação pontual, não limite garantido |
| Memória física total do iPhone | não informada | Não deve ser deduzida dos valores acima |

Os 384 MB devem ser lidos como configuração experimental atual da RAM emulada, não como limite definitivo da arquitetura. A diferença entre 384 MB e a memória WASM observada (437–438 MB) não foi explicada pelos dados disponíveis; qualquer causa é hipótese não confirmada.

## Versão testada

- **Versão testada:** não identificada.
- **Referência de trabalho:** INKSOC-2026-015, commit `e8f9844004a260e043798fb528224ae9c72fd365` (branch `fix/startup-info-structure`).
- Não há comprovação, nos dados recebidos, de que os benchmarks listados foram executados nesse commit. Nenhum resultado deve ser associado a ele sem evidência adicional.

## Convenções deste diretório

- Valores reproduzem exatamente o que foi informado, com unidades originais.
- FPS derivado de tempo por frame é estimativa, e está marcado como tal.
- Estados de evidência seguem `docs/collaboration/PROTOCOL.md`: identificado, diagnosticado, implementado, compilado, testado, validado e confirmado funcionalmente. Para estes benchmarks, usa-se: comprovado pela execução informada, parcialmente comprovado, não testado e bloqueado por limitação conhecida.
- Logs brutos não foram anexados porque não estão disponíveis no ambiente de trabalho desta rodada. Nenhum log foi reconstruído ou inventado.
