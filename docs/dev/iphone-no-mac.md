# O app de iPhone no Mac, sem simulador

Num Mac com Apple Silicon o app de iPhone roda nativo, numa janela no formato de iPhone
("My Mac (Designed for iPhone)"). É o jeito padrão de testar o app de iPhone sem simulador
e sem iPhone. Usa a config `Device`: bundle `xyz.tironi.zoen.dev`, só iPhone
(`TARGETED_DEVICE_FAMILY = 1`, por isso a janela tem formato de iPhone e não de iPad),
`SUPPORTS_MAC_DESIGNED_FOR_IPHONE_IPAD = YES` e o relay de staging (`https://relay.tryzoen.com`).

## Rodar

```sh
scripts/mac-iphone.sh
```

O script compila, assina com o seu Personal Team (Apple ID grátis, o primeiro time em
Xcode > Settings > Accounts) e abre o app. Se não houver time, adicione seu Apple ID no
Xcode uma vez. Outro servidor: `ZOEN_RELAY=http://127.0.0.1:8787 scripts/mac-iphone.sh`.

Pelo Xcode: esquema `Zoen-Device`, destino **My Mac (Designed for iPhone)**, Run.

Um `.app` de iOS não abre com `open` direto ("incorrect executable format"); o script monta
o invólucro que o macOS espera (`build/ZoenPhone.app/Wrapper/Zoen.app` +
`WrappedBundle`). O perfil do Personal Team precisa incluir o Mac, e o
`-allowProvisioningUpdates` cuida disso. Ele também expira a cada 7 dias: rode o script de novo.

## O que funciona e o que não

| | No Mac (Designed for iPhone) |
| --- | --- |
| Telas, navegação, sincronização, MLS | Iguais ao iPhone |
| Toques, rolagem, arrastar | Com mouse/trackpad: clique = toque, arrastar com o botão pressionado = deslizar; rolagem pelo trackpad |
| Deslizar para voltar, gestos de borda | Não há borda de tela; use o botão Voltar |
| Pinça e gestos com vários dedos | Só os que o trackpad traduz (pinça vira zoom) |
| Hápticos | `UIImpactFeedbackGenerator` não faz nada no Mac (sem erro). O app de Mac nativo usa `NSHapticFeedbackManager` no trackpad; este não |
| Câmera | A câmera do chat (`InlineCamera`) pede a traseira (`position: .back`), que o Mac não tem: deve cair no estado "indisponível" (não testado). Para usar a FaceTime do Mac seria preciso cair para `.front`/`.unspecified` |
| Microfone e transcrição | Funcionam com o microfone do Mac (pedem permissão) |
| Teclado | O teclado físico; o teclado da tela e o comportamento dele (por exemplo os testes de teclado) não aparecem |
| Notificações e passkeys | Fora, como no Apple ID grátis no iPhone |

## Testes de interface (XCUITest)

**Não rodam neste destino.** O `xcodebuild` recusa:

```sh
xcodebuild -project apple/Zoen.xcodeproj -scheme ZoenUITests \
  -destination 'platform=macOS,arch=arm64,variant=Designed for iPad' \
  -allowProvisioningUpdates DEVELOPMENT_TEAM=<seu time> CODE_SIGNING_ALLOWED=YES test
# error: Cannot test target "ZoenUITests" on "My Mac": UI tests are not supported on My Mac (Designed for iPad)
```

As jornadas de UI continuam no simulador (ou num iPhone de verdade). Observação para quem
for tentar: o destino é `variant=Designed for iPad` mesmo para app só de iPhone; a forma
`Designed for [iPad,iPhone]` que o `-showdestinations` mostra quebra no parser por causa da vírgula.
