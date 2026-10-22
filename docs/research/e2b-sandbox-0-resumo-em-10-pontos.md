[Research index](e2b-sandbox.md).

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
