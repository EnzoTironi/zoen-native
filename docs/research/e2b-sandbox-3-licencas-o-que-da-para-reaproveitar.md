[Research index](e2b-sandbox.md).

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
