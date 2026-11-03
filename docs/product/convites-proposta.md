# Convites no Zoen

[Playbook de referência e fontes](convites.md)

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

### 4.2 Prêmio: 5 convidados ativos = 1 mês do plano Business

- **Regra (decisão do Enzo, 9 out 2026, 03h02 BRT):** quem traz **5 usuários ativos** ganha **1
  mês do plano Business**. O prêmio se repete a cada 5 (**10 ativos = 2 meses**, **15 = 3
  meses**, e assim por diante), **sem teto** no lançamento: crescimento é a prioridade. O número
  total de convites continua limitado pela cota (10 mais os liberados por feedback).
- **O que é "usuário ativo" (proposta nossa, anti-fraude; é a proteção no lugar do teto):** um
  convidado que
  1. aceitou um **convite pessoal** daquela pessoa;
  2. criou conta com **telefone verificado** que nunca foi usado no Zoen;
  3. tem um **dispositivo atestado** (App Attest no iOS, Play Integrity no Android; na web, a conta
     só conta depois de ligar um aparelho);
  4. **mandou mensagens em pelo menos 2 dias diferentes dentro de 14 dias** desde o cadastro, com
     pelo menos uma das mensagens para alguém **que não seja quem convidou**.
- **Qual plano:** o **plano Business**. Preço e custo por assinante ainda estão **TBD** (não
  precificado). O premium opcional da [conta por usuário](../research/unit-economics.md) (seção
  16) continua existindo para o consumidor; o prêmio de indicação é o Business, não o plano mais
  barato.
- **Custo de aquisição:** **recalcular quando o Business for precificado** (custo do mês de
  Business ÷ 5 = custo por usuário ativo trazido). Até lá, qualquer número antigo baseado no
  premium (≈ US$ 0,67) está **obsoleto**. O plano grátis custa ≈ US$ 0,053 por usuário por mês.
- **Sem teto no lançamento (decisão do Enzo):** não há limite de meses grátis por trimestre ou por
  pessoa. A proteção contra abuso fica na definição de "ativo" (telefone novo, dispositivo
  atestado, mensagens em 2 ou mais dias). O teto **pode voltar depois**, com dados de custo e de
  fraude.
- **O prêmio é para a pessoa, não sai em dinheiro e não se transfere.** Quem já assina o Business
  ganha 1 mês grátis na próxima cobrança; os meses ganhos se somam.
- **Lojas:** um mês grátis de assinatura no iOS precisa passar pelas ferramentas da Apple (offer
  codes ou ofertas promocionais), ou o prêmio vale como crédito no plano comprado na web. É preciso
  checar com o jurídico e com a App Review antes de lançar.
- Quem conectou o próprio ChatGPT usa os créditos dele. Para essa pessoa o plano Business ainda
  vale pelos outros benefícios.
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
- **Custo do prêmio:** meses de Business dados ÷ usuários ativos trazidos (**TBD** até o Business ser precificado; a meta antiga de ≈ US$ 0,67 era do premium e está obsoleta).
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
- **Prêmio:** 1 mês do **plano Business** a cada 5 convidados ativos, **sem teto** no lançamento
  (crescimento primeiro; o teto pode voltar com dados).
- **"Convidado por":** não existe.
- **Convites de ouro:** só para o B2B.

Ainda em aberto:
1. Quantos convites extras cada rodada de feedback libera (proposta: +5 a cada 14 dias) e se o
   feedback é uma conversa com o agente ou uma pesquisa curta.
2. Confirmar a definição de usuário ativo (proposta: mensagens em 2 ou mais dias em 14; telefone
   verificado e dispositivo atestado).
3. **Precificar o plano Business** (preço e custo por assinante ainda TBD). Sem isso o custo por
   ativo trazido não fecha.
4. O que o convite de ouro dá ao convidado e ao dono do negócio (Bot Pro, créditos, destaque) e
   quantos cada plano B2B recebe.
5. Como dar um mês grátis no iOS respeitando as regras de compra da Apple (offer codes ou crédito
   no plano da web).
