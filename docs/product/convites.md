# Convites: o playbook do Instinct e como o Zoen vai usar

> Decisão do Enzo (9 out 2026): o Zoen **abre para todo mundo desde o primeiro dia**, mas copia
> o **playbook de convites do Instinct**. Este documento registra o que esse playbook é (com
> fontes), o que dá para copiar num produto aberto e o que precisa mudar.
>
> Regras: números só com fonte; o que não achei está marcado **não encontrado**; suposições
> nossas estão marcadas como **hipótese**.
>
> **Atualizado em 9 out 2026, 02h30 (BRT), depois da revisão do Enzo:**
> - todo usuário começa com **10 convites**, e mais convites saem em troca de feedback;
> - quem traz **5 usuários ativos** ganha **1 mês do plano Business**;
> - **não existe "convidado por"** no perfil;
> - **convites de ouro são só para o B2B** (negócios, criadores e donos de comunidade).
>
> **Atualizado em 9 out 2026, 03h02 (BRT):** o prêmio é **1 mês do plano Business a cada 5
> ativos**, **sem teto** no lançamento (crescimento é a prioridade). O preço do Business ainda
> está **TBD**; o custo por ativo trazido será recalculado quando o plano for precificado. A
> proteção contra abuso continua na definição de "ativo". O teto pode voltar depois, com dados.
>
> As seções 0, 3 e 4 seguem essas decisões.

## 0. Resumo

1. **Qual Instinct.** O Instinct citado é quase certamente o **Instinct (instinct.com)**, o
   assistente pessoal de IA da Spear Street Technology, de Noah Shinn, que a gente usa por
   mensagem ou ligação. Confiança: **alta**. A pasta `/workspace/OpenInstinct` é um fork privado do
   **OpenInstinct da Merit Systems**, um clone open source sem relação com a empresa e **sem nenhum
   mecanismo de convite de consumidor**. Ver a seção 1.
2. **O playbook em uma frase.** Acesso só por convite, com **5 convites por pessoa**, entregues
   como um **link privado que a pessoa pede ao próprio assistente**. Fora isso há uma lista de
   espera por telefone, liberada conforme a capacidade. O resultado: **todo novo usuário chega
   pelas mãos de um amigo ou parente próximo**.
3. **Números.** Começou com cerca de 200 amigos e familiares (205 no dia seguinte, depois 210). Ao
   chegar a alguns milhares, cresceu 1–2%, depois 6–9% e então **10–11% ao dia**, com **US$ 0 em
   marketing**. Convites foram revendidos no eBay por US$ 93 a US$ 300. Passou de 100 mil
   usuários segundo a The Information, citada pela PYMNTS. Seção 2.
4. **Para o Zoen (decidido pelo Enzo).** O produto é aberto e não tem portaria. Do Instinct copiamos:
   - **a cota de convites:** 10 por pessoa, com mais convites liberados em troca de feedback;
   - **o convite como gesto pessoal**, pedido ao próprio agente;
   - **o convidado chegando dentro de uma conversa**;
   - **as janelas de convite nos lançamentos.**

   A recompensa é **1 mês do plano Business a cada 5 convidados ativos**, **sem teto** no
   lançamento. Os **convites de ouro** ficam só para negócios, criadores e donos de comunidade.
   Quem convidou quem **nunca aparece para outras pessoas**. Seções 3 e 4.

## 1. Desambiguação: qual "Instinct"

| Nome | O que é | É o do playbook? |
|---|---|---|
| **Instinct** (instinct.com, Spear Street Technology, Inc., CEO Noah Shinn) | Assistente pessoal de IA por SMS, iMessage, WhatsApp, ligação e e-mail; age por você em sites e apps. Beta privado desde fev/2026; US$ 350 mi levantados até ago/2026 e cerca de US$ 1 bi a US$ 10 bi de avaliação em set/2026 | **Sim, confiança alta.** É o único com um programa de convites famoso e documentado, e é um assistente de IA em mensageiro, como o Zoen |
| **OpenInstinct** (Merit Systems, `github.com/Merit-Systems/OpenInstinct`, openinstinct.sh) | Clone open source (MIT), auto-hospedado no Vercel, assistente de iMessage com cofre de senhas. A pasta `/workspace/OpenInstinct` é um fork privado dele (o "Companion", com Telegram, WhatsApp e web) | Não. Cada usuário é adicionado à mão numa lista de contatos permitidos do Linq. Não há convites nem lista de espera |
| **Open Instinct** (projeto de Maria Gorskikh) | Projeto separado com nome parecido, segundo o [agentcomparison.net](https://agentcomparison.net/agents/openinstinct/) | Não |
| **Instinct** (app de namoro) e **Instinct Science** (saúde animal) | Empresas sem relação, segundo a [Layer3Labs](https://www.layer3labs.io/guides/how-to-get-instinct-ai) | Não |
| **Garmin Instinct** | Relógio esportivo | Não |

### O que existe na pasta local `/workspace/OpenInstinct`

Li a pasta sem modificar nada. O que achei:

- `db/services/organization-invites.ts`: convites **corporativos** por e-mail, com papel (RBAC),
  domínio permitido (Google Workspace), validade e recibo de auditoria. Serve para equipes, não para
  crescimento.
- `docs/consumer-first-run.md`: cadastro pela web (escolher Telegram ou WhatsApp, confirmar no
  mensageiro, criar a conta e o workspace num só fluxo). Não há convite, lista de espera nem
  indicação.
- `docs/native-onboarding.md` e `docs/conversation-experience.md`: "convite" ali é convite de
  calendário, não de produto.
- `docs/consumer-marketing.md`: páginas `/welcome` e `/pricing` com estrutura inspirada no Poke e
  no Town. Não fala de convites.
- O upstream (README do Merit-Systems/OpenInstinct) também não tem programa de convites: o dono
  do deploy cadastra cada telefone na lista de contatos do Linq.

**Conclusão:** o playbook de convites **não está no código local**. Ele vem do produto Instinct
público, descrito abaixo.

## 2. O playbook do Instinct, como encontrado

### 2.1 Mecânica

| Elemento | Como funciona | Fonte |
|---|---|---|
| Acesso | Só por convite ou pela lista de espera. Beta privado em fev/2026 e beta por convite a partir de ago/2026 | [Fortune, 30/set/2026](https://fortune.com/2026/09/30/noah-shinn-instinct-ai-assistant-meta-muse-alexandr-wang-tech-series-c-ai-agent-mark-zuckerberg/) |
| **Cota** | **5 convites por pessoa** | Fortune; Noah Shinn no [Invest Like the Best](https://podcasts.apple.com/us/podcast/noah-shinn-building-instinct-the-personal-agent/id1154105909?i=1000792019848) |
| **Formato** | Não existe código. É um **link privado** que você **pede ao seu Instinct** por mensagem ("Ask Instinct at any time for your private invite link"). O convidado abre o link, digita o telefone e o código de 6 dígitos e já está dentro | [Carly, 6/out/2026](https://www.usecarly.com/blog/instinct-invite-code/) |
| Estados do link | "This invite link has been used up"; "Referral limit reached" (os 5 acabaram); expirado ou revogado, quando oferece a lista de espera; "Instinct is at capacity", quando até um link válido cai na lista | Carly |
| **Lista de espera** | "Text Instinct to get started": telefone e código por SMS ou WhatsApp, depois a tela "You're on the waitlist. We're letting people in as fast as we can." Sem prazo nem tamanho de fila publicados. Avisa por SMS quando a vaga abre | Carly; [Layer3Labs](https://www.layer3labs.io/guides/how-to-get-instinct-ai) |
| Furar a fila | Só com um convite de membro. Não há "pule a fila indicando amigos" para quem está na lista | Carly e Layer3Labs (**não encontrei** nenhum mecanismo de pular a fila) |
| **Motivo declarado** | Controlar o crescimento enquanto a capacidade de computação aumenta ("actively bringing up more compute"), e não sinalizar exclusividade. Shinn: não quer acordar com 10× usuários e 80% sem conseguir falar com o produto | Fortune; [resumo do podcast (PJFP)](https://pjfp.com/noah-shinn-instinct-personal-ai-assistant-10-percent-a-day/); [BidClub](https://bidclub.ai/e/noah-shinn-building-instinct-personal-agent) |
| **Princípio** | "Our exploding invite-only program that ensures that every new user is onboarded by a close friend or family member" | [Noah Shinn no X, 28/set/2026](https://x.com/noahrshinn/status/2104593307087314968) |
| **Janela de convites no lançamento** | Ao abrir o Trusted Person network para todos: "For the next two days, you'll receive an additional **15 invites**", para trazer as pessoas mais importantes para a sua rede | [Noah Shinn no X, 14/set/2026](https://x.com/noahrshinn/status/2099358203121393851) |
| **Rede que puxa convites** | O Trusted Person network deixa o seu Instinct falar com o Instinct de outra pessoa para marcar planos. Só funciona entre pessoas de confiança, então dá motivo concreto para convidar | [X, 9/set/2026](https://x.com/noahrshinn/status/2097794967574028448) |
| **Entrada sem conta** | Instinct em grupos: "Your friends don't even need Instinct to join in." O Instinct do grupo é separado do pessoal, e o pessoal pede permissão antes de confiar no grupo | [X, 5/out/2026](https://x.com/noahrshinn/status/2107161132192690558) |
| Lançamentos em estágios | Primeiro Shinn, depois o time, depois o grupo de acesso antecipado e por fim todos. "Peça ao seu Instinct para te colocar na lista" | PJFP; X, 5/out/2026 |
| Indicação paga | **Não encontrado.** Não achei recompensa em dinheiro, crédito ou benefício para quem convida. O "prêmio" é a própria escassez | — |
| Cartão para compartilhar | **Não encontrado** um cartão próprio de convite. Os "Files" (páginas interativas, públicas ou privadas) são compartilháveis e levam o produto a quem não usa | [X, 18/set/2026](https://x.com/noahrshinn/status/2101080443667767385) |
| Termos | Uma conta por pessoa; "You may not allow anyone else to use your Account"; 18 anos ou mais; confidencialidade no beta fechado | Carly; Layer3Labs |

### 2.2 Números

| Métrica | Valor | Fonte |
|---|---|---|
| Base inicial | ~200 pessoas, amigos próximos e família | [CEO Insider (fala de Shinn)](https://www.ceoinsider.io/answer/what-does-10-percent-day-over-day-growth-actually-look-like-and-how-did-it-start); BidClub |
| Primeiros dias | 205 no dia seguinte, depois 210: "very slow and linear at first" | CEO Insider |
| Inflexão | Com "a couple thousand users", gente começou a postar casos de uso: de 1–2% para 6–9% ao dia | CEO Insider; PJFP |
| Crescimento recente | **10–11% ao dia, US$ 0 de marketing** ("roughly 10% A DAY") | CEO Insider; [Patrick O'Shaughnessy no X, 28/set/2026](https://x.com/patrick_oshag/status/2104542892073095398) |
| Leitura do próprio Shinn | "Every single day about ten percent of the audience is making a deliberate decision to give up one of their five valuable invites" | CEO Insider |
| Revenda | "Around 300 dollars" no eBay (Shinn e O'Shaughnessy); a Cybernews achou 2 anúncios de US$ 93 a mais de US$ 200 em 30/set | CEO Insider; Carly |
| Efeito social | Pessoas mandando e-mail "envergonhadas" para pedir convite; outras se gabando de ter "três sobrando" | Transcrição em [hraness.com](https://hraness.com/reading/inside-the-personal-ai-assistant-growing-10-a-day-instinct-founder) |
| Usuários totais | >100 mil (The Information, via PYMNTS); a empresa **não divulgou** um número oficial | Layer3Labs; [MLQ](https://mlq.ai/news/instinct-is-still-invite-only-as-its-ai-assistant-takes-broad-access-to-users-data/) |
| Volume transacionado | ~US$ 1 bi/ano, 50% em viagens | Fortune; PJFP |
| Retenção ligada à confiança | 40% compartilham um cartão em 3 semanas; quem compartilha um dado sensível retém ~80% | PJFP |
| Países | O cadastro aceitava telefones de 83 países em 6/out/2026 (o começo era só EUA) | Carly |
| Taxa de aceitação, convites usados por pessoa, K | **Não encontrado** | — |

Leitura (nossa, não da fonte): crescer 10% ao dia com 5 convites por pessoa quer dizer que, em
média, cerca de 1 em cada 10 usuários "gasta" um convite que vira conta a cada dia. A curva teve
três fases: linear enquanto só a família usava, aceleração quando o conteúdo saiu para fora (posts
de casos de uso) e composta depois disso. A cota não criou a demanda. Ela **concentrou** a demanda
em quem já confiava no convidado.

### 2.3 Problemas que o playbook trouxe

- **Mercado paralelo de convites** (eBay) e risco de golpe: vender o código de 6 dígitos entrega a
  conta (Layer3Labs).
- **Frustração com capacidade:** um link válido ainda pode cair em "at capacity".
- **Confiança e privacidade** viraram assunto público durante o beta (e-mails indexados e retidos,
  envio sem confirmação). Os termos foram revistos em 26/ago/2026 (MLQ, Layer3Labs).
- **Quedas** em 22–23/set sem aviso de erro (TheStreet, via Layer3Labs).

## 3. O que copiar, o que não copiar

| Do Instinct | No Zoen aberto | Por quê |
|---|---|---|
| Portaria (sem convite, não entra) | **Não copiar.** Qualquer pessoa entra direto pelo app ou pela web | Decisão do Enzo. Portaria também deu revenda e frustração |
| 5 convites por pessoa | **Copiar a cota, com mais folga: 10 convites por pessoa.** Para ganhar mais, a pessoa dá feedback (decisão do Enzo) | A cota faz cada convite ser uma escolha ("escolhi você"). O pedido de feedback transforma quem mais convida em fonte de aprendizado |
| Pedir o link ao próprio assistente | **Copiar.** "Zoen, me dá um convite para a Ana" gera o link na conversa com o agente | É natural num app com agente, e o pedido já diz para quem é o convite |
| "Todo novo usuário chega por um amigo ou parente" | **Copiar como padrão.** O link abre a conversa com quem convidou, e o agente de quem convidou recebe a pessoa | O primeiro momento é com alguém conhecido, não uma tela vazia |
| Grupo funciona para quem não tem conta | **Copiar.** O link de um Space ou grupo abre na web e a pessoa participa na hora (ADR 0029) | É o mesmo "your friends don't even need Instinct" |
| +15 convites por 2 dias no lançamento de um recurso social | **Copiar como "janela de convites"** a cada recurso que fica melhor com amigos | Concentra a viralidade nos picos de novidade |
| Lançamentos em estágios ("peça para entrar no acesso antecipado") | **Copiar** para recursos novos e caros, como modelos premium e o navegador do agente | Dá a escassez onde existe custo real de computação |
| Sem recompensa por indicação | **Mudar:** a cada **5 convidados ativos**, quem convidou ganha **1 mês do plano Business**, sem teto no lançamento (decisão do Enzo) | Sem portaria, a escassez de acesso não existe. O prêmio substitui o status; crescimento sem teto no começo |
| Status social em volta do convite (gente se gabando dos convites que sobraram) | **Não copiar no perfil:** não existe "convidado por". Só a própria pessoa vê as estatísticas dos convites dela (decisão do Enzo) | Privacidade: quem convidou quem é grafo social, e não se expõe |
| Termos: conta pessoal e intransferível | **Copiar.** Convites e prêmios ficam presos a quem convidou e a quem aceitou. Não valem para revenda | Evita o eBay |

[Proposta para o Zoen e perguntas em aberto](convites-proposta.md)

## Fontes

- Instinct: https://instinct.com/
- Fortune, 30/set/2026: https://fortune.com/2026/09/30/noah-shinn-instinct-ai-assistant-meta-muse-alexandr-wang-tech-series-c-ai-agent-mark-zuckerberg/
- Carly, "Instinct Invite Code and Waitlist", 6/out/2026: https://www.usecarly.com/blog/instinct-invite-code/
- Layer3Labs, "How to Get Instinct AI", 3/out/2026: https://www.layer3labs.io/guides/how-to-get-instinct-ai
- CEO Insider (fala de Noah Shinn): https://www.ceoinsider.io/answer/what-does-10-percent-day-over-day-growth-actually-look-like-and-how-did-it-start
- PJFP, resumo do Invest Like the Best: https://pjfp.com/noah-shinn-instinct-personal-ai-assistant-10-percent-a-day/
- BidClub: https://bidclub.ai/e/noah-shinn-building-instinct-personal-agent
- Transcrição (hraness): https://hraness.com/reading/inside-the-personal-ai-assistant-growing-10-a-day-instinct-founder
- Podcast: https://podcasts.apple.com/us/podcast/noah-shinn-building-instinct-the-personal-agent/id1154105909?i=1000792019848
- Posts de Noah Shinn no X: https://x.com/noahrshinn/status/2104593307087314968, https://x.com/noahrshinn/status/2099358203121393851, https://x.com/noahrshinn/status/2097794967574028448, https://x.com/noahrshinn/status/2107161132192690558, https://x.com/noahrshinn/status/2101080443667767385
- Patrick O'Shaughnessy no X: https://x.com/patrick_oshag/status/2104542892073095398
- MLQ: https://mlq.ai/news/instinct-is-still-invite-only-as-its-ai-assistant-takes-broad-access-to-users-data/
- OpenInstinct (Merit Systems): https://github.com/Merit-Systems/OpenInstinct, https://openinstinct.sh/
- agentcomparison.net: https://agentcomparison.net/agents/openinstinct/
- Apple App Review Guidelines: https://developer.apple.com/app-store/review/guidelines/
- Internas: ADR 0017, 0020, 0021, 0029; docs/research/curvas-crescimento-redes.md, apps-virais-estudantes-concurseiros.md, unit-economics.md, analise-critica-zoen.md
