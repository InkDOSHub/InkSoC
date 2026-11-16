# InkSoC — contexto operacional

> Resumo técnico para continuidade. Não é transcrição de conversas nem histórico completo.
> Atualizado em 2026-11-16. Baseline consultada: `main` em `6f96656504a247f0d2a116b441e3d01e7e190841`.
> Revalidar branches, PRs, workflows e SHA antes de cada alteração.

## 1. Projeto

Engine x86-64 em Rust/WebAssembly, JIT, suporte a código 32-bit/WoW64 e componentes Wine, executada no navegador, com foco em WebKit/iPhone/iPad. Drive C: e dados de programas ficam no armazenamento privado do navegador (OPFS).

## 2. Responsabilidades atuais

- **Mantenedor humano:** define prioridades, fornece validação no dispositivo real e mantém a decisão final sobre merge/publicação.
- **GPT-6:** responsável pela condução técnica nesta etapa: investiga o repositório, propõe arquitetura, implementa diretamente em branches autorizadas, revisa diffs, acompanha GitHub Actions e reporta limites/evidências.
- **GitHub:** fonte de verdade para código, histórico, PRs e resultados automatizados.
- **GitHub Actions:** builds e testes reproduzíveis associados a SHAs.

Claude Code não é mais dependência operacional deste projeto. Não criar tarefas que dependam de créditos, execução ou resposta desse agente.

## 3. Fluxo de trabalho e confiabilidade

1. Consultar o estado real de `main`, branches, PRs e Actions.
2. Definir uma alteração lógica por branch/PR, com hipótese, arquivos afetados, riscos e critérios de aceitação.
3. Implementar em branch isolada; nunca editar `main` diretamente, fazer force-push ou reescrever histórico.
4. Rodar verificações locais disponíveis e workflows automatizados; vincular resultados ao SHA exato.
5. Revisar o diff e os efeitos colaterais; atualizar documentação de estado com fatos verificáveis.
6. Mantenedor testa no Safari/iPhone/iPad quando o cenário depende do dispositivo real.
7. Merge em `main` somente após autorização explícita e específica.

A autorização geral para conduzir o projeto permite investigar e implementar em branches e abrir PRs. Não equivale a autorização irrestrita para apagar dados, alterar segredos/configurações administrativas, reescrever histórico ou fazer merge sem confirmação específica.

## 4. Prioridades técnicas conhecidas

### P0 — Steam e transporte de rede
- Logs de 2026-11-15 mostram Steam Setup com exit code 1 e falhas de download de `bins_cef_win32_win7-64` e `bins_webhelpers_win32_win7-64` com zero bytes recebidos, apesar de respostas reportadas como HTTP 200.
- Isso sugere investigar o caminho de rede/stream antes de continuar ajustando apenas geometria da janela. A causa ainda não está diagnosticada.
- Transporte TCP do engine está em `src/engine/src/net.rs`; a ponte browser/túnel está em `worker.js`. Verificar fluxo WISP, buffers, encerramento, backpressure, DNS, TLS, HTTP e downloads maiores.
- Não repetir calibração que force o navegador a esgotar memória. A calibração anterior provocou reload deliberado da aba.

### P1 — abertura de arquivos e integração com InkDOS Hub
- InkDOS Hub já possui launcher por extensão, handoff por IndexedDB e aplicativos web de texto, documentos, PDF, planilhas, apresentações, EPUB, imagens, mídia e código.
- Referências no Hub: `inicio/lancador.js`, `inkdos/apps/txt/`, `inkdos/apps/documents/`, `inkdos/apps/pdf/`, `inkdos/apps/spreadsheets/`, `apps/codigo/`.
- InkSoC mantém C: em OPFS sob `inksoc-c`; integração precisa de uma ponte explícita e segura entre o drive virtual e os apps do Hub. Não presumir que compartilhar IndexedDB ou origem já resolve permissões, isolamento, persistência ou salvamento de volta.
- Objetivo: abrir/visualizar e, quando suportado, editar/salvar arquivos sem perder dados ou quebrar o isolamento entre aplicativos.

### P1 — monitor de recursos
- Distinguir memória residente do engine, heap WASM, heap JavaScript, swap/OPFS, armazenamento virtual C: e quota/uso do site.
- Não chamar memória configurada de memória física do dispositivo.
- Rede deve medir bytes efetivamente encaminhados pelo túnel.
- GPU real por processo e disco físico do dispositivo podem não ser expostos pelo navegador; mostrar indisponível quando não houver fonte confiável, nunca inventar zero.

### P2 — shell, descoberta e atalhos
- Rever descoberta de aplicativos para priorizar atalhos reais do Menu Iniciar/Desktop e entry points conhecidos, evitando listar executáveis auxiliares como aplicativos independentes.
- Criar atalhos de forma persistente e documentar o alvo real.
- Corrigir caminhos inválidos e garantir navegação coerente pelo C:.

### P3 — compatibilidade
- Construir matriz por aplicativo, versão, arquitetura, instalação, inicialização, interface, rede, I/O e encerramento.
- Build ou instalação concluídos não significam compatibilidade funcional.
- Não declarar suporte genérico a Windows; publicar apenas cenários efetivamente testados.

## 5. Estado do repositório na última consulta

- `main`: `6f96656504a247f0d2a116b441e3d01e7e190841`.
- PRs #2 e #3 estavam abertos em 2026-11-16; seus SHAs-base não coincidiam com o HEAD atual de `main`. Reavaliar diff, ancestralidade e checks antes de reutilizar ou integrar.
- Graphify está em `src/engine/src/graphify-out`; usá-lo para navegação do engine antes de releituras amplas.

## 6. Privacidade em repositório público

- Não publicar transcrições de conversa, contexto pessoal, nomes reais, e-mails, identificadores de conta, endereços, caminhos locais, nomes privados de arquivos, tokens, credenciais ou dados de terceiros.
- Logs públicos devem ser técnicos, mínimos e anonimizados; usar rótulos como `mantenedor`, dados sintéticos e caminhos fictícios.
- Não anexar screenshots, PDFs ou arquivos reais do usuário a Issues/PRs. Reproduzir problemas com arquivos sintéticos.
- Histórico Git já publicado não é apagado por editar um arquivo atual; qualquer dado sensível encontrado deve ser avaliado separadamente, sem reescrita de histórico sem autorização e plano específico.

## 7. Regra de evidência

Distinguir sempre: identificado, diagnosticado, implementado, compilado, testado, validado e confirmado funcionalmente. Relatórios devem registrar SHA, comandos/testes, resultados, Actions e limitações. Nunca afirmar que algo foi testado no Safari ou no dispositivo do mantenedor sem evidência fornecida por ele.
