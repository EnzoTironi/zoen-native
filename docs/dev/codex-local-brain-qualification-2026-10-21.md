# Codex como Brain local — 21/10/2026

O pedido de usar a autenticação do Codex é atendido pelo processo oficial do
Codex. O adapter não abre `auth.json`, não lê tokens/Keychain, não copia credenciais
para o Zoen e não escolhe endpoints internos. O próprio Codex administra o login
ChatGPT existente. A integração é opt-in e local; o Brain Workers AI continua
sendo o padrão.

## Limite de produto e autenticação

A documentação oficial admite integrações existentes locais/open-source usando a
autenticação gerenciada do app-server, mas exclui esse mecanismo para serviços
comerciais/hosted. O produto hospedado precisa do fluxo próprio de consentimento
[Sign in with ChatGPT](https://learn.chatgpt.com/docs/app-server#auth-endpoints),
ainda não implementado neste lote. O
[modo de token externo](https://developers.openai.com/siwc/token-sharing-open-source/codex-app-server)
é outro fluxo, com credencial emitida para o aplicativo; não autoriza extrair o
login do Codex. Uso do plano consome sua quota; não se afirma acesso ilimitado,
custo zero ou disponibilidade de um modelo apenas por constar no catálogo.

A versão qualificada local é `codex-cli 0.163.0`. A chamada de metadata informou
`account.type = chatgpt`. Uma inferência sintética real concluída confirmou acesso
a `gpt-6.1-sol` naquele pedido, sem API key fornecida. O
[contrato app-server](https://learn.chatgpt.com/docs/app-server) é o protocolo
JSON-RPC oficial sobre stdio, sem abrir um listener TCP ou reutilizar um daemon
externo.

## Fronteira de execução

`CodexBrain` implementa o `Brain` existente e produz propostas JSON limitadas.
Ele não fornece ferramentas dinâmicas para execução no app-server. O Runtime
continua responsável por gate inbound/outbound, cada ferramenta oferecida,
autor/Space/fonte, limites de rounds/calls, journal existente e efeitos aceitos
pelo núcleo. Resultados de passos já executados voltam como contexto; não existe
um novo diário, estado de documento ou fila paralela no adapter.

Cada think cria um diretório privado vazio e um thread efêmero. `environments: []`
na criação e no turn retira o acesso ao ambiente; sandbox read-only sem rede,
raízes/capacidades vazias e approval never são conferidos na admissão. Shell,
exec, apps, plugins, hooks, memórias, browser, computer use e demais capacidades
ambientais listadas no fonte ficam desabilitadas por overrides temporários. O
adapter confere a configuração efetiva antes de iniciar inferência e exige que
não tenham sido carregadas instruction sources externas. O processo ainda
administra seus próprios arquivos de autenticação/diagnóstico do Codex; isso não
é uma garantia de zero I/O do binário.

`mcp_servers={}` faz merge e deixou dois servidores habilitados no primeiro
probe. O adapter descobre somente os IDs via metadata, reinicia com deny explícito
por servidor e exige enabled=false em todos antes de continuar. Não persiste nem
imprime a configuração completa. Comandos de notify também são desabilitados e
conferidos. A versão diferente, login por API key, capacidades não desabilitadas,
requests externos, itens de ação, turn falho/interrompido, ferramentas não
oferecidas ou propostas ambíguas falham fechados. Não há fallback silencioso para
outro provider/modelo.

O transporte limita o contexto de entrada a 256 KiB, cada linha recebida a 1 MiB e toda sessão a
4 MiB; respostas finais têm limite de 64 KiB. O think possui orçamento de 120 s.
O processo é encerrado e os recursos locais são descartados em erro/cancelamento;
stderr e respostas de erro externas não são copiados para diagnósticos do Zoen.
Nenhum segredo de provider do processo pai é repassado no ambiente filho.

## Ativação explícita para desenvolvimento local

Configure no processo do host, sem editar o login/configuração do Codex:

```sh
export ZOEN_BRAIN=codex-local
export ZOEN_CODEX_BIN=/caminho/absoluto/para/codex
# Opcional: modelo exato presente no catálogo; omitir usa seu default.
export ZOEN_CODEX_MODEL=gpt-6.1-sol
```

Depois use o `agent-host` existente com seu perfil sintético autorizado. Esse
provider não configura nem substitui o Clef: a política de gate continua sendo a
existente. Ausência de `ZOEN_BRAIN`, ou workers-ai explícito, conserva o caminho
Workers AI. Configuração local inválida recusa; não muda o provider padrão da
aplicação iOS nem concede ao modelo acesso ao repositório/shell.

## Provas e limites

Base do lote: `654d9c0c4329b7f92937929aa9752d73efc5d41e`.
Evidências: `/Users/enzotironi/.codex/builds/zoen-codex-brain-v260/`.
`metadata-01/02.json` e `probe-attempts.json` registram a admissão e os controles
falhos sem credenciais. `inference-01.json` é a primeira proposta sintética real,
com turn completed e somente itens userMessage/agentMessage; ela não executou uma
ferramenta.

Sete testes focados usam subprocessos JSON-RPC reais com respostas de modelo
controladas: admissão, proposta exata, API-key/version/MCP/notify recusados antes
de inferência, ferramentas não oferecidas, requests externos, failed/interrupted,
notificações que ultrapassam a resposta RPC, saída grande sem newline, timeout e
morte do filho real. Não são sete chamadas reais ao Codex/modelo nem equivalência
com sua suíte upstream. O teste real de Runtime/DeckCraft é separado e ignorado
na execução comum; exige ativação explícita e o worker/manifesto públicos assinados
do operador.

A prova `actual-runtime-02` passou em 285,14 s, incluindo 6,44 s de compilação
incremental e 277,42 s do teste com carregamento frio do worker. O Brain real do
Codex propôs read → slide.notes → confirmação; o Runtime/Gate e o loader de
manifesto assinado do operador executaram dois passos reais. O núcleo aceitou
exatamente a versão 2; repetir a mesma chamada devolveu o receipt existente, sem
nova versão. O Client foi encerrado, o banco SQLite reaberto e o documento voltou
com os mesmos bytes, SHA, versão e exatamente dois passos no journal. O worker
WASI-p2 usado tem SHA `4d0d9d82852f2e2bb86ca59877ee721c38889b452ddaca5d50a4498b70a17504`;
o manifesto e publisher públicos estão selados em `operator-binding-01.json`.
O gate deste fixture é explicitamente `Gate::default()`/offline floor; não
qualifica o Clef remoto, que tem prova independente.

`actual-runtime-01` permanece como falha: o pedido sintético incluía um ponto
final em `gated edit.`, mas seu oracle esperava a frase sem ele. Os dois passos,
a gravação e o replay já tinham passado; a verificação de reabertura ainda não
havia sido alcançada. O fixture foi esclarecido com literal explícito e oracle
consistente, e somente a prova real necessária foi repetida. Não houve mudança
no adapter de produção para aceitar esse resultado. A compilação fria anterior
levou 262 s e o teste falho 309,90 s; não são medições de SLA ou capacidade.

Na fonte final, `final-focused-01` passou nos sete testes e `clippy-02` passou em
zoen-agentd + zoen-cli, todos os alvos, com `-D warnings`. Uma revisão independente
read-only do adapter, seletor, contrato Runtime e schema0.163.0 não encontrou
achado acionável; não executou testes nem aprofundou a jornada DeckCraft. O
commit e hashes congelados estão em `source-ready.json`.

Não houve alteração no LIVE, contas/histórico reais do Zoen,
login/configuração do usuário, provedor comercial, serviço principal ou build
iOS/Pods. SIWC comercial, finanças/admissão durável, cancelamento em máquina
remota, outros sistemas operacionais e capacidade em escala continuam pendentes.

## Host local: exclusão entre processos

O comando `agent-host` agora adquire um lock exclusivo do sistema operacional
antes de abrir o vault, o Client, a sincronização ou os provedores. O mesmo guard
permanece vivo durante o loop e até a revogação do engine no encerramento da
sincronização. Outro processo cooperativo com o mesmo diretório persistente
recusa a execução. O arquivo de lock não é apagado: após uma saída normal ou
morte do processo, o sistema libera o lock sobre o mesmo inode. Erros ao abrir
ou adquirir o lock recusam a inicialização. Nenhuma dependência foi adicionada.

A fonte `8e323ce8` passou em seis testes com processos reais do sistema, cobrindo
contenção, saída normal, morte do holder, diretórios independentes e lifetime do
guard; a seleção CLI passou em 30 testes, com seis casos externos explícitos
ignorados, além de Clippy estrito. A execução foi em Darwin arm64. Isso impede
concorrência local cooperativa; não estabelece fencing distribuído, garantias de
filesystem de rede, join global de tarefas assíncronas nem um ledger de tentativas
desconhecidas de provedores.

A [qualificação Clef](cloudflare-clef-gate-qualification-2026-10-21.md) registra a
prova conjunta com Codex, duas aprovações canônicas, uma edição real e recuperação
da mesma versão. O cadastro do host pelo iPhone e a jornada pelo relay/MLS real
ainda não foram comprovados. O runtime gerenciado remoto continua fechado sem
custódia e continuidade implementadas; não é substituído por este processo local.
