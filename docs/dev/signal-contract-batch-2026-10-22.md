# Signal: oito contratos e uma correção de limite de mídia

Comparação da fonte `1e66b98aaf2b6680d8b9708eff8d24642bf4b065` com as
asserções upstream fixadas no [registro deste lote](signal-contract-batch-2026-10-22.json).
O JSON contém estado inicial, sequência, falha, resultado realmente assertado,
commit, hash SHA-256 de cada arquivo lido e limite de cada classificação.
Nenhum teste do Signal foi executado ou copiado para o produto.

| Contrato lido | Comparação atual no Zoen | Limite observado |
| --- | --- | --- |
| C01: retries mantêm a identidade da mensagem persistida | Equivalente delimitado | Os testes existentes preservam intent/ciphertext e cardinalidade após falha SQLite e reabertura; não importamos o número de tentativas do Signal. |
| C02: três envios persistidos da mesma conversa conservam a ordem | Parcial | Há provas de dependências File/mensagem e bloqueio por Space. FIFO genérico de três mensagens restauradas não foi qualificado neste lote. |
| C04: registros inválidos não executam ações | Parcial | Recuperação de chunks privados ausentes/corrompidos recusa publicação. Isso não prova uma fila genérica de quarentena; apagar o job não autoriza apagar conteúdo do usuário. |
| C05: receipt anterior à mensagem é conciliado | Parcial | Watermarks diferidos sobrevivem a SyncDone, dependência tardia e recusa SQL. Identificação por timestamp do Signal não é o protocolo do Zoen. |
| C06: destino incorreto é recusado | Parcial | Demandas de mídia revalidam identidade, aparelho, versão/hash e associação. Não equivale à matriz completa de destinos MLS. |
| C07: frame inválido não impede o próximo válido | Parcial | O teste existente usa socket/decoder reais, assinatura inválida, dois eventos válidos e replay. Ciphertext MLS corrompido é outro recorte. |
| C09: apagar uma referência não elimina metadados compartilhados | Política incompatível | Histórico imutável do Zoen tem retenção própria. Não transplantamos a exclusão de metadados nem alegamos prova de GC físico. |
| C11: limites de composição e ordens MIME mistas | Política incompatível | Classes/quantidades são escolhas de produto. O defeito abaixo envolve limites de bytes já definidos pelo Zoen. |

A fonte C09 tinha um caminho histórico incorreto no registro anterior: no commit
Android fixado, o arquivo está em `app/src/test/java` e usa Robolectric, não em
`androidTest`. O caminho antigo respondeu 404; o arquivo real foi localizado na
árvore do mesmo commit e suas asserções foram lidas. A correção está explícita
no novo registro, sem mudar silenciosamente o resultado histórico. Em C04, a
leitura também corrigiu a descrição inicial: o stream já está ativo quando
os valores inválidos são adicionados; não é uma restauração de fila pausada.

A autorização de confirmações pendentes em #139/PR #190 já pertence ao lote
congelado do responsável principal (`54c4cfe6`). Este trabalho não a duplica e
não atribui sua correção aos oito mapeamentos acima.

## Defeito reproduzido e corrigido: #197

Perfis e fundos criavam demandas sem limite de ciphertext, enquanto File já
possuía o seu. O worker usava `response.bytes()` nesses dois caminhos antes de
verificar hash/AEAD. Na admissão do cache, a abertura genérica também ignorava
a política existente de `ProfilePhoto::open`.

O controle RED preservado em `bad864246adf8e2f99a11e9c93ce51c2e51770a9`
recebe um perfil assinado e cifrado de outro aparelho sintético da mesma
identidade. Os hashes e a criptografia são válidos, mas os bytes excedem em um o
limite de 5 MiB. A abertura normal recusa; a admissão antiga no cache aceita,
fazendo falhar a asserção específica. Nenhum conteúdo do usuário foi utilizado.

A correção reutiliza os limites existentes: 5 MiB para perfil, 8 MiB para fundo,
e o comprimento do descriptor para File/página. Todos os downloads passam pela
leitura incremental limitada. Fundos recusam comprimento vazio, excessivo ou
que excederia a soma; o cache aplica o mesmo limite e reutiliza `ProfilePhoto::open`
para a foto. Não há alteração de FFI, esquema, política de retries ou autoridade.
Dados já armazenados e histórico assinado não são apagados ou migrados.

Os 28 casos focados passaram, incluindo o RED corrigido, limite inclusivo,
foto vazia/corrompida, comprimento inválido de fundo, cancelamento, revogação e
backoff existentes. O teste HTTP real recusa Content-Length excessivo sem
aguardar o corpo, interrompe chunked acima do limite sem aguardar o terminador
e aceita os bytes exatos da resposta seguinte. Clippy estrito de todos os
targets do client-core passou.

O cache de compilação foi clonado por APFS para um target privado. Os 25
pacotes locais derivados do metadata atual foram limpos antes do compile;
`CARGO_MANIFEST_DIR`, artefato executado e inventário de testes foram registrados.
Não foi executada a suíte completa, upstream, iPhone, backend implantado ou
providers. O teste HTTP e a admissão de cache são fronteiras adjacentes; não
constituem uma jornada móvel completa do worker. Memória de pico e decodificação
real de imagens não foram medidas. A garantia de limite diz respeito aos bytes
acumulados do corpo, não a todo o consumo do processo/TLS/alocador.
