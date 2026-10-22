[Research index](e2b-sandbox.md).

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
