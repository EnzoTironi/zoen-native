# Frente f — aparelhos e retirada segura do vínculo

Escopo: linha “Devices + link pickup” do inventário em
`origin/reason/task-a1e26d3e:docs/reviews/messenger-parity.md`.
Não inclui recuperação/senha (g), threads, presença, mídia ou mudanças na rejeição de aparelhos revogados.

## Alterações

- Apple/Android: entrada de código em texto, conferência explícita dos seis números antes de aprovar,
  lista atualizável, erros recuperáveis e confirmação antes de desconectar outro aparelho.
- Web: tela Aparelhos com atualização e desconexão; a aprovação de novos aparelhos continua no celular,
  pois o navegador não guarda a chave raiz da conta. A orientação agora descreve o caminho de colar código,
  sem prometer um leitor de QR que não existe nestas telas.
- Pickup nativo: reconexão em falhas de transporte; um prazo absoluto cobre conexão,
  autenticação, leitura, escrita e espera. Recusas explícitas continuam terminais. Cancelar a future
  cancela a conexão; não foi alterada a autorização do servidor.
- Pickup web: prazo de 15 minutos guardado junto ao pedido criptografado, preservado ao recarregar;
  registros antigos sem prazo e códigos vencidos são substituídos. Rejeição de pedido após Ready
  não encerra o transporte e agenda nova tentativa. Cancelamento/prazo impedem que respostas tardias
  iniciem confirmação ou salvamento. Um salvamento já iniciado não é transacionalmente cancelável.

## Evidência local

- `cd packages/zoen-ui && npm run typecheck:web`: passou.
- `cd packages/zoen-ui && npm run test:web`: 17 testes passaram; quatro novos testes do Pairing
  usam transporte e relógio de fixture (retry após primeiro Ready, prazo durante pedido travado,
  cancelamento e prazo preservado/migração de registro antigo). Não são E2E.
- `swiftc -frontend -parse apple/Shared/Features/Settings/DeviceConnectionSection.swift apple/Shared/Features/Settings/SettingsScreens.swift`:
  passou; valida sintaxe, não tipos/integração com bindings.
- Testes Rust de prazo/reconexão/cancelamento: em execução no primeiro marco de publicação.
- Compilação Android local indisponível: `java -version` informa ausência de Java Runtime.

## Lacunas

Não foram executados simuladores/emuladores, jornadas reais em dois aparelhos, servidor de produção,
transferência de histórico, revogação E2E nem build completo Apple/Android. Testes determinísticos
não comprovam entrega, recepção ou exclusão de mensagens num aparelho real. Evidência visual, se
adicionada, será identificada como fixture; nenhuma captura de fixture comprova vínculo real.
