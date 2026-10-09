# E2B por dentro: o que o Zoen copia, adapta ou evita no "computador do agente"

> Pesquisa feita em 09/10/2026 lendo o código-fonte aberto da E2B, para orientar o
> [ADR 0028](../adr/0028-agent-sandbox.md) (sandboxes próprias) e o `zoen-agentd`,
> `zoen-sandboxd`, `zoen-guestd`, `zoen-egress` e `zoen-liveview`.
> Complementa [agent-sandbox.md](agent-sandbox.md), que tem as nossas medições (P1 e P2) e o
> modelo de custo. Este documento é só pesquisa e desenho: não muda código.

## Versões lidas (commits fixos)

| Repositório | Commit (HEAD em 09/10/2026) | Data do commit (BRT) | O que tem |
|---|---|---|---|
| [e2b-dev/infra](https://github.com/e2b-dev/infra/tree/79fcdf59b093eddda444a76932a4d140850b71c6) (o README já chama de "E2B Runtime", `e2b-dev/runtime`) | `79fcdf59b093eddda444a76932a4d140850b71c6` | 09/10/2026 01:50 | API, orquestrador, Firecracker, envd, proxy, build de templates |
| [e2b-dev/E2B](https://github.com/e2b-dev/E2B/tree/08945952ccefc1f74a9929e5ae4bc561c8b9aaf0) | `08945952ccefc1f74a9929e5ae4bc561c8b9aaf0` | 08/10/2026 14:24 | SDKs JS/Python, CLI e, desde 2026, também os SDKs de desktop e code interpreter |
| [e2b-dev/desktop](https://github.com/e2b-dev/desktop/tree/1ff98a36306989d155ce5eceab2a2d38c9a8d6d2) | `1ff98a36306989d155ce5eceab2a2d38c9a8d6d2` | 30/09/2026 08:05 | template do desktop (Xvfb, XFCE, VNC) e exemplos |
| [e2b-dev/code-interpreter](https://github.com/e2b-dev/code-interpreter/tree/1d3de4589c616c7a3405e0ea3ac5b4b22b264f55) | `1d3de4589c616c7a3405e0ea3ac5b4b22b264f55` | 08/10/2026 13:56 | template com Jupyter e um servidor FastAPI |

Abaixo, `infra:caminho` quer dizer o arquivo nesse commit do `e2b-dev/infra`
(link base: `https://github.com/e2b-dev/infra/blob/79fcdf5/caminho`). O mesmo vale para
`E2B:`, `desktop:` e `ci:` (code-interpreter). A fonte principal da arquitetura é o próprio
[`infra:docs/ARCHITECTURE.md`](https://github.com/e2b-dev/infra/blob/79fcdf59b093eddda444a76932a4d140850b71c6/docs/ARCHITECTURE.md),
que o repositório exige manter atualizado a cada PR. Conferimos os pontos citados no código.

---

## 0. Resumo em 10 pontos

1. **Na E2B, criar uma sandbox é retomar um snapshot.** O template é uma VM já ligada e
   congelada (memória + disco + estado). "Criar", "retomar depois de pausar" e "bifurcar"
   passam pelo mesmo caminho. É o que o ADR 0028 já decidiu. O código oferece uma
   referência de implementação; nossa capacidade ainda precisa de medições próprias.
2. **A memória chega sob demanda.** Um handler de `userfaultfd` entrega cada página só quando o
   convidado a toca, lida direto do arquivo de memória do template. Um passo "optimize" no
   build grava quais páginas o boot toca e vira dica de pré-carga. É o item que mais falta no
   nosso P1.
3. **O disco é cópia-na-escrita em espaço de usuário.** A imagem base fica só leitura; as
   escritas vão para um cache por sandbox servido ao Firecracker por um servidor NBD dentro
   do orquestrador. Pausar exporta só os blocos sujos. Template e pausa têm o **mesmo
   formato**: uma cadeia de diffs com um índice (`.header`). Vale copiar esse formato.
4. **Plano de controle e plano de dados nunca se misturam.** A API escolhe *onde*; o nó decide
   *como*; o tráfego da sandbox não passa pela API. O nó publica o próprio registro de rota,
   e tráfego para uma sandbox pausada a acorda sozinho. Copiamos isso, trocando Redis por
   FoundationDB.
5. **A rede usa uma fronteira diferente.** Cada VM tem uma interface de rede (tap, netns,
   nftables), e a lista de domínios é decidida pelo SNI/Host. Nosso desenho (VM sem placa de
   rede, só vsock até o `zoen-egress`, que resolve DNS ele mesmo) é mais fechado. Não copiar.
6. **O isolamento do processo segue outro desenho.** O Firecracker deles sobe por
   `unshare -m` + `ip netns exec`, como root, sem `jailer`. Mantemos jailer, seccomp, um uid
   por VM e core scheduling.
7. **O desktop da E2B é simples e antigo:** Xvfb + XFCE + x11vnc + noVNC no navegador. Cada
   clique do agente é um processo `xdotool` novo, a senha do VNC vai na URL e humano e agente
   mexem ao mesmo tempo, sem trava. A imagem não é cifrada de ponta a ponta. Serve de
   referência do que **não** fazer na tela ao vivo do Zoen.
8. **Tudo que importa é Apache-2.0** (infra, SDKs, desktop, code-interpreter). Mas o desktop
   depende de x11vnc (GPL-2.0), websockify (LGPL-3.0) e noVNC (MPL-2.0), e o resolvedor de
   segredos deles fica num `orchestrator-ee` fechado. Podemos ler e portar ideias; o que roda
   no telefone escrevemos nós.
9. **O "computador do agente" no Zoen deve ser uma tela nativa com conteúdo transmitido:** a
   moldura (abas, endereço, passos, botão "Assumir") é SwiftUI; só os pixels da página chegam
   pelo vídeo, selados para o aparelho do dono. Assumir o controle é exclusivo, com
   sequência e assinatura, como o `zoen-liveview` já faz.
10. **Custo:** uma hora de desktop com navegador na E2B (2 vCPU, 4 GiB) sai a **US$0,166**; no
    nosso modelo em Hetzner, **≈US$0,017** parado e **≈US$0,028** transmitindo vídeo. É
   uma diferença projetada de 6 a 10 vezes nessas premissas, sem prova de custo em
   produção. O vídeo só existe enquanto alguém assiste.

---

## 1. Arquitetura da E2B

### 1.1 Peças

| Serviço | Onde no código | Faz o quê |
|---|---|---|
| API | `infra:packages/api` | REST público; ciclo de vida, posicionamento, autenticação, cotas |
| Orquestrador | `infra:packages/orchestrator` | Um binário Go **como root** em cada nó; liga Firecracker, rede, disco, cgroup |
| Template manager | mesmo binário, papel `template-manager` | Constrói templates a partir de imagens Docker |
| client-proxy | `infra:packages/client-proxy` | Borda: `https://<porta>-<id>.<domínio>` → nó certo |
| envd | `infra:packages/envd` | Agente dentro de cada VM (porta 49983): processos, PTY, arquivos, watchers |
| Dashboard API | `infra:packages/dashboard-api` | Backend do painel web |

Estado: Postgres (times, templates, builds, snapshots), Redis (sandboxes rodando, rotas,
limites de taxa), ClickHouse (métricas, eventos), armazenamento de objetos GCS/S3/Azure
(artefatos). Fonte: `infra:docs/ARCHITECTURE.md` §"Services" e §"Data stores".

### 1.2 Firecracker e o processo da VM
- Uma VM Firecracker por sandbox, em cgroup e namespace de rede próprios; configurada pela API
  HTTP do Firecracker num socket Unix; metadados (id da sandbox, hash do token do envd) vão
  por MMDS (`infra:packages/orchestrator/pkg/sandbox/fc/`).
- **Sem jailer.** O processo sobe assim (`infra:packages/orchestrator/pkg/sandbox/fc/process.go`
  ~linha 243 e `fc/script_builder.go`, `startScriptV2`):
  `unshare -m -- bash -c 'mount --make-rprivate / && mount -t tmpfs … && ln -s rootfs … && ip netns exec <ns> firecracker --api-sock …'`.
  Ou seja, namespace de montagem e de rede, mas sem chroot, sem uid sem privilégio e sem o
  filtro de seccomp do jailer (o seccomp embutido do Firecracker continua valendo).
- Firecracker próprio: eles mantêm um fork com patches, versionado como `vX.Y-<semver e2b>`
  (`infra:firecracker/fc-versions/README.md`). Kernels do convidado 6.1.x
  (`infra:firecracker/fc-kernels/kernel_versions.txt`: 6.1.102, 6.1.158, 6.1.177, 6.1.182),
  configs em `fc-kernels/configs/`.
- Entropia: cada VM recebe um dispositivo de entropia (`fc/config.go`: 1 KB, recarga
  a cada 100 ms; `fc/process.go` `setEntropyDevice`).
- Nós de sandbox usam **hugepages** (páginas de 2 MiB) na memória das VMs; o posicionamento
  pondera a carga do pool de hugepages.

### 1.3 Template: como nasce
Fonte: `ARCHITECTURE.md` §"Template build", código em
`infra:packages/orchestrator/pkg/template/build/phases/`.
1. Baixa a imagem Docker, injeta o envd e o provisionamento, extrai um `rootfs` ext4.
2. Liga uma VM por fase: `base → user → uma camada por passo da receita → resize do disco →
   finalize → optimize`. Cada camada é o *diff de pausa* da fase anterior e tem hash;
   rebuild só refaz o que mudou.
3. **optimize** liga o template e grava quais páginas de memória um resume toca: vira
   dica de pré-carga (`pkg/sandbox/uffd/prefetch/`).
4. Sobe para o armazenamento: `{buildID}/memfile`, `rootfs.ext4`, `snapfile`,
   `metadata.json` e os índices `.header`.

O SDK define templates em código (`desktop:template/template.py` é um bom exemplo:
`Template().from_image("ubuntu:22.04").apt_install([...]).git_clone(...)`).

### 1.4 Formato de disco, memória e snapshot
- **Template e pausa têm o mesmo formato.** Um snapshot é só um build novo cujos `memfile` e
  `rootfs` são diffs contra o pai; o `.header` resolve a cadeia.
- O header (`infra:packages/shared/pkg/storage/header/header.go`, serialização v3–v5 em
  `serialization_v5.go`) guarda, por build, tamanho, SHA-256 e uma tabela de frames
  (compressão opcional), e um **mapa por bloco** dizendo de qual build vem cada bloco. O mapa
  é compacto (9–10 bytes por entrada) e sempre em granularidade de página de 4 KiB, mesmo
  quando o arquivo de memória usa blocos de 2 MiB (`header/compact.go`,
  `compact_test.go`).
- **Memória preguiçosa (UFFD):** no resume, o Firecracker restaura a VM sem carregar memória;
  um handler `userfaultfd` no orquestrador atende cada falta de página lendo do `memfile` do
  template (cache local → NFS → peer → objeto). Há modo de proteção contra escrita para
  rastrear páginas sujas na pausa (`pkg/sandbox/uffd/userfaultfd/`, testes `sync_wp_*`,
  `cow_export.go`).
- **Disco COW via NBD:** `pkg/sandbox/rootfs/`, `nbd/`, `block/`. O rootfs base é só leitura; um
  servidor NBD em espaço de usuário dentro do orquestrador serve um cache COW por sandbox.
  Na pausa, os blocos sujos viram o diff. Com a flag `deferred-rootfs-export`, o cache é
  ejetado e "selado" (reflink) em segundo plano, fora do caminho crítico da pausa.
- **Cache de templates** no disco do nó, opcionalmente num cache NFS compartilhado e até
  **de nó para nó** antes de o upload terminar (`ChunkService`). Pinos impedem despejar um
  template em uso.

### 1.5 Tempo de partida (números públicos)

| Medida | Número | Fonte |
|---|---|---|
| Promessa da E2B | ~150 ms para "criar" (é um resume) | material da E2B, repetido em [jacar.es](https://jacar.es/en/e2b-code-sandbox-for-agents/) |
| Restore de snapshot otimizado (fornecedor) | ~80 ms | citado por [SemSwitch](https://semswitch.com/learn/anatomy-of-a-true-cold-start) |
| Criação pública medida por terceiros, ponta a ponta com rede | p50 467 ms, p95 750 ms (abr/2026) | [Podflare](https://podflare.ai/blog/cloud-sandbox-benchmark-e2b-daytona-podflare) |
| Idem, outro teste | 717 ms | LogRocket via SemSwitch |
| Caminho do orquestrador sob concorrência | 100 ms com 1 criação; **1.063 ms com 100 simultâneas**, por cópia da tabela de montagens no `unshare -m`; correção proposta mantém 44–52 ms | [infra#3012](https://github.com/e2b-dev/infra/issues/3012) |
| **Nosso** (P1/P2, box com KVM aninhado) | restore até o convidado responder p50 17,8 ms; pegar do pool quente 22–38 ms; navegador 74–77 ms | [agent-sandbox.md §4–5](agent-sandbox.md) |

Lição: o gargalo sob carga não foi o Firecracker, foi o *host* (tabela de montagens enorme
copiada a cada VM). No P4 precisamos medir com 100 criações simultâneas, não uma por vez.

### 1.6 Pausa, retomada, checkpoint e fork
Fonte: `ARCHITECTURE.md` §"Pause and resume".
- **Pausa:** a API grava a linha do snapshot no Postgres e chama `Pause` no nó; o nó congela,
  faz o diff de memória (páginas sujas) e disco (COW), guarda local e sobe assíncrono com
  orçamento de retentativas. Orçamento total: 80 s; chave de transição no Redis com TTL 95 s.
- **Admissão de disco antes da pausa:** o nó recusa (de forma retentável) uma pausa que não
  cabe no disco: memória + cache do rootfs + snapfile, com folga configurável.
- **Resume:** igual à criação, mas prefere o **nó de origem**, onde o snapshot ainda está em
  cache (zero leitura de objeto).
- **Checkpoint:** pausa + resume no lugar, para persistir sem parar.
- **Fork:** checkpoint do original e até **100 sandboxes novas** por pedido, espalhadas
  entre nós.
- **Snapshot só de disco** (`memory: false`): quando a memória não serve, boot a frio a partir
  do disco; antes do boot roda `e2fsck -p -E journal_only` preso num jail (só replay de
  journal). Se uma auto-pausa ficar recusada além de 120 s, vira snapshot só de disco.
- **Auto-pausa e auto-resume:** sandboxes ociosas pausam; tráfego na URL as acorda. No SDK,
  `lifecycle.onTimeout: 'pause' | 'kill'` (`E2B:packages/js-sdk/src/sandbox/sandboxApi.ts`
  ~linha 524–542).
- **Upgrade do envd ao vivo:** `POST /upgrade` envia o binário novo e o envd faz `exec` **com
  o mesmo PID**, levando stdio, PTYs, tabela de processos e watchers num blob em tmpfs
  (`infra:packages/envd/internal/services/process/upgrade.go`). Os processos do usuário ficam
  congelados até o `/init` seguinte devolver o token. Para envd velho demais, o binário é
  trocado no rootfs antes do boot com `debugfs` preso num jail, nunca montando a imagem do
  cliente no kernel do host.

### 1.7 Rede e saída (egress)
- Cada sandbox recebe um *slot*: netns + par veth + tap, IP único de um /16, NAT, e **firewall
  nftables por slot** (`infra:packages/orchestrator/pkg/sandbox/network/firewall.go`,
  tabela `slot-firewall`, cadeia em PREROUTING). Slots são reaproveitados de um pool.
- **Lista de domínios por SNI/Host:** `infra:packages/orchestrator/pkg/tcpfirewall/proxy.go`
  redireciona a porta 443 para um ouvinte que lê o SNI do TLS e a 80 para um que lê o `Host`
  (`handlers.go`, `domainHandler`). Decide permitir/negar pelo nome que o *cliente* diz.
- **Segredos como marcadores:** o cliente guarda um marcador; quem troca pelo valor real, na
  saída, é o `orchestrator-ee` (fechado). A API nunca vê o valor (`ARCHITECTURE.md`
  §"Secrets").
- **Identidade de carga:** a sandbox ganha tokens curtos sem a API emitir credencial.
- **Entrada:** toda porta que um processo abre vira `https://<porta>-<id>.<domínio>`; o
  proxy do nó (:5007) exige um token de acesso por sandbox; as rotas internas do envd
  (`/init`, `/upgrade`, freeze/thaw) são marcadas `x-internal` e o proxy responde 404.

### 1.8 Multi-inquilino, agendamento e limites
- **Posicionamento best-of-K** (`infra:packages/api/internal/orchestrator/placement/`):
  sorteia K nós prontos, nota = maior entre carga de CPU e carga de hugepages, escolhe o
  menor; nó que recusa por falta de recurso continua elegível, com espera exponencial com
  jitter. K, overcommit e alfa ajustáveis ao vivo por feature flag.
- **Limites por time** (concorrência, duração máxima, taxa por grupo de API via Redis) vêm de
  `team_limits`; planos: Hobby 20 simultâneas e 1 h, Pro 100 e 24 h.
- `max_sandboxes` por nó, atualizado por flag a cada 30 s.
- Nós drenam antes de desligar: param de aceitar pausas, terminam uploads, depois fecham.
- Observabilidade: OpenTelemetry em tudo; eventos e stats de cgroup no ClickHouse.

### 1.9 Segurança: o que é forte e o que é fraco
**Forte:** VM por sandbox; token do envd por MMDS e só o hash fica guardado; rotas internas
bloqueadas na borda; segredos nunca passam pela API; `debugfs`/`e2fsck` em jail em vez de
montar disco do cliente; registro de rota com guarda por `execution_id` (um ciclo velho não
apaga o novo).

**Fraco (para nós):**
- Sem jailer, orquestrador root executa o Firecracker num namespace de montagem e rede, mas
  com o uid do root.
- A VM tem placa de rede. Uma falha no firewall do slot ou no NAT expõe o host ou outras VMs.
- Filtro por SNI/Host aceita o nome que o cliente declara; *domain fronting* e DNS próprio
  dentro da VM são vetores clássicos.
- O envd fala HTTP/Connect sobre TCP dentro da VM; qualquer porta aberta vira URL pública
  (com token opcional). No template do code interpreter o Jupyter roda com
  `--IdentityProvider.token=""` (`ci:template/start-up.sh` linha 18). Fica protegido só pelo
  token do proxy.

---

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

## 3. Licenças: o que dá para reaproveitar

| Parte | Licença | Pode entrar no Zoen? |
|---|---|---|
| `e2b-dev/infra` (todo o Go: orquestrador, envd, proxy, header, uffd) | Apache-2.0 (`infra:LICENSE`, único LICENSE do repo) | Sim, com aviso de atribuição e NOTICE. Na prática **portamos ideias para Rust**, não importamos Go. |
| `e2b-dev/E2B` (SDKs, CLI, desktop-js/python, code-interpreter-js/python) | Apache-2.0 (um LICENSE por pacote) | Sim |
| `e2b-dev/desktop` (template) | Apache-2.0 | Sim, mas é só uma receita |
| `e2b-dev/code-interpreter` | Apache-2.0 | Sim |
| Firecracker (fork deles) | Apache-2.0 | Já usamos o upstream |
| Kernel do convidado | GPL-2.0 | Só dentro da VM, como já decidido |
| x11vnc | GPL-2.0 | Só como programa separado dentro da VM; **não ligar** a nada nosso |
| websockify | LGPL-3.0 | Desnecessário para nós |
| noVNC | MPL-2.0 (copyleft por arquivo) | Não precisamos: o cliente é SwiftUI nativo |
| Xvfb / Xorg | MIT/X11 | OK |
| `orchestrator-ee` (resolve segredos na saída) | **fechado**, não está no repo | Não disponível; já temos o equivalente no `zoen-egress` |

Conclusão: nada da E2B impede o nosso caminho. O cuidado é com o que vai **dentro** da VM do
desktop (componentes GPL rodando como processos separados no nosso servidor, como o kernel) e
com patentes de vídeo (seção 5.3).

---

## 4. Copiar, adaptar ou evitar (mapeado no ADR 0028)

| # | Ideia da E2B | Onde está | Veredito | Onde entra no Zoen |
|---|---|---|---|---|
| 1 | Template = VM já ligada; criar = resume | `ARCHITECTURE.md` §1 | **Copiar** (já é o ADR 0028 §3) | `zoen-sandboxd` |
| 2 | Memória por `userfaultfd` com pré-carga gravada no build | `pkg/sandbox/uffd/`, `uffd/prefetch/`, fase optimize | **Copiar** (P1 pendente) | `zoen-sandboxd::uffd`, com crate `userfaultfd` (MIT/Apache) |
| 3 | Disco COW em espaço de usuário (NBD) com export de blocos sujos | `pkg/sandbox/{rootfs,nbd,block}` | **Adaptar**: o ADR fala em dm-thin; servidor de blocos em Rust nos dá o diff de graça e não depende de LVM no host. Avaliar `vhost-user-blk` em vez de NBD | `zoen-sandboxd::block` |
| 4 | Template e pausa com o mesmo formato (cadeia de diffs + `.header` com mapa por bloco, SHA-256 por build, compressão por frame) | `shared/pkg/storage/header/` | **Copiar o formato**; acrescentar **cifragem por bloco com a chave do dono** (ADR 0028 §1 exige snapshots cifrados) | novo módulo `zoen-snapfmt` |
| 5 | Fases de build em camadas com hash, cache por passo | `template/build/phases/` | **Copiar** (P1 "OCI template builder") | `zoen-sandboxd build` |
| 6 | Plano de controle × dados; o nó publica a própria rota; acordar ao receber tráfego | `orchestrator/pkg/routing`, client-proxy | **Copiar** com FDB (`/sbx/lease`, `/sbx/route`), guarda por `execution_id` | `zoen-sbx-scheduler` |
| 7 | Best-of-K com nota de CPU e hugepages, backoff com jitter | `api/internal/orchestrator/placement/` | **Copiar**, somando "template em cache" e "dono espalhado" do ADR §5 | `zoen-sbx-scheduler` |
| 8 | Preferir o nó de origem no resume; peers servem chunks antes do upload | `ARCHITECTURE.md` §Pause | **Copiar** | scheduler + `zoen-sandboxd` |
| 9 | Admissão de disco antes da pausa; degradar para snapshot só de disco; `e2fsck` só journal em jail | §Pause | **Copiar** | `zoen-sandboxd::suspend` |
| 10 | Fork de até 100 a partir de um checkpoint | §Fork | **Adaptar** (P4+): agentes em paralelo, "tente 3 caminhos" | `SandboxProvider::fork` |
| 11 | Upgrade do agente do convidado ao vivo, mesmo PID | `envd/.../upgrade.go` | **Adaptar** para `zoen-guestd` (handover de fds por vsock) | `zoen-guestd` |
| 12 | Versão do agente gravada no build; features gated por versão | `envd/pkg/version.go` | **Copiar** | manifest do template |
| 13 | envd sobre TCP/HTTP na VM, token por MMDS | `packages/envd` | **Evitar**: vsock + token por lease já é mais rápido (nosso exec p50 3,2 ms) e sem rede | `zoen-guestd` (como está) |
| 14 | Tap + netns + nftables + filtro por SNI | `sandbox/network`, `tcpfirewall` | **Evitar**: VM sem placa de rede, CONNECT pelo vsock, DNS feito pelo proxy | `zoen-egress` (como está) |
| 15 | Segredos como marcadores trocados na saída | §Secrets (ee) | **Valida** o que já fizemos (`zoen-secret://`) | `zoen-egress` |
| 16 | Firecracker sem jailer, como root | `fc/process.go` | **Evitar** | `zoen-sandboxd::jail` (como está) |
| 17 | `unshare -m` copiando tabela de montagens enorme sob concorrência | issue #3012 | **Evitar**: manter a tabela de montagens do host pequena; medir com 100 criações simultâneas no P4 | P4 |
| 18 | Hugepages de 2 MiB para memória da VM | nós de sandbox | **Adaptar/medir**: acelera o resume, mas o diff de pausa precisa continuar em 4 KiB | P4 |
| 19 | Qualquer porta vira URL pública | client-proxy | **Evitar**: nada da VM fica público; o único canal de saída para o dono é a tela selada | — |
| 20 | Desktop por Xvfb + x11vnc + noVNC; ações por `xdotool` por processo | `desktop:`, `desktop-js` | **Evitar** a forma; **aproveitar** a ideia de API de computer use (clicar, digitar, rolar, tela) | `zoen-guestd::desktop` |
| 21 | Stateful code interpreter (Jupyter quente) | `code-interpreter` | **Adaptar**: template T1 "python" com kernel no snapshot | template T1 |
| 22 | Eventos de ciclo de vida + webhooks + ClickHouse | §Events | **Adaptar** ao nosso `UsageRecorded` e ao painel de métricas (ADR 0043), só metadados | `zoen-agentd` |

---

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

## 6. Custo por hora: Zoen × E2B

Preços da E2B em 09/10/2026 ([e2b.dev/pricing](https://e2b.dev/pricing)): **US$0,000014 por
vCPU-segundo** (US$0,0504/h) e **US$0,0000045 por GiB-segundo** (US$0,0162/h); armazenamento
grátis; plano Pro US$150/mês; Enterprise mínimo US$3.000/mês; 1–8 vCPU e 1–8 GiB; padrão
2 vCPU + 4 GiB; sem GPU.

Nossos custos usam as premissas de [agent-sandbox.md §6](agent-sandbox.md) (Hetzner AX102,
US$302,10/mês, 128 GB, 16 núcleos/32 threads, 60% de uso, SMT com core scheduling a 80%).
São projeções por host, sem medir a densidade real nem o custo total de operação,
redundância, manutenção e infraestrutura de controle. Os preços públicos da E2B incluem
serviços que esse modelo próprio ainda precisa entregar.
Para o desktop: 3 GiB alocados (~2,25 GiB efetivos), 2 vCPU com 2:1 → ~40 VMs por host
paradas e ~25 transmitindo vídeo em tela cheia (o encoder gasta ~0,5 núcleo).

| Hora de… | Configuração | E2B | Zoen (Hetzner, 60% de uso) | Diferença |
|---|---|---|---|---|
| Sandbox de código | 1 vCPU + 1 GiB | US$0,067 | US$0,0069 | ~10× |
| Sandbox de código | 2 vCPU + 1 GiB | US$0,117 | ~US$0,0069 (memória manda) | ~17× |
| Navegador (sem tela) | 2 vCPU + 2 GiB | US$0,133 | US$0,0135 | ~10× |
| **Desktop com navegador**, sem ninguém olhando | 2 vCPU + 4 GiB (padrão E2B) / 3 GiB nosso | **US$0,166** | **≈US$0,017** | ~10× |
| **Desktop transmitindo** em tela cheia | idem + vídeo | US$0,166 + saída de rede | **≈US$0,028** + ~0,5 GB de tráfego (incluso no Hetzner) | ~6× |
| Pausado | — | grátis (armazenamento incluso) | NVMe local, depois objeto (~US$0,02/GB-mês) | — |

Contas: AX102 = US$302,10 / 730 h = US$0,414 por host-hora. Desktop parado: 0,414 / 40 /
0,6 ≈ US$0,017. Transmitindo: 0,414 / 25 / 0,6 ≈ US$0,028. Os números são estimativas e
serão trocados por medições no P3.5/P4 (as mesmas regras do ADR 0028 §9).

Em escala, o vídeo só existe enquanto alguém olha. Se 1 em cada 10 horas de desktop for
assistida em tela cheia, a média fica ≈US$0,018/h. A grande alavanca continua sendo a do
ADR 0028: **só ligar desktop quando a tarefa pede** (o manifest declara `desktop` acima de
`browser`).

Comparação de pacote: até 1.100 sandboxes simultâneas na E2B custam US$1.150/mês de plano,
fora o uso. Com 1.100 desktops ligados 24 h, só o uso seria ~US$133 mil/mês na E2B, contra
~US$13,7 mil em Hetzner na nossa conta (≈46 hosts AX102 a 60% de uso).

---

## 7. O que muda no ADR 0028 (proposta, não decidido)
1. Novo tier **T2d "desktop"** (ou um `display: true` no tier T2), com manifest
   `desktop { egress, apps, max_secs }`, escalado só quando o navegador sozinho não resolve.
2. Formato de snapshot do `zoen-sandboxd` = formato da E2B (cadeia de diffs + índice por
   bloco) **mais cifragem por bloco**; substitui "dm-thin" no §2.
3. `userfaultfd` + pré-carga gravada no build sobe de prioridade no P1.
4. Teste de carga do P4 inclui 100 criações simultâneas (a lição do issue #3012).
5. Nada de jailer a menos, nada de placa de rede, nada de porta pública: as três diferenças
   em que estamos mais fechados que a E2B continuam.

## Fontes
- E2B infra: [`docs/ARCHITECTURE.md`](https://github.com/e2b-dev/infra/blob/79fcdf59b093eddda444a76932a4d140850b71c6/docs/ARCHITECTURE.md),
  [`fc/process.go`](https://github.com/e2b-dev/infra/blob/79fcdf59b093eddda444a76932a4d140850b71c6/packages/orchestrator/pkg/sandbox/fc/process.go),
  [`fc/script_builder.go`](https://github.com/e2b-dev/infra/blob/79fcdf59b093eddda444a76932a4d140850b71c6/packages/orchestrator/pkg/sandbox/fc/script_builder.go),
  [`fc/config.go`](https://github.com/e2b-dev/infra/blob/79fcdf59b093eddda444a76932a4d140850b71c6/packages/orchestrator/pkg/sandbox/fc/config.go),
  [`sandbox/uffd/`](https://github.com/e2b-dev/infra/tree/79fcdf59b093eddda444a76932a4d140850b71c6/packages/orchestrator/pkg/sandbox/uffd),
  [`sandbox/network/firewall.go`](https://github.com/e2b-dev/infra/blob/79fcdf59b093eddda444a76932a4d140850b71c6/packages/orchestrator/pkg/sandbox/network/firewall.go),
  [`tcpfirewall/proxy.go`](https://github.com/e2b-dev/infra/blob/79fcdf59b093eddda444a76932a4d140850b71c6/packages/orchestrator/pkg/tcpfirewall/proxy.go),
  [`storage/header/header.go`](https://github.com/e2b-dev/infra/blob/79fcdf59b093eddda444a76932a4d140850b71c6/packages/shared/pkg/storage/header/header.go),
  [`envd/internal/services/process/upgrade.go`](https://github.com/e2b-dev/infra/blob/79fcdf59b093eddda444a76932a4d140850b71c6/packages/envd/internal/services/process/upgrade.go),
  [`api/internal/orchestrator/placement/`](https://github.com/e2b-dev/infra/tree/79fcdf59b093eddda444a76932a4d140850b71c6/packages/api/internal/orchestrator/placement),
  [`firecracker/fc-kernels/kernel_versions.txt`](https://github.com/e2b-dev/infra/blob/79fcdf59b093eddda444a76932a4d140850b71c6/firecracker/fc-kernels/kernel_versions.txt),
  [`firecracker/fc-versions/README.md`](https://github.com/e2b-dev/infra/blob/79fcdf59b093eddda444a76932a4d140850b71c6/firecracker/fc-versions/README.md),
  [`LICENSE`](https://github.com/e2b-dev/infra/blob/79fcdf59b093eddda444a76932a4d140850b71c6/LICENSE).
- E2B SDK: [`packages/desktop-js/src/sandbox.ts`](https://github.com/e2b-dev/E2B/blob/08945952ccefc1f74a9929e5ae4bc561c8b9aaf0/packages/desktop-js/src/sandbox.ts),
  [`packages/desktop-js/src/utils.ts`](https://github.com/e2b-dev/E2B/blob/08945952ccefc1f74a9929e5ae4bc561c8b9aaf0/packages/desktop-js/src/utils.ts),
  [`packages/js-sdk/src/sandbox/sandboxApi.ts`](https://github.com/e2b-dev/E2B/blob/08945952ccefc1f74a9929e5ae4bc561c8b9aaf0/packages/js-sdk/src/sandbox/sandboxApi.ts).
- Desktop: [`template/template.py`](https://github.com/e2b-dev/desktop/blob/1ff98a36306989d155ce5eceab2a2d38c9a8d6d2/template/template.py),
  [`README.md`](https://github.com/e2b-dev/desktop/blob/1ff98a36306989d155ce5eceab2a2d38c9a8d6d2/README.md).
- Code interpreter: [`template/template.py`](https://github.com/e2b-dev/code-interpreter/blob/1d3de4589c616c7a3405e0ea3ac5b4b22b264f55/template/template.py),
  [`template/start-up.sh`](https://github.com/e2b-dev/code-interpreter/blob/1d3de4589c616c7a3405e0ea3ac5b4b22b264f55/template/start-up.sh).
- Preços: [e2b.dev/pricing](https://e2b.dev/pricing) (lido em 09/10/2026).
- Tempos de terceiros: [infra#3012](https://github.com/e2b-dev/infra/issues/3012),
  [Podflare, abr/2026](https://podflare.ai/blog/cloud-sandbox-benchmark-e2b-daytona-podflare),
  [SemSwitch](https://semswitch.com/learn/anatomy-of-a-true-cold-start),
  [jacar.es](https://jacar.es/en/e2b-code-sandbox-for-agents/).
- Zoen: [ADR 0028](../adr/0028-agent-sandbox.md), [agent-sandbox.md](agent-sandbox.md),
  `crates/zoen-liveview/src/lib.rs`.
