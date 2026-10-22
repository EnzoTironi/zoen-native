[Research index](e2b-sandbox.md).

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
