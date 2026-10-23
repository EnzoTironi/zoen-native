# Sites: publicação de uma versão escolhida

O arquivo privado `text/html` continua sendo um Item FileDoc criptografado. O
cliente prepara os bytes verificados da versão escolhida; uma ação explícita de
uma pessoa autoriza a cópia pública. Editar ou abrir uma prévia não publica nada.
O contrato está em `protocol::sites`; a preparação do cliente e a experiência iOS
são acompanhadas em #319. Este incremento de backend não encerra essa jornada.

`SignedMutation` fixa audiência, pessoa, dispositivo, destino, request ID, head
público esperado e versão/hash/tamanho da origem. A assinatura vale por cinco
minutos. A renovação pode mudar dispositivo e instante, mantendo o mesmo intent;
o backend ainda exige um dispositivo atual da mesma pessoa antes de devolver um
receipt antigo. Uma assinatura de agente não autoriza publicação.

O PostgreSQL mantém somente o índice de entrega pública e receipts imutáveis.
A fence do dispositivo e a verificação de pessoa permanecem na transação de
commit; o lock por Site serializa inclusive a primeira criação. Request repetido
devolve o receipt original antes de comparar o head atual, sem restaurar o head
antigo. Request ID reutilizado com outro intent falha. Rollback é uma nova
publicação explícita de uma versão anterior; unpublish também compara o head.

Os bytes são gravados primeiro em `public-sites/v1/{site}/{sha256}`, com create
condicional. Falha de transação pode deixar uma cópia órfã inacessível pelo
servidor público. Um retry só a reutiliza após verificar os bytes. Head e receipt
são confirmados juntos. A leitura pública verifica tamanho e digest antes de
entregar conteúdo; corrupção gera indisponibilidade, nunca HTML diferente.

## Execução

O processo opcional `zoen-sites` reutiliza os adaptadores PostgreSQL/object_store.
Não inicia FoundationDB nem altera os serviços locais existentes. Precisa de:

- `DATABASE_URL`: diretório de identidades/dispositivos existente.
- `ZOEN_RELAY_NAME`: audiência assinada, exatamente a configurada no cliente.
- `ZOEN_SITES_MANAGEMENT_BIND` e `ZOEN_SITES_MANAGEMENT_ORIGIN`.
- `ZOEN_SITES_PUBLIC_BIND` e `ZOEN_SITES_PUBLIC_ORIGIN`.
- `ZOEN_SITES_BLOB_DIR`: diretório local; as opções S3 existentes continuam válidas.

As origens precisam de hostnames diferentes e listeners distintos. O proxy deve
encaminhar cada origem somente ao listener correspondente, com TLS na implantação
pública. Gestão expõe apenas `POST /v1/sites/publications`, corpo
`{ "signed": SignedMutation, "html": string | null }`, resposta `Receipt`. O
listener público expõe somente `GET /s/{site_id}`. Nenhum endpoint de gestão ou
autenticação existe nele. Nenhum HTML público é servido no router do relay.

O primeiro formato suporta um snapshot HTML UTF-8 autocontido de até 2 MiB, sem
NUL. Os bytes não são reescritos. A CSP executa scripts inline em sandbox de origem
opaca, permite estilo inline e recursos data/blob, bloqueia conexões de scripts,
formulários, frames e navegação do contexto pai. Não há ponte para credenciais ou arquivos do
Zoen. Assets externos, sites multipágina e seus domínios não são aceites concluídos
deste incremento.

Respostas usam no-store, nosniff, no-referrer e permissões de câmera/microfone/
localização negadas. Unpublish impede novas seleções no servidor depois do commit;
não revoga cópias já recebidas nem respostas admitidas antes da confirmação.

Limites iniciais por listener: 16 handlers simultâneos antes de extrair JSON ou
buscar conteúdo e prazo de 20 s incluindo leitura do corpo. Gestão e leitura
pública têm filas independentes. Por processo: 16 operações diretas de publicação, 8 MiB/minuto por pessoa com
burst de 16 MiB, 5 s por operação de storage e limites SQL de 5 s. São limites de
trabalho e payload, não uma medição de RSS ou capacidade de produção. Ainda faltam
retenção/GC de órfãos e histórico, operação pública e provas do envelope de carga.

## Provas

Os testes puros cobrem assinatura, substituição, audiência, limites, freshness,
renovação e intenções de rollback/unpublish. O teste `site_publications` usa
PostgreSQL real, filesystem object_store e dois listeners HTTP próprios. Cobre
replay concorrente, CAS, posse, agente recusado, reabertura, corrupção, isolamento
das rotas e revogação concorrente observada nos locks reais do banco. Resultados
de execução e revisões ficam na issue #319; existência de testes não significa
aprovação até a execução correspondente.
