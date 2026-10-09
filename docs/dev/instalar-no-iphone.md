# Instalar o Zoen no seu iPhone com Apple ID grátis

Sem a conta paga de desenvolvedor (US$ 99/ano) dá para rodar o Zoen no **seu** iPhone com o
"Personal Team" que o Xcode cria para qualquer Apple ID. O projeto tem um esquema só para isso,
**`Zoen-Device`**: bundle id `xyz.tironi.zoen.dev` (não ocupa o `xyz.tironi.zoen` da conta paga),
nenhum time fixo e o servidor de staging (`relay.tryzoen.com`).

Uma linha, na pasta do repositório atualizada com o `main`, para gerar o projeto, conferir que
compila para iPhone e abrir o Xcode:

```sh
git pull && scripts/device.sh
```

## Passo a passo (uma vez)

1. **Xcode > Settings > Accounts**, botão **+**, **Apple ID**, entre com o seu Apple ID.
   Ele aparece como "Seu Nome (Personal Team)".
2. **Plugue o iPhone no Mac** com o cabo, desbloqueie e toque em **Confiar** neste computador.
3. **Ative o Modo de Desenvolvedor no iPhone**: Ajustes > Privacidade e Segurança >
   Modo de Desenvolvedor > ligar. O iPhone reinicia; ao voltar, confirme **Ativar**.
   (A opção só aparece depois que o iPhone foi plugado num Mac com Xcode.)
4. No Xcode, clique no projeto **Zoen** > target **ZoeniOS** > aba **Signing & Capabilities**
   > **Team**: escolha **Seu Nome (Personal Team)**. Deixe "Automatically manage signing" ligado.
5. No topo da janela, escolha o esquema **Zoen-Device** e, ao lado, o **seu iPhone**.
6. Aperte **▶ (Run)**. A primeira vez demora (compila tudo).
7. Se o iPhone disser "Desenvolvedor não confiável": **Ajustes > Geral > VPN e Gerenciamento de
   Dispositivos** > o seu Apple ID > **Confiar**. Depois toque no ícone do Zoen.

Pelo Terminal, sem clicar no Xcode (depois do passo 1 a 3): o Team ID aparece em
Xcode > Settings > Accounts > seu Apple ID > Personal Team.

```sh
ZOEN_TEAM=SEU_TEAM_ID scripts/device.sh install
```

## Toda semana

O app gratuito **expira em 7 dias**: ele para de abrir. Plugue o iPhone e aperte **▶** de novo
(ou rode o comando acima). Os seus dados no aparelho continuam lá.

Se você rodar `scripts/device.sh` (ou `xcodegen`) de novo, o projeto é recriado e o Team volta a
ficar vazio: repita o passo 4.

## Limites do Apple ID grátis

Da Apple ([conta de desenvolvedor](https://developer.apple.com/help/account/basics/about-your-developer-account/),
[capabilities por tipo de conta](https://developer.apple.com/help/account/reference/supported-capabilities-ios)):

- o app instalado e o perfil valem **7 dias**;
- até **3 aparelhos** e **3 apps** por aparelho;
- até **10 App IDs**, que também expiram em 7 dias;
- sem TestFlight e sem App Store: só no seu aparelho, plugado no seu Mac;
- sem push, iCloud, App Groups, Associated Domains (passkeys, links universais) e Keychain sharing.

## O que fica desligado nessa versão

Hoje o Zoen não declara nenhuma capability paga (o projeto não tem arquivo de entitlements), então
**tudo que já existe no app funciona igual**, inclusive câmera, microfone, transcrição no aparelho,
localização, calendário, contatos e o modelo da Apple no aparelho (em iPhones com Apple
Intelligence). O que ainda não existe no app e, quando existir, fica fora desta versão:

- **notificações push** com o app fechado;
- **passkeys e links `tryzoen.com` abrindo o app** (Associated Domains);
- **widgets e extensões** que dividem dados com o app (App Groups);
- **backup no iCloud**.

Quando alguma dessas entrar no código, ela checa `Flags.personalDevice` e fica quieta nesta
versão, sem travar e sem aviso técnico. Os entitlements dela ficam fora da config `Device`.

## Servidor

O iPhone não alcança o `127.0.0.1` do Mac, então o `Zoen-Device` aponta para o staging,
`https://relay.tryzoen.com`.

**Estado em 9/10/2026:** o staging está no ar, mas roda uma versão antiga do servidor. Cadastro
funciona; a conexão de sincronização cai, porque o app atual já manda as chaves da criptografia de
grupos (M2), que o servidor antigo não conhece. Volta a funcionar quando o staging for atualizado
(precisa do `fly auth login` no Mac). Enquanto isso, três saídas grátis:

- **Ver o app sem servidor:** em Product > Scheme > Edit Scheme > Run > Arguments, marque
  `-RodaDemo YES`. Dados de demonstração, tudo local.
- **Servidor no Mac, iPhone no mesmo Wi-Fi:** rode `scripts/dev-stack.sh` (ele escuta em
  `0.0.0.0:8787`) e marque `-RodaRelay http://SEU-MAC.local:8787` trocando `SEU-MAC` pelo nome em
  Ajustes do Sistema > Geral > Compartilhamento > Nome do host local. Na primeira vez o iPhone
  pede acesso à rede local: aceite.
- **Servidor no Mac, iPhone em qualquer rede (4G):** com o `dev-stack.sh` rodando,
  `cloudflared tunnel --url http://127.0.0.1:8787` imprime um endereço
  `https://….trycloudflare.com`, grátis e sem conta. Marque `-RodaRelay` com esse endereço.
  O endereço muda cada vez que o túnel reinicia, e fica público enquanto estiver rodando.
  O túnel precisa de saída na porta 7844 (normal em casa; na rede do box ela é bloqueada, então
  esse caminho ainda não foi testado de ponta a ponta).

Pelo Terminal, o mesmo vale com `ZOEN_RELAY=<endereço> ZOEN_TEAM=… scripts/device.sh install`.
