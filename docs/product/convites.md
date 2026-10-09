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
> - quem traz **5 usuários ativos** ganha **1 mês do plano pago mais barato**;
> - **não existe "convidado por"** no perfil;
> - **convites de ouro são só para o B2B** (negócios, criadores e donos de comunidade).
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

   A recompensa é **1 mês do plano pago mais barato a cada 5 convidados ativos**. Os **convites de
   ouro** ficam só para negócios, criadores e donos de comunidade. Quem convidou quem **nunca
   aparece para outras pessoas**. Seções 3 e 4.

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
| Sem recompensa por indicação | **Mudar:** a cada **5 convidados ativos**, quem convidou ganha **1 mês do plano pago mais barato** (decisão do Enzo) | Sem portaria, a escassez de acesso não existe. O prêmio substitui o status |
| Status social em volta do convite (gente se gabando dos convites que sobraram) | **Não copiar no perfil:** não existe "convidado por". Só a própria pessoa vê as estatísticas dos convites dela (decisão do Enzo) | Privacidade: quem convidou quem é grafo social, e não se expõe |
| Termos: conta pessoal e intransferível | **Copiar.** Convites e prêmios ficam presos a quem convidou e a quem aceitou. Não valem para revenda | Evita o eBay |

## 4. Proposta para o Zoen

### 4.1 Tipos de convite

1. **Link de conversa ou Space (aberto, gerido pelos admins).** Todo chat e todo Space tem um link.
   Ele abre **direto na conversa na web**, sem landing page, e a pessoa lê e responde em segundos
   (ADR 0029). O pedido para instalar o app só aparece quando algo precisa do nativo, como
   notificações ou o agente local. **Proposta nossa:** esse link não gasta a cota de 10 e não conta
   para o prêmio, porque é entrada num grupo e não indicação pessoal. Tem limite de entradas por
   hora, controlado pelos admins.
2. **Convite pessoal (10 por pessoa; decisão do Enzo).** "Zoen, convida a Ana." O link abre uma DM
   com quem convidou, e o agente dele já dá as boas-vindas ("A Ana chegou!"). Todo usuário começa
   com **10**:
   - **Um convite só é gasto quando é aceito.** Link que expira ou é revogado devolve o convite
     (proposta nossa, como o "used up" do Instinct).
   - **Mais convites em troca de feedback.** Quando os 10 acabam, o Zoen oferece mais em troca de
     uma conversa curta com o agente do Zoen ou uma pesquisa de 3 a 5 perguntas dentro do app.
     **Proposta nossa:** +5 convites por rodada de feedback, no máximo uma rodada a cada 14 dias.
     O feedback precisa ter conteúdo (respostas em branco ou genéricas não liberam), e as respostas
     vão para a fila de pesquisa com usuários, sem o conteúdo de conversas.
   - Isso respeita a regra 3.2.2(x) da Apple: o feedback é dentro do app, nunca uma avaliação na
     App Store, e não trava nenhuma função do app. Só libera convites extras.
3. **Convite de ouro (só para o B2B; decisão do Enzo).** Para **negócios, criadores e donos de
   comunidade**, não para usuários comuns. Exemplos:
   - a confeiteira chama clientes para o bot da loja;
   - um criador como o Alexor Mods abre a comunidade dele;
   - o dono de um Space de concurso chama a turma.

   Com o convite de ouro, o convidado chega já dentro do Space ou do bot com algum benefício (por
   exemplo, créditos de modelos premium dentro daquela comunidade). O dono ganha ferramentas do lado
   empresa, como um período de **Bot Pro** ou destaque na loja (**proposta nossa**; valores a
   definir). A quantidade depende do plano da empresa ou do criador e é liberada pela equipe para
   os parceiros de lançamento.

### 4.2 Prêmio: 5 convidados ativos = 1 mês do plano pago mais barato

- **Regra (decisão do Enzo):** quem traz **5 usuários** ganha **1 mês do nosso plano pago mais
  barato**. O prêmio se repete a cada 5, mas o número total de convites é limitado pela cota (10
  mais os liberados por feedback).
- **O que é "usuário" (proposta nossa, para evitar fraude):** um convidado que
  1. aceitou um **convite pessoal** daquela pessoa;
  2. criou conta com **telefone verificado** que nunca foi usado no Zoen;
  3. tem um **dispositivo atestado** (App Attest no iOS, Play Integrity no Android; na web, a conta
     só conta depois de ligar um aparelho);
  4. **mandou mensagens em pelo menos 2 dias diferentes dentro de 14 dias** desde o cadastro, com
     pelo menos uma das mensagens para alguém **que não seja quem convidou**.
- **Qual plano:** hoje o único plano pago desenhado é o **premium opcional** da
  [conta por usuário](../research/unit-economics.md) (seção 16): preço sugerido de R$ 29,90 ou
  US$ 9,99, com custo de ≈ **US$ 3,34 por assinante por mês**. Se surgir um plano mais barato, o
  prêmio passa a ser ele.
- **Custo de aquisição (conta nossa):** ≈ US$ 3,34 ÷ 5 ≈ **US$ 0,67 por usuário ativo trazido**,
  pago uma vez. O plano grátis custa ≈ US$ 0,053 por usuário por mês. Isso fica bem abaixo de
  qualquer aquisição paga, mas **é custo real** e precisa de teto: no máximo 2 meses de prêmio por
  pessoa por trimestre (**hipótese**).
- **O prêmio é para a pessoa, não sai em dinheiro, não se transfere e não se acumula além de 3
  meses.** Quem já assina ganha 1 mês grátis na próxima cobrança.
- **Lojas:** um mês grátis de assinatura no iOS precisa passar pelas ferramentas da Apple (offer
  codes ou ofertas promocionais), ou o prêmio vale como crédito no plano comprado na web. É preciso
  checar com o jurídico e com a App Review antes de lançar.
- Quem conectou o próprio ChatGPT usa os créditos dele. Para essa pessoa o plano pago ainda vale
  pelos outros benefícios.
- **Privacidade (decisão do Enzo):** não existe "convidado por" no perfil nem em lugar nenhum
  visível para terceiros. A relação entre quem convidou e quem aceitou **fica privada**. Só a
  própria pessoa vê as estatísticas dela: convites restantes, aceitos, quantos já contam como
  ativos e quanto falta para o próximo mês grátis. O convidado não vê quem mais aquela pessoa
  convidou. O vínculo é guardado só para calcular o prêmio, com o mínimo de dados, e é apagado
  depois de 90 dias (**proposta nossa**).

### 4.3 O convidado chega "dentro de algo"

- O link sempre aponta para algo concreto: a DM com quem convidou, o grupo da turma, o Space do
  concurso, a comunidade do criador. Nunca para uma home vazia.
- Na web (ADR 0029), a pessoa entra como **dispositivo web** com chave própria. Ela pode ler e
  responder antes de criar a conta completa, e só confirma o telefone quando quiser ficar.
- O momento mágico (da [análise crítica](../research/analise-critica-zoen.md)) é ver conhecidos e
  receber uma resposta útil nos primeiros 10 minutos. O convite pessoal já entrega o primeiro
  conhecido.
- Igual ao Instinct em grupos, o agente de um Space ajuda quem ainda não tem conta (votação,
  divisão de conta, agenda), e isso é o que dá vontade de instalar.

### 4.4 Janelas de convite

- A cada recurso que fica melhor com amigos (um agente que marca encontros entre agentes, widgets de
  casal, salas de estudo), abrir **48 h com convites pessoais extras** (por exemplo +5), como os +15 do Trusted Person. Para parceiros B2B, a janela libera convites de ouro.
- No calendário brasileiro: começo de semestre (calouros), publicação de edital (concurseiros, na
  ideia do agente que avisa o edital em
  [apps-virais-estudantes-concurseiros.md](../research/apps-virais-estudantes-concurseiros.md)) e
  lançamentos de criadores (playbook Alexor Mods).
- Em **choques externos**, como queda do WhatsApp ou bloqueio de um concorrente, a importação de
  grupos e o link na web são o que transforma o pico em degrau
  ([curvas de crescimento](../research/curvas-crescimento-redes.md), Telegram em 2015 e 2021 e Bluesky
  em 2024). Janela de convite automática nesses dias.

### 4.5 Sugestões com base nos contatos, com privacidade

- **Sem subir a agenda inteira.** Por padrão a pessoa escolhe contatos pelo seletor do sistema (no
  iOS, o seletor fora do processo e o acesso limitado a contatos). As regras da Apple
  ([App Review Guidelines](https://developer.apple.com/app-store/review/guidelines/)) exigem:
  - 5.1.1(iii): preferir seletor ou share sheet a pedir acesso total aos Contatos;
  - 5.1.2(iv): não usar os Contatos para montar um banco de contatos próprio;
  - 5.1.2(v): só contatar alguém da agenda por iniciativa explícita do usuário, uma pessoa por vez,
    **sem "selecionar todos"** e sem todos pré-marcados, mostrando antes como a mensagem vai
    aparecer para quem recebe;
  - 3.2.2(x): não exigir avaliação ou outra ação na loja para liberar funções. Dar incentivo por
    ação dentro do app é permitido;
  - 5.6.3: não manipular indicações.
- **LGPD:** o telefone de quem **não** usa o Zoen é dado pessoal de um terceiro que não consentiu.
  Por isso:
  - o servidor não guarda números de não usuários;
  - o link de convite é um token de capacidade (`new_secret_id`, 128 bits, ADR 0017) que não
    identifica o destinatário;
  - descobrir "quem já está no Zoen" usa uma consulta privada que não deixa a agenda no servidor;
  - não há lembrete automático para quem não respondeu.
- **Sugestões úteis, não insistentes:** "3 pessoas da sua turma já estão no Space de Cálculo I" é
  aceitável quando vem de um Space em que a pessoa já está. "Convide 20 amigos" na tela inicial
  não é.

### 4.6 Limites antispam e antifraude

- **A cota é o primeiro limite:** 10 convites pessoais, e mais só com feedback.
- **Limites por conta e por dispositivo** com os mesmos baldes GCRA do relay (ADR 0020). Proposta
  inicial (hipótese): até 10 links pessoais gerados por dia e no máximo 1 convite por destinatário
  por remetente a cada 30 dias.
- **Links pessoais:** uso único, validade de 14 dias, revogáveis e presos ao primeiro telefone que
  os aceitar. Links de Space ficam a cargo dos admins.
- **Prêmio só com convidado ativo**, pela definição da 4.2: telefone novo, dispositivo atestado e
  mensagens em 2 ou mais dias em 14, com pelo menos uma para alguém além de quem convidou. Contas de
  fazenda que só falam entre si são detectadas pelo padrão e não contam.
- **Sem mercado paralelo:** entrar é grátis, e o prêmio fica preso a quem convidou. A UI avisa que
  ninguém vende convite e que o código de verificação nunca deve ser repassado.
- **Menores (ECA Digital):** sem prêmio por indicação para menores de 18 e sem sugestões baseadas em
  contatos para eles. Space com menores segue as regras de proteção da visão (seção de comunidades).

### 4.7 Medir o K

- **Funil por coorte semanal**, como Schultz descreve na análise crítica: convites enviados por
  usuário ativo (i), links abertos, contas ou dispositivos web criados, convidados ativos na
  semana 1 e na semana 4, e convidados que convidam de novo.
- **K = i × c**, em que c é a conversão até ficar ativo. Medir separado por tipo (conversa, pessoal,
  ouro B2B) e por nicho (turma, concurso, gamer, pequeno negócio). Medir também o **tempo de ciclo**
  (dias entre entrar e convidar alguém que fica ativo): K com ciclo curto cresce mais rápido que K
  alto com ciclo longo.
- **Métrica no estilo do Instinct:** a fração da base ativa que teve um convite pessoal aceito hoje. O Instinct relata ~10% ao dia. É um termômetro de boca a boca que dá para comparar.
- **Retenção antes de K:** a meta da análise crítica continua (≥40% ativos na semana 4). K sem
  retenção é o caso Gas e Clubhouse ([curvas](../research/curvas-crescimento-redes.md)).
- **Funil da cota:** quantas pessoas esgotam os 10 convites, quantas dão feedback para ganhar mais e quanto esse grupo convida depois.
- **Custo do prêmio:** meses de plano dados ÷ usuários ativos trazidos (meta ≈ US$ 0,67 ou menos).
- **Privacidade:** as métricas usam eventos pseudônimos (o mesmo padrão da telemetria, ADR 0021) e
  nunca o conteúdo de conversas.

### 4.8 Como fica com o "aberto para todos"

A [pesquisa de curvas](../research/curvas-crescimento-redes.md) recomendava nicho primeiro e abertura
geral no fim. Com a decisão de abrir desde o início, o nicho deixa de ser uma portaria e vira
**onde a gente coloca energia**: janelas de convite, embaixadores e Spaces prontos por turma e por
concurso. O produto fica aberto, e a densidade continua sendo criada à mão, por rede atômica. A cota de
10, as janelas e os convites de ouro do B2B são o jeito de ter a escassez do Instinct sem fechar a porta.

## 5. Perguntas em aberto

Já decididas pelo Enzo em 9 out 2026:
- **Cota:** 10 convites por pessoa, e mais em troca de feedback.
- **Prêmio:** 1 mês do plano pago mais barato a cada 5 convidados ativos.
- **"Convidado por":** não existe.
- **Convites de ouro:** só para o B2B.

Ainda em aberto:
1. Quantos convites extras cada rodada de feedback libera (proposta: +5 a cada 14 dias) e se o
   feedback é uma conversa com o agente ou uma pesquisa curta.
2. Confirmar a definição de usuário ativo (proposta: mensagens em 2 ou mais dias em 14) e o teto de
   prêmio por trimestre.
3. Qual é o "plano pago mais barato" no lançamento. Hoje existe só o premium sugerido (R$ 29,90).
   Um plano menor mudaria o custo do prêmio.
4. O que o convite de ouro dá ao convidado e ao dono do negócio (Bot Pro, créditos, destaque) e
   quantos cada plano B2B recebe.
5. Como dar um mês grátis no iOS respeitando as regras de compra da Apple (offer codes ou crédito
   no plano da web).

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
