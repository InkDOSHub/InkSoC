# Benchmarks InkSoC — iPhone 17 Pro Max / Safari (registro de 15/11/2026)

> Ambiente, memória e versão: ver [README.md](README.md).
> Todos os valores abaixo são **resultados informados pelo usuário**, registrados como observados. Não foram reexecutados nesta rodada.
> Versão testada: **não identificada**. Não há comprovação de que estes benchmarks foram executados no commit `e8f9844004a260e043798fb528224ae9c72fd365` (INKSOC-2026-015).

## 1. Escopo e limites desta documentação

- Documento somente de registro. Não altera código de produção, workflows, configurações do engine ou subsistema de swap.
- Não calcula percentual global de conclusão do InkSoC.
- Os benchmarks não foram comparados com equipamentos de referência nem com execução nativa.

## 2. CPU — 7-Zip 23.01 x64

Configuração informada: 4 processadores; Windows apresentado 10.0 19045; RAM apresentada 8.192 MB; RAM real configurada 384 MB; memória WASM aproximadamente 438 MB.

| Medição | Velocidade | Utilização | MIPS R/U | Rating |
|---|---|---|---|---|
| Compressão | 1.567 KiB/s | 234% | 596 | 1.396 MIPS |
| Descompressão | 24.159 KiB/s | 370% | 527 | 1.951 MIPS |
| Total | — | 302% | 561 | 1.674 MIPS |

- **Rating final informado pelo engine:** 1.936 MIPS.

**Observado:** os valores acima, como informados.

**Interpretação:** os valores de MIPS vêm de métricas distintas: 1.936 MIPS é o rating final do engine e 1.674 MIPS é o total do 7-Zip. **Não são diretamente comparáveis** e não devem ser apresentados como equivalentes.

**Limitações:** metodologia de medição, tamanho do conjunto de dados e número de execuções não foram informados.

## 3. Gráfico — streaming de texturas

- Pool de GPU: 188 MB.
- Texturas residentes: 47.
- Texturas armazenadas em disco: 2.048 MB.
- Duração: 30 s.
- FPS observados: 59 no primeiro registro e 60 nos cinco seguintes.
- FPS médio informado: 60.
- Pior frame: 33 ms.
- Dados transferidos do disco: 3.780 MB.
- Taxa de streaming informada: 126 MB/s (consistente com 3.780 MB em 30 s).
- Draws que aguardaram textura: 3,4%.

**Observado:** os valores acima.

**Interpretação:** neste cenário, a taxa média de quadros se manteve próxima de 60 FPS, com um pior frame de 33 ms.

**Limitações:** este teste **não comprova** compatibilidade geral com jogos nem equivalência a GPU nativa. Não foi informado o conteúdo da cena, a resolução nem a metodologia de medição do FPS.

## 4. Armazenamento e páginas de memória

| Medição | Valor |
|---|---|
| Escrita sequencial | 1.522 MB/s |
| Leitura sequencial | 1.735 MB/s |
| Página de 4 KB, leitura | 0,9 µs |
| Página de 4 KB, escrita | 3,3 µs |
| Taxa de páginas informada | 1.136.364 páginas/s |

**Interpretação:** 1.136.364 páginas/s × 4 KB ≈ 4,65 GB/s de tráfego teórico (decimal). Esse número é uma **conta derivada**, não uma medição sequencial. Não coincide diretamente com as velocidades sequenciais acima e deve ser tratado como medição separada.

**Limitações:** não foram informados método, cache, número de threads nem condições da medição.

## 5. Memória e limites do navegador

| Execução | Recarga da aba | RAM real disponibilizada aos programas |
|---|---|---|
| 1 | aproximadamente 769 MB | 384 MB |
| 2 | aproximadamente 1.281 MB | 384 MB |

**Observado:** a aba recarregou nas duas execuções, nos valores indicados.

**Limitações:** 769 MB e 1.281 MB são limites **observados em execuções específicas**. Não são limites garantidos do navegador nem do InkSoC. A diferença entre as duas execuções não foi explicada.

**Decisão a registrar:** manter 384 MB de RAM real é uma **configuração experimental atual**, não um limite definitivo da arquitetura.

## 6. WebGPU — tempo por frame

| Draws por frame | Tempo por frame (informado) | FPS equivalente (estimativa) |
|---|---|---|
| 500 | 0,86 ms | ≈ 1.163 |
| 1.000 | 0,98 ms | ≈ 1.020 |
| 2.000 | 1,76 ms | ≈ 568 |
| 4.000 | 3,43 ms | ≈ 292 |
| 8.000 | 6,51 ms | ≈ 154 |

**Observado:** tempo por frame, como informado.

**Estimativa:** a coluna FPS equivalente é a conversão 1.000 / tempo por frame. **Não é FPS efetivamente apresentado.**

**Interpretação:** o tempo por frame cresce aproximadamente de forma linear com o número de draws neste intervalo (de 500 a 8.000).

**Limitações:** não foi informado se as medições incluem sincronização de GPU, rasterização, shaders, apresentação ou espera pelo dispositivo. Não há conclusão sobre o custo de cada componente.

## 7. Rufus 4.15.2396 x64 — inicialização

**Observado:**

- Inicialização concluída.
- Windows 10 Pro x64, build 19045.3803, detectado.
- Syslinux 4.07 e 6.04/pre1 detectados.
- GRUB 0.4.6a e 2.14 detectados.
- Configurações do registro consultadas.
- Localizações carregadas, incluindo pt-BR; arquivo de localização embutido utilizado.
- Log salvo.
- Aplicação encerrada normalmente, exit code 0.

**Limitações observadas:**

- 0 devices found.
- VDS indisponível.
- Falhas de CoCreateInstance para algumas classes COM.
- Privilégios de processo negados.
- PowerShell compatível não detectado; download de ISO desativado.
- Sessão de Internet não aberta: erro 0x00002F83.
- Verificação de atualizações DBX não concluída.
- Wldp.dll indisponível.
- Chamadas win32u ainda não implementadas: `NtUserSetProcessDefaultLayout`, `NtUserModifyUserStartupInfoFlags`, `NtUserSetWindowContextHelpId`, `NtUserQueryInputContext`.

**Não testado:** gravação de mídias. **Não afirmar** que o Rufus consegue gravar mídias. A ausência de dispositivos impede essa conclusão.

## 8. Leitura do drive C:

| Execução | Arquivos | Tamanho total informado | Lidos | Exit code |
|---|---|---|---|---|
| Anterior | 4 | 1.009,9 MB | 3,2 MB | não informado |
| Posterior | 112 | 1.015,5 MB | 430,0 MB | 0 |

**Observado:** a segunda execução disponibilizou uma lista extensa de fontes TTF/OTF e DLLs, incluindo componentes Microsoft Visual C++ e bibliotecas de fontes. Essa composição é evidência de carregamento de dependências e recursos.

**Limitações:**

- **430 MB são volume acumulado lido, não RAM ocupada simultaneamente.** Não concluir que esse volume permaneceu na RAM.
- O tamanho total informado variou entre execuções (1.009,9 MB e 1.015,5 MB). A causa não foi informada.

## 9. Pendência: swap e OPFS

**Mensagem recorrente, literal:**

```
[swap] OPFS sync access unavailable, using RAM instead
```

**Observado:** a mensagem aparece nos logs informados.

**Interpretação:**

- O caminho de acesso síncrono ao OPFS não estava disponível.
- O sistema usou fallback em RAM.
- **Não há comprovação nesses logs de que o swap persistente via OPFS tenha funcionado.**
- O impacto do fallback no orçamento de memória ainda precisa ser medido.

**Pendências técnicas (não investigadas nesta rodada; o subsistema de swap não foi modificado):**

- [ ] Disponibilidade e contexto de `createSyncAccessHandle()` no Safari/iOS.
- [ ] Restrições do navegador e do ambiente de execução.
- [ ] Consumo de RAM pelo fallback.
- [ ] Integridade de escrita e leitura.
- [ ] Recuperação e liberação de páginas.
- [ ] Comportamento sob pressão de memória.

## 10. Classificação geral das evidências

Todos os resultados desta tabela são informados pelo usuário e não foram reexecutados nesta rodada.

| Subsistema | Teste | Resultado | Status da evidência | Limitações | Próximo teste recomendado |
|---|---|---|---|---|---|
| CPU | 7-Zip 23.01 x64, compressão | 1.567 KiB/s; 234%; 596 R/U; 1.396 MIPS | Parcialmente comprovado | Metodologia e dados não informados | Repetir com versão e conjunto de dados registrados |
| CPU | 7-Zip 23.01 x64, descompressão | 24.159 KiB/s; 370%; 527 R/U; 1.951 MIPS | Parcialmente comprovado | Idem | Idem |
| CPU | 7-Zip, total e rating do engine | 302%; 561 R/U; 1.674 MIPS; engine 1.936 MIPS | Parcialmente comprovado | Métricas não comparáveis entre si | Definir métrica única de comparação |
| Textura | Streaming, 30 s | 60 FPS médios; pior frame 33 ms; 126 MB/s; 3,4% de draws aguardando | Parcialmente comprovado | Não comprova compatibilidade com jogos | Cena reproduzível com metodologia documentada |
| Armazenamento | Sequencial | Escrita 1.522 MB/s; leitura 1.735 MB/s | Parcialmente comprovado | Sem método, cache ou threads informados | Repetir com método declarado |
| Memória | Página de 4 KB | Leitura 0,9 µs; escrita 3,3 µs; 1.136.364 páginas/s | Parcialmente comprovado | Taxa é separada das velocidades sequenciais | Medir com método declarado |
| Navegador | Recarga da aba | 769 MB e 1.281 MB | Comprovado apenas como observação pontual | Não são limites garantidos; causa da diferença desconhecida | Repetir em série e registrar condições |
| GPU | WebGPU, draws × tempo | 0,86 ms a 6,51 ms (500 a 8.000 draws) | Parcialmente comprovado | Não se sabe o que o tempo inclui; FPS é estimativa | Medir com sincronização e apresentação explícitas |
| Windows (app) | Rufus, inicialização | Concluída; exit 0; várias limitações | Parcialmente comprovado | Só a inicialização foi observada | Repetir com dispositivo USB de teste |
| Windows (app) | Rufus, gravação de mídia | — | Não testado | Ausência de dispositivos | Ambiente com dispositivo de teste |
| Sistema de arquivos | Leitura do C:, execução posterior | 112 arquivos; 430,0 MB lidos acumulados; exit 0 | Parcialmente comprovado | Volume acumulado, não simultâneo | Medir RAM residente durante a leitura |
| Swap | Fallback OPFS para RAM | Mensagem recorrente de fallback | Bloqueado por limitação conhecida (swap persistente não comprovado) | Acesso síncrono ao OPFS indisponível | Ver seção 9 |

Não foi calculado percentual global de conclusão do InkSoC a partir desses benchmarks.

## 11. Reprodutibilidade e qualidade documental

- **Data de registro:** 15/11/2026.
- **Versão testada:** não identificada.
- **Referência de trabalho:** INKSOC-2026-015, commit `e8f9844004a260e043798fb528224ae9c72fd365`, sem comprovação de que os benchmarks correspondem a esse commit.
- **Configuração do ambiente:** ver [README.md](README.md).
- **Unidades:** conforme informadas (KiB/s, MB/s, MB, µs, ms, MIPS).
- **Logs brutos:** não disponíveis no ambiente de trabalho desta rodada. Nenhum log foi inventado. Se forem fornecidos, devem ser anexados em `docs/benchmarks/evidence/` em rodada própria.

**Próximos testes sugeridos:**

1. Registrar a versão exata do build usado em cada benchmark (commit ou artefato com SHA-256).
2. Repetir os benchmarks em série e registrar variância.
3. Declarar metodologia do 7-Zip (conjunto de dados, número de execuções) e do benchmark de armazenamento.
4. Investigar o caminho OPFS (seção 9) e medir o consumo de RAM do fallback.
5. Medir WebGPU com sincronização explícita, separando tempo de CPU, GPU e apresentação.
6. Repetir o teste de recarga da aba e explicar a diferença entre 769 MB e 1.281 MB.

## 12. Pendências desta documentação

- Versão do build testado não identificada.
- Versões do iOS e do Safari não informadas.
- RAM física total do iPhone não informada.
- Logs brutos não disponíveis.
- Metodologia de vários benchmarks não informada.
- Validação funcional em Windows ou Wine real não realizada.
