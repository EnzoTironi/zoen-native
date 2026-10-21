# Preflight de restauração — qualificação isolada de 21/10/2026

Uma tag criptográfica válida autentica o payload; não demonstra que sua imagem
SQLite é completa ou importável. O candidato adiciona o mesmo preflight antes
de `restore/enroll` no fluxo público e antes do wipe na restauração direta.
A base isolada é `c8d70728`; o controle negativo inicial foi fixado em
`50b701d6` antes de alterar a implementação.

## RED executado pelo fluxo público

Os fixtures criam um backup real com `backup_snapshot`, abrem/sealam o payload
com as funções criptográficas de produção e servem a recuperação por uma porta
HTTP loopback efêmera. O cliente chama `restore_backup_with_recovery_key`, faz
start/open/download e usa a chave real para abrir o payload. Um alvo sintético
sem conta contém dados locais anteriores; a prova compara sua imagem SQLite
inteira e conta as escritas no vault e os pedidos de enrollment. Nenhum segredo
ou imagem de usuário é usado ou impresso.

| Entrada autenticada | Antes da correção |
| --- | --- |
| SQLite vazio, mas válido | Aceito; 1 enrollment; 5 escritas de vault; alvo alterado. |
| SQLite sem `events` | Aceito; 1 enrollment; 5 escritas de vault; alvo alterado. |
| Bytes que não são SQLite | Recusado após 1 enrollment e alteração do alvo. |
| `agreement_secret` com 31 bytes | Aceito; 1 enrollment; 5 escritas de vault; alvo alterado. |
| Tipo inválido em `profile_keys.fields` | Recusado após 1 enrollment, 5 escritas de vault e alteração do alvo. |
| BLOB em `synced_spaces.space` | Recusado após 1 enrollment e alteração do alvo. |
| Chave própria de perfil inválida | Aceito; 1 enrollment; 5 escritas de vault; alvo alterado. |

Os primeiros quatro casos estão em `red-public-02`; os três últimos em
`red-reader-02`. `red-public-01` é uma falha de preparação do fixture (nenhum
log canônico existia antes do primeiro evento) e não conta como prova funcional.
`red-reader-01` registra a primeira reprodução do erro de tipo do perfil.

## Implementação

- O header exige versão suportada, vínculo entre raiz e identidade e segredos
  de raiz/acordo com exatamente 32 bytes. Não rotaciona silenciosamente uma chave
  de acordo de tamanho inválido durante a recuperação.
- SQLite executa `integrity_check`, identifica tabelas reais e suas colunas.
  Views, tabelas ausentes e campos obrigatórios ausentes são recusados.
- O mesmo import de produção por interseção de colunas é executado numa Store
  atual descartável em memória. Constraints e leitores reais de identidades,
  Espaços sincronizados e chaves de perfil precisam funcionar antes do admission.
- `me` e o perfil raiz precisam corresponder ao header; o marcador do formato
  de eventos precisa ser o suportado. Cada linha de evento deve corresponder
  aos seus campos indexados e passar por `SpaceLog.accept` (sequência, cadeia,
  assinatura, certificado e vínculo causal existentes).
- A preparação não roda migrações de Engine sobre a entrada nem acrescenta
  autoridade durável. O import final mantém o filtro de rascunhos do aparelho
  anterior e revalida a mesma entrada antes de apagar conteúdo local.

A API pública já verificava alguns limites de header; os casos de versão,
raiz curta/divergente e hexadecimal inválido são regressões de preservação dessas
barreiras. O novo comportamento material está demonstrado nas linhas RED acima.

## Compatibilidade comprovada

A fonte mais antiga retida da implementação de backup, `cbb4b1f6`, já contém as
oito tabelas portáveis e os campos mínimos exigidos. As colunas nullable
`identities.secret`/`profile_keys.fields` continuam opcionais; o default de
`profile_keys.seen_version` continua 0. A prova constrói um snapshot sem essas
colunas, com uma coluna extra desconhecida e `user_version=3`, e o restaura pelo
fluxo público com sucesso. Isso comprova a compatibilidade da política de import;
não afirma existir um backup histórico distribuído com essa combinação sintética.
O marcador `event_format=3`, já presente naquela fonte retida, permanece exigido
para impedir um restore que seria apagado pela migração prelaunch na reabertura.

Também permanecem aceitos o backup saudável atual e um retry saudável sobre o
mesmo alvo/vault depois de recusar um snapshot incompleto. O corpus canônico já
existente conserva 77 eventos assinados, 65 mensagens, perfis, versões, chaves e
projeções exatas durante a restauração e a reabertura fria em novo aparelho.

## Resultado final

| Comando | Resultado |
| --- | --- |
| `cargo test --locked -p client-core --lib --jobs 2 backup -- --nocapture --test-threads=1` | 25 passaram; 0 falharam. |
| `cargo clippy --locked -p client-core --lib --tests --no-deps --jobs 2 -- -D warnings` | Passou. |

O lote cobre dados incompletos/malformados, página B-tree corrompida, tabelas
substituídas por view, constraints/duplicatas, campos indexados divergentes,
assinaturas/JSON inválidos, tipos dinâmicos do SQLite, ausência de raiz/formato,
headers, restauração direta e exclusão de rascunhos/recibos locais. Todos os casos
públicos recusados terminaram sem enrollment, sem escrita de vault e com imagem
do alvo idêntica. Os negativos não leem só nomes de testes ou strings da fonte.

O teste HTTP usa a API e a criptografia de produção com um servidor de fixture;
não executa o relay/FDB/NATS, Keychain real, HSM/OPRF de senha ou aparelhos.
Nenhuma suíte do Signal foi copiada ou executada. Esta mudança não torna atômicos
os efeitos externos e o armazenamento local de um snapshot válido se houver falha
posterior de disco ou Keychain; tampouco qualifica escala/performance. A API
pública e a restauração direta fazem passes separados do mesmo preflight.

Evidências persistentes:
`/Users/enzotironi/.codex/builds/zoen-backup-preflight-v255/`.
`final-source-inputs-02.json` vincula as fontes de `green-backup-04` e
`clippy-02`; `schema-compatibility.json` fixa a referência de schema;
`source-ready.json` registra os commits e o estado final.
