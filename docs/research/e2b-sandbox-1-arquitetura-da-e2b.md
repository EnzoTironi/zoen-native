[Research index](e2b-sandbox.md).

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
