[Signal reference index](signal-reliability-reference.md).

## Aprender com as suítes públicas

Os levantamentos de 20 de outubro de 2026 estão no
[registro de contratos](signal-test-contracts.json). Há 51 mapeamentos baseados
em asserções efetivamente lidas, em seis repositórios: 24 de servidor/libsignal,
18 famílias dos três clientes e nove do RingRTC. Uma família pode conter vários
casos; os mapeamentos também podem compartilhar um invariante. Esses números não
são quantidade de testes executados nem uma extração completa das suítes. Os 42
registros originais conservam a base comparada `f3ab6fae`; cada entrada RingRTC
explicita `01de169b`. Subconjuntos implementados depois indicam seus próprios
commits, sem transformar a classificação histórica em aprovação atual.

O inventário identifica versões, caminhos, fixtures inválidas, parametrização e
alvos de fuzzing onde encontrados. Os 1.702 arquivos candidatos dos clientes
foram enumerados por padrões; apenas os trechos registrados de 19 arquivos
selecionados tiveram leitura semântica. Os contadores lexicais não expandem
parâmetros e podem incluir comentários/infraestrutura. No servidor e libsignal,
as áreas efetivamente inventariadas e trechos lidos também são explícitos.
No RingRTC, 351 caminhos rastreados resultam em 20 candidatos amplos, incluindo
15 fontes, dois executores e três arquivos de apoio/configuração. As 121 marcações
Rust, 34 Swift e 35 Node são contagens lexicais dos arquivos obtidos, sem expansão
ou execução. A maioria das asserções, todos os corpos Android e mecanismos
inline de fuzzing permanecem sem revisão. Os nove registros distinguem callbacks
ICE e mídia simulada, áudio de teste no Swift e HTTP simulado no Electron de
execução nativa real; não comprovam privacidade do microfone, continuidade sonora
ou liberação física dos recursos. Nenhuma suíte do Signal foi executada.

Cada entrada registra estado inicial, sequência, falha, resultado realmente
assertado, fonte imutável e equivalente/lacuna no Zoen. A classificação vale para
a fonte comparada, não aprova equivalência de produto. Priorizar perda de dados,
duplicação, autorização, recuperação e mídia. Testar os limites definidos pelo
Zoen, sem copiar números de retries, janelas ou políticas de outro protocolo.

O primeiro teste original integrado, em `crates/mls/src/tests.rs`, verifica que
um ciphertext adulterado não modifica as linhas de grupo OpenMLS nem seu
checkpoint. Após fechar/reabrir a conexão SQLite, a mensagem original continua
legível; outro reabrir recusa replay e aceita tráfego posterior. A biblioteca MLS
passou nos 19 testes e no Clippy estrito. O contrato é da camada criptográfica:
o Zoen preserva eventos externos autenticados como histórico opaco quando o
conteúdo interno ainda não pode ser decifrado. Não copiar uma asserção de rejeição
do evento/cursor inteiro que contrariaria essa política. O teste não representa
perda de energia, processo encerrado ou recepção por outro aparelho.

Três testes originais em `crates/client-core/src/backup.rs` verificam oito
roundtrips com nonces distintos e bytes exatos, rejeição por chave/dono incorretos,
adulteração de nonce/ciphertext/tag, truncamento e bytes extras. Corpos inválidos
selados pelo próprio teste cobrem framing, versão e dono interno autenticados. A
biblioteca client-core completa passou em 539 testes, com Clippy estrito e
formatação aprovados. Controles negativos detectaram nonce fixo e validações
removidas de versão/dono. Isso cobre o envelope, não a restauração real de SQLite,
Wasm/navegador ou a jornada inteira de recuperação. O código de produção não
precisou mudar; nenhum defeito de produção foi demonstrado por esses quatro novos
testes.

Em seguida, os commits `01de169b` e `0db43269` acrescentaram dois testes
originais. Em S04, a sessão WebSocket real do cliente recebe um evento persistido
que ultrapassa a última página de replay; solicita o cursor ausente antes do
primeiro `SyncDone`, reconcilia replay sobreposto e verifica quatro eventos e duas
mensagens com IDs, ordem, sequências e textos exatos, inclusive após reabrir o
SQLite. Nenhuma mensagem posterior disfarça a reconciliação. A revisão seguinte
retira explicitamente a posse do `EngineLease` antes de reabrir. Isso ainda não
qualifica a janela real de inscrição do relay/FDB/NATS nem uma notificação
completamente perdida: esses interleavings continuam como próximo lote.

Em C16, signatários, horários e IDs fixos comparam projeções de edições assinadas
no outbox durável, confirmações próprias, replay remoto, duplicatas e reabertura.
Asserções cobrem cardinalidade e DTO completo, quatro mensagens, texto editado,
citação, thread e nove eventos finais. O teste insere os intents assinados no
outbox diretamente; admissão pelo comando público de edição, MLS, anexos,
expiração e um corpus gerado amplo permanecem fora desse recorte. O lote integrado
do client-core passou nos 541 testes e no Clippy estrito. Os controles positivos
e negativos registrados são evidência dos contratos delimitados; esta atualização
do registro não executou novamente testes, aplicativos ou serviços e não altera
as flags de aceitação do produto.

A matriz também registra limites encontrados nas próprias suítes: comparações
por `zip` sem checagem prévia de cardinalidade; fixtures cujo erro esperado vem
de uma validação anterior ao problema sugerido pelo nome; testes de backup
condicionados a diretórios externos; e fuzzing com exclusões explícitas. Aplicar
as propriedades úteis e fortalecer as checagens do Zoen quando adequado, usando
fixtures próprias. Nenhum código de teste do Signal foi copiado.

## Avaliação de um fork completo

Recomendação em avaliação, sem migração autorizada por esta análise: manter o
Expo como interface do produto e avaliar as fundações de mensagens separadamente.
Um fork próximo do Signal pode reduzir o trabalho para um mensageiro iOS privado.
Para o Zoen, assume também aplicações distintas para as outras plataformas, seu
backend e a integração de Apps, Páginas, permissões e agentes. Estar pre-launch
permite mudar essa escolha; investimento já feito não é critério para preservá-la.

O núcleo Rust de libsignal pode ser avaliado pela ponte nativa que já existe no
Zoen. A ausência de um pacote pronto para Expo não impede essa integração. Os
critérios relevantes são compatibilidade de identidades, grupos e histórico,
transações de sessão, manutenção das atualizações e o custo de distribuição em
cada plataforma. A avaliação não autoriza misturar protocolos nem estabelece
que o MLS atual tenha as propriedades de privacidade do Signal. Nenhum código do
Signal foi integrado e a licença do Zoen permanece inalterada neste lote.
