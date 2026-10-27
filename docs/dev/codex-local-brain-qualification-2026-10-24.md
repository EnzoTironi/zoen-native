# Codex local: requalificação 0.164.0

O adapter local aceita exatamente `codex-cli 0.164.0`. A versão instalada mudou
desde a [qualificação anterior](codex-local-brain-qualification-2026-10-21.md);
o gate antigo recusava a inferência. Não ampliamos o gate para versões arbitrárias.
Issue: [#570](https://github.com/EnzoTironi/zoen-native/issues/570).

## Contrato e prova real

Inspecionamos os schemas gerados pelo executável oficial e fizemos uma admissão
sem inferência. O login observado foi `chatgpt`; o default do catálogo foi
`gpt-6.1-sol`. As 25 capacidades ambientais ficaram desabilitadas, nenhum MCP
ficou habilitado, notify ficou vazio, web search desabilitada e project docs zero.
O thread efetivo foi efêmero, provider OpenAI, modelo exato, sandbox read-only
sem rede, raízes/capacidades vazias e instruction sources vazias.

O Codex administra seu próprio login. O probe e o Zoen não abrem nem copiam
`auth.json`, tokens ou Keychain. Isso continua sendo desenvolvimento local;
a autenticação de um serviço comercial hospedado permanece uma entrega separada.
Referência: [autenticação oficial](https://learn.chatgpt.com/docs/auth).

Após os checks, o exemplo opt-in `codex_local_check` executou uma única chamada
`Brain::think` pela implementação de produção, com IDs sintéticos, histórico vazio
e zero ferramentas. O modelo devolveu exatamente `Zoen local validation 0164`.
O processo terminou com exit 0 em 5,006 s, incluindo admissão e teardown.
Não é uma medição de SLA, custo ou retries internos do provider.

O exemplo exige diretório privado e cria um marcador exclusivo antes da tentativa;
recusa reutilizar esse diretório. A ativação exige `ZOEN_TEST_REAL_CODEX=1`,
`ZOEN_CODEX_LOCAL_QA_OUT`, o caminho oficial em `ZOEN_CODEX_BIN` e, nesta prova,
`ZOEN_CODEX_MODEL=gpt-6.1-sol`. Não roda na suíte comum nem altera documentos.

## Verificação reproduzível

Fonte executada: `86b1a37f8639364987e99f8dc82b82eba2c5586d`.
Evidências locais: `/Users/enzotironi/.codex/builds/zoen-codex-local-0164-v571/`.

- `red-01`: fixture 0.164 com adapter ainda 0.163 recusou a versão, como esperado.
- `metadata-02.json`: contrato efetivo sem chamada de modelo.
- `green-01`: 16 testes de subprocesso/contrato passaram; um teste de integração
  opt-in permaneceu ignorado. Clippy de todos os alvos do agentd com
  `-D warnings` passou; o exemplo compilou normalmente.
- `inference-01`: tentativa única, resposta exata, fonte/binário estáveis,
  recibo sem conteúdo de credenciais. Binário SHA-256
  `e529deedb174dd1b853101e7d36d9e4988551c3e83acdc8943d9664e38e4c8a5`.

Os testes mantêm recusas para API key, versão incompatível, MCP habilitado,
requests externos, ferramentas não oferecidas, turn incompleto, limites de saída
e timeout com encerramento do filho. A prova sintética não substitui Runtime/Gate,
Clef remoto, relay real, aprovação na interface ou teclado do iPhone. As provas
anteriores continuam com suas próprias fontes e limites.
