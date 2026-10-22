[Research index](e2b-sandbox.md).

## 2. Como funciona o desktop da E2B

### 2.1 Pilha
Template em `desktop:template/template.py` (Ubuntu 22.04):
- **Xvfb** (servidor X sem tela) + **XFCE4** + `tint2`, LibreOffice, Firefox ESR, Chrome,
  VS Code.
- **x11vnc** expõe o display em RFB na porta 5900; **noVNC** (fork `e2b-dev/noVNC`, branch
  `e2b-desktop`) + **websockify v0.12.0** convertem para WebSocket na porta 6080; o navegador
  do usuário abre `https://6080-<id>.<domínio>/vnc.html`.
- O SDK (que agora mora em `E2B:packages/desktop-js/src/sandbox.ts`, 763 linhas) é quem liga
  tudo **depois** de criar a sandbox:
  - `_start` (~linha 529): `Xvfb :0 -ac -screen 0 1024x768x24 -retro -dpi 96 -nolisten tcp -nolisten unix`, espera `xdpyinfo`, depois `startxfce4`;
  - `VNCServer.start` (~linha 685–741): `x11vnc -bg -display :0 -forever -wait 50 -shared -rfbport 5900 [-rfbauth ~/.vnc/passwd]` e `novnc_proxy --vnc localhost:5900 --listen 6080`.

### 2.2 APIs de "computer use"
Cada ação é um **processo novo** pedido ao envd (`commands.run`):

| Método do SDK | Executa no convidado |
|---|---|
| `screenshot()` | `scrot --pointer /tmp/x.png` e depois lê o arquivo (2 idas e voltas) |
| `leftClick/rightClick/doubleClick(x,y)` | `xdotool mousemove --sync x y` + `xdotool click 1` |
| `write(text)` | `xdotool type --delay N -- '<texto>'` |
| `press(keys)` | `xdotool key ctrl+c` |
| `scroll`, `drag`, `mousePress/Release` | `xdotool click --repeat`, `mousedown/up` |
| `getScreenSize`, `getCursorPosition`, janelas | `xrandr`, `xdotool getmouselocation/search/getwindowname` |
| `launch(app)`, `open(url)` | `gtk-launch`, `xdg-open` |

### 2.3 Latência (estimada a partir do código)
- Uma ação = RPC ao envd por TLS + `fork/exec` do `xdotool` + conexão ao X: algumas dezenas de
  ms dentro do datacenter, mais a ida e volta da rede do cliente. `screenshot` são duas.
- O x11vnc varre a tela com `-wait 50` (até 50 ms entre varreduras) e codifica em RFB
  (tight/JPEG). O noVNC desenha num `<canvas>`. Na prática, 100–300 ms entre o agente agir e
  o humano ver, dependendo da rede. A E2B não publica número oficial.
- Desktop + XFCE sobem a cada `Sandbox.create()` (não estão no snapshot), o que soma
  segundos à primeira tela.

### 2.4 Takeover e segurança da tela
- Não existe "assumir": o link é interativo ou `view_only=true` (`getUrl`, ~linha 655).
  Com `-shared`, humano e agente mexem **ao mesmo tempo**.
- A senha do VNC (16 caracteres aleatórios; o VNC clássico só usa os 8 primeiros) vai **na
  query da URL** (`url.searchParams.set('password', authKey)`). Fica em histórico, logs e
  `Referer`.
- A imagem é TLS até a borda da E2B; ali e no nó ela está em claro. Não há cifragem de ponta
  a ponta até o aparelho.
- Um stream por vez por sandbox.

### 2.5 Code interpreter, em uma linha
`ci:template/template.py` instala Jupyter com kernels Python, JS, R, Bash e Java;
`ci:template/start-up.sh` sobe `jupyter server` sem token e um FastAPI em `:49999`
(`ci:template/server/main.py`) que mantém "contextos" com estado e devolve resultados ricos
(gráficos extraídos por `ci:chart_data_extractor`). Ideia útil para nós: um template T1
"Python com estado", com o kernel já quente no snapshot.

---
