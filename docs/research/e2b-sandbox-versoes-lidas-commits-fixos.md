[Research index](e2b-sandbox.md).

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
