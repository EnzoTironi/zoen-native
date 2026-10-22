[Research index](e2b-sandbox.md).

## 5. Proposta: o "computador do agente" no Zoen

Pedido do Enzo (09/10): "uma área de trabalho de verdade, como a do Grok Bot, e um card bonito
seguindo os padrões". Hoje o Zoen tem o navegador do agente com tela ao vivo selada e takeover
(ADR 0028 §7 "As built in P2", PR #25). A proposta abaixo cresce a partir disso, sem trocar a
criptografia.

### 5.1 O que a pessoa vê

**No chat: o card "Computador do Zoen".**
- Miniatura ao vivo (2–4 quadros por segundo, só enquanto o card está visível na tela),
  cantos e sombra do padrão dos cards de mini-app, avatar do agente no canto.
- Uma linha de status em linguagem simples: "Abrindo mercadolivre.com.br", "Preenchendo o
  endereço", "Esperando você".
- Barra fina de passos (1 de 5) e o tempo. Um cadeado pequeno com "só você vê".
- Quando o agente para, a miniatura vira a última imagem (não continua transmitindo).
- Quando o site pede senha, 2FA ou CAPTCHA, o card vira o pedido "Zoen precisa de você" com o
  botão **Assumir**, com vibração de aviso (já existe no fluxo do PR #25).

**Ao tocar: a tela cheia "Computador".**
- A moldura é **nativa** (SwiftUI): abas abertas, endereço do site, lista de arquivos baixados,
  um mini-terminal só leitura quando houver, e a linha do tempo de passos. Só a área da página
  é vídeo. Assim o desktop fica com cara de Zoen (papel, tinta, fontes do app) e não de XFCE.
- Gestos: pinça para dar zoom, arrastar para mover, girar o telefone para paisagem.
  Voltar fecha só esta tela.
- Linha do tempo: cada passo importante guarda uma imagem (selada) para a pessoa voltar e ver
  "o que ele fez". Sem gravar senha: na hora de digitar senha a imagem salva é borrada.
- Botão **Assumir** sempre visível. Ao assumir: borda colorida, vibração forte, o agente
  congela, a barra de teclado ganha teclas especiais (Tab, Esc, setas, colar código). Dois
  modos de toque: direto (tocar = clicar ali) e trackpad (para alvos pequenos).
- Botão **Devolver ao Zoen**, com confirmação no próprio lugar; o agente recebe só "pronto,
  pode seguir", nunca o que foi digitado.

**No Mac:** a mesma tela em janela, com teclado e mouse físicos repassados ao vivo. Na
coluna da direita do layout de três colunas.

### 5.2 Como funciona por baixo

```
VM (Firecracker, T2 "desktop")                     servidor                   aparelho do dono
┌─────────────────────────────────┐   vsock    ┌──────────────┐  relay   ┌──────────────────┐
│ Chromium (sem moldura)          │  1081      │ zoen-sandboxd│  (só     │ zoen-liveview    │
│ servidor de display headless    │──────────▶ │ repassa      │ ───────▶ │ abre e decodifica│
│ zoen-guestd::desktop            │  quadros   │ ciphertext   │ cifrado) │ VideoToolbox     │
│  ├ captura por dano (só o que   │  selados   └──────────────┘          │ SwiftUI (moldura │
│  │ mudou) → H.264/JPEG          │                                       │ nativa)          │
│  ├ sela cada quadro (X25519 +   │ ◀──────── entrada selada, com sequência ────────────────┘
│  │ HKDF + ChaCha20-Poly1305)    │
│  └ injeta toque/tecla (CDP ou   │
│    XTEST num processo fixo)     │
└─────────────────────────────────┘
```

1. **Dois modos de tela, mesma chave.**
   - *Modo navegador* (o que já existe): CDP `Page.startScreencast` em JPEG, 48–64 ms até o
     primeiro quadro, ~8 KiB por quadro. Suficiente para o card e para acompanhar.
   - *Modo desktop* (novo, quando o agente usa mais de um app: planilha, arquivos, terminal):
     um servidor de display sem tela (Xvfb, ou um compositor Wayland headless), captura só as
     regiões que mudaram (XDamage) e codifica em vídeo (H.264 por padrão, VP8/VP9 como
     alternativa) num processo separado. Cada unidade de vídeo é selada por `zoen-liveview`,
     do mesmo jeito que os JPEGs.
2. **Sem servidor de vídeo no meio.** O nó e o relay só repassam bytes cifrados. Sem noVNC,
   sem websockify, sem senha em URL. A sessão nasce de uma chave nova da VM e da chave do
   aparelho, como no P2.
3. **Entrada por um processo fixo, não um processo por clique.** O `zoen-guestd` mantém a
   conexão com o Chromium (CDP `Input.dispatch*Event`) ou com o X (XTEST) aberta. Corta as
   dezenas de ms de `fork/exec` por ação que a E2B paga com `xdotool`.
4. **Assumir é exclusivo.** Enquanto a pessoa controla, a VM recusa tudo do modelo (navegador,
   exec, arquivos: `TAKEOVER_IN_PROGRESS`), como no P2. Só o `Done` selado do aparelho
   devolve. Diferente do `-shared` da E2B.
5. **Desktop já ligado no snapshot.** Display, gerenciador de janelas e Chromium sobem no build
   do template, não a cada sessão (a E2B paga segundos por isso). Restore do pool quente:
   alvo < 100 ms, como os 74–77 ms do navegador hoje.
6. **Só transmite com alguém olhando.** Card fora da tela: 0 quadros. Card visível: 2–4 qps.
   Tela cheia: até 30 qps, adaptando ao Wi-Fi/4G (o aparelho manda "recebi o quadro N"; a VM
   pula quadros se ele atrasar, como o `ack` do screencast).
7. **Depois (P5): WebRTC.** Se o vídeo pelo relay não ficar fluido no 4G, usar WebRTC entre a
   VM e o aparelho. O DTLS-SRTP já é ponta a ponta entre os dois. Para que nem o nosso
   servidor de sinalização possa se meter no meio, as impressões digitais DTLS das duas pontas
   vão **dentro** do canal selado do `zoen-liveview`. O TURN só vê bytes cifrados.

Metas de latência (a medir no P2.5): toque do dono → tela atualizada < 150 ms no mesmo país;
primeiro quadro < 300 ms depois de tocar no card; 0,3–1,5 Mbps em tela cheia, ~30 KB/s na
miniatura.

### 5.3 Riscos e cuidados
- **Patentes de vídeo.** H.264 tem patentes; o decode no iPhone já é licenciado pela Apple,
  mas o *encode* nos nossos servidores precisa de revisão jurídica. Alternativas sem royalty:
  VP8/VP9 (libvpx, BSD) ou JPEG/WebP por região alterada. Recomendação: começar com
  JPEG/WebP por região (o caminho atual) e só ligar H.264 depois da revisão.
- **Licenças dentro da VM.** Se usar x11vnc, ffmpeg ou x264, só como processos separados
  dentro da VM, nunca ligados aos nossos binários. A preferência é encoder sem copyleft
  (openh264 BSD via o binário da Cisco, ou libvpx).
- **Memória.** Um desktop com Chromium e XFCE gasta mais que o navegador sem tela. Orçar
  2–3 GiB por VM até medir. Sem XFCE: a moldura é do app, então um gerenciador de janelas
  mínimo basta.
- **Privacidade das imagens da linha do tempo:** seladas à chave do dono, apagadas na
  derrubada da tarefa, como os snapshots (ADR 0028 §1).

### 5.4 Fases sugeridas (encaixe no plano do ADR 0028)

| Fase | Entrega | Prova |
|---|---|---|
| P2.5 (app) | Card "Computador do Zoen" + tela cheia nativa sobre o screencast atual; assumir/devolver; linha do tempo | journey de UI: card ao vivo, assumir, devolver; o modelo não vê o que foi digitado |
| P3.5 (servidor) | Template T2 "desktop" com display headless no snapshot; captura por dano; entrada por processo fixo; regiões em JPEG/WebP seladas | restore < 100 ms; toque → tela < 150 ms na box |
| P4 | Codec de vídeo (após revisão de patentes), adaptação de taxa, Mac com teclado/mouse | Mbps e qps medidos em 4G |
| P5 | WebRTC com impressões digitais no canal selado | mesma garantia de ponta a ponta, menos latência |

---
