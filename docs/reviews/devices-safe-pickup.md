# Frente f — aparelhos e retirada segura do vínculo

Escopo: linha “Devices + link pickup” do inventário em
`origin/reason/task-a1e26d3e:docs/reviews/messenger-parity.md`.
Não inclui recuperação/senha (g), threads, presença, mídia ou mudanças na rejeição de aparelhos revogados.

## Alterações

- Apple/Android: entrada de código em texto, conferência explícita dos seis números antes de aprovar,
  lista atualizável, erros recuperáveis e confirmação antes de desconectar outro aparelho.
- Web: tela Aparelhos com atualização e desconexão; a aprovação de novos aparelhos continua no celular,
  pois o navegador não guarda a chave raiz da conta. A orientação descreve o caminho de colar código,
  sem prometer um leitor de QR que não existe nestas telas.
- Pickup nativo: reconexão em falhas de transporte; um prazo absoluto cobre conexão,
  autenticação, leitura, escrita e espera. Recusas explícitas continuam terminais. Cancelar a future
  cancela a conexão; não foi alterada a autorização do servidor. Prazo já vencido não abre conexão.
- Pickup web: prazo de 15 minutos guardado junto ao pedido criptografado, preservado ao recarregar;
  registros antigos sem prazo e códigos vencidos são substituídos. Rejeição de pedido após Ready
  não encerra o transporte e agenda nova tentativa. Cancelamento/prazo descartam respostas tardias
  e abortam salvamentos pendentes (inclusive antes de abrir a transação IndexedDB). Um vínculo já
  salvo antes do cancelamento continua sendo um vínculo concluído, não uma revogação.

## Evidência local

- `cd packages/zoen-ui && npm run typecheck:web`: passou.
- `cd packages/zoen-ui && npm run test:web`: 20 testes passaram; seis novos testes do Pairing
  usam transporte/core de fixture (sucesso somente após salvamento, falha no salvamento, retry após
  primeiro Ready, prazo durante pedido travado, cancelamento e prazo preservado/migração de registro
  antigo). Um sétimo teste novo comprova que cancelamento durante a criptografia não grava a sessão.
  Não são E2E; a fixture de Pairing não valida criptografia ou um relay real.
- `cargo test -p roda-ffi pickup_ --lib`: duas regressões falharam com a função original de main
  (sem reconexão; timeout de conexão ignora o prazo do pickup). A mesma checagem passou com a correção.
- `cargo test -p roda-ffi net::tests --lib`: 12 testes passaram, incluindo as duas regressões
  e o novo teste de prazo já vencido sem abrir conexão. Fixture TCP, não vínculo real entre aparelhos.
- `cargo test -p zoen-web-core --lib`: quatro testes passaram, incluindo serialização do pedido
  Unlink com validação da chave, usando a operação autenticada existente, sem mudar autorização.
- `swiftc -frontend -parse apple/Shared/Features/Settings/DeviceConnectionSection.swift apple/Shared/Features/Settings/SettingsScreens.swift`:
  passou; valida sintaxe, não tipos/integração com bindings.
- Compilação Android local indisponível: `java -version` informa ausência de Java Runtime.
- Browser no mesmo laptop, componente real `Devices` com dados inventados em
  `packages/zoen-ui/web/e2e/devices-fixture.tsx`: confirmação aparece; cancelar preserva aparelho;
  falha de atualização mostra erro; nova tentativa recupera; confirmar remove o aparelho da fixture.
  Captura inspecionada. A ferramenta retornou a imagem sem caminho de arquivo exportável para anexar à PR.

## Lacunas

Não foram executados simuladores/emuladores no laptop, jornadas reais em dois aparelhos,
servidor de produção, transferência de histórico, revogação E2E nem build completo Apple local.
Testes determinísticos e a fixture visual não comprovam entrega, recepção ou exclusão de mensagens
num aparelho real. O build web WASM completo não foi executado; typecheck, bundle da fixture e
os testes Rust host não equivalem a esse build.

A PR permanece rascunho até concluir a CI e a revisão das lacunas nativas. Resultados de CI
são específicos de cada commit; não confundir um build verde anterior com o head mais recente.
