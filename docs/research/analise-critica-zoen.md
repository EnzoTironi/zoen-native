# Análise crítica do Zoen pelas aulas de "How to Start a Startup" (CS183B, Stanford, 2014)

> Escrita em 9/out/2026 a pedido do Enzo, na chamada de voz. É um julgamento honesto, não um pitch.
> As referências às aulas vêm das transcrições públicas do curso (links no fim). Citações entre
> aspas estão em inglês e foram copiadas das transcrições; o resto é paráfrase.
> Contexto do Zoen: `docs/product/vision.md`, `docs/adr/`, `docs/decisions-log.md`, `docs/cost-model.md`
> e a pesquisa de custo por usuário (`docs/research/zoen-unit-economics.md`, PR #18).

## 1. Veredito

**A engenharia está no caminho certo. A empresa ainda não começou.** Em dois dias o Zoen ganhou
um núcleo em Rust com criptografia de ponta a ponta, sincronização, relay testado em carga, sandbox
de código e navegador em microVM. Isso é raro, e prova capacidade de execução. Mas não há nenhum
usuário, nenhuma conversa com usuário registrada e pelo menos dez produtos na visão: mensageiro,
agente para pequeno negócio, loja de apps de IA, comunidades, widgets, cortes de casal, anúncio
para bots, web, Android e trabalho com ontologia. Pelas aulas, isso não é um detalhe; é a causa de
morte mais comum. **A única coisa a mudar: parar de construir para 1 bilhão e escolher uma rede
pequena (uma comunidade de 30 a 150 pessoas que o Enzo alcança pessoalmente), um momento mágico e
uma métrica de retenção, e não fazer mais nada até essa rede funcionar sozinha.**

## 2. Pontos fortes

**Missão real.** Altman (Aula 1) diz que as melhores empresas são orientadas por missão e que "it's
easier to found a hard startup than an easy startup": um problema difícil e importante atrai ajuda
e gente boa. "Todo pequeno negócio com um agente de graça" e "privacidade de verdade num mensageiro
brasileiro" são missões que dá para explicar em uma frase e que empolgam.

**Capacidade de execução e intensidade.** Altman (Aula 2) divide execução em foco e intensidade.
Intensidade o Zoen tem de sobra: 13 PRs com mérito técnico real entraram na `main` em cerca de uma hora e
meia, na virada de 8 para 9/out. Benchmarks medidos (cerca de 5 ms por mensagem, 2 mil mensagens/s, 10 mil
conexões; Firecracker voltando de snapshot em cerca de 18 ms) mostram que, quando a direção estiver
certa, o Enzo e os agentes andam mais rápido do que quase qualquer time pequeno. Esse é o ativo mais
valioso da empresa.

**Instinto certo de nicho.** Thiel (Aula 5) diz que o grande erro é atacar um mercado gigante no
primeiro dia, e que o certo é dominar um mercado pequeno e crescer em círculos: o PayPal começou com
cerca de 20 mil "power sellers" do eBay; o Facebook foi de 0 a 60% dos 10 mil alunos de Harvard em
dez dias. A visão já fala de Mensa, gamers, universitários e concurseiros. A direção está certa;
falta escolher só um.

**Um "por que agora" plausível.** Altman (Aula 1) cita a pergunta da Sequoia: por que agora, e não
dois anos antes ou depois? O Zoen tem respostas boas: o Discord está com vídeo, Go Live e
compartilhamento de tela suspensos no Brasil pela ANPD desde agosto de 2026; modelos baratos custam
centavos (a pesquisa de custo chegou a cerca de US$ 0,05 por usuário ativo por mês no cenário
médio); o padrão MCP Apps existe desde jan/2026; e a Meta passou a cobrar por mensagem de
provedores de IA no WhatsApp brasileiro desde 11/mar/2026.

**Base técnica que pode virar vantagem duradoura.** Thiel (Aula 5) lista as fontes de monopólio:
tecnologia própria, efeito de rede, economia de escala e marca. Criptografia de ponta a ponta por
padrão com agentes que só leem onde são membros declarados é uma combinação que nem WhatsApp nem
Telegram oferecem hoje. O custo de infraestrutura projetado (`cost-model.md`: cerca de US$ 0,01 por
usuário diário por mês em 1 milhão de DAU) tem cara de custo fixo alto e marginal baixo, o perfil
que Thiel associa a monopólio.

**Cuidado com privacidade e com o ECA Digital desde o começo.** A visão já registra que a ANPD
citou a criptografia do Discord como "menos protetiva" para menores. Ver o risco cedo é melhor do
que descobrir com uma suspensão.

## 3. Riscos e fraquezas mais sérios (em ordem)

**1. Falta de foco: coisas demais ao mesmo tempo.** É o risco número um, e os outros derivam dele.
O Startup Playbook de Altman é direto: "A very, very common cause of startup death is doing too many
of the wrong things", e "don't let your company start doing the next thing until you've dominated
the first thing". Na Aula 2, ele diz que boa execução é dizer não 97 vezes em 100 e que você não
ganha crédito por tentar, só por fazer algo que o mercado quer. Hoje o Zoen trabalha ao mesmo tempo
em mensageiro, agentes, loja, MCP Apps, comunidades, widgets, cortes, anúncio para bots, web,
Kotlin, sandbox e navegador. Agentes de IA tornam cada ideia barata de começar, e isso piora o
problema: o custo de "mais uma coisa" parece zero, mas a atenção do fundador não é.

**2. Nenhum usuário e nenhuma conversa com usuário registrada.** Altman (Aula 1): o trabalho é
fazer algo que usuários amem, e é melhor ter poucos usuários que amam do que muitos que só gostam.
O Playbook pede um "motor de melhoria de produto": falar com usuários, vê-los usar, consertar e
repetir. O repositório tem 30 ADRs, um modelo de escala para 1 bilhão e nenhuma entrevista. PG (Aula 3)
chama isso de "playing house": passar pelos rituais de uma startup sem o que importa. Toda decisão
de produto até aqui (cartões de aprovação, jiggle, cabeçalho expansível) foi tomada pelo gosto do
fundador, não pelo comportamento de alguém.

**3. Partida a frio de mensageiro contra um WhatsApp que está em praticamente todo celular.**
Thiel (Aula 5) diz que efeito de rede é valioso mas difícil de começar, e pergunta: por que isso é
valioso para a primeira pessoa? Hoffman (Aula 13) conta que diziam que o LinkedIn não teria valor
para o primeiro usuário até ter centenas de milhares. Mensageiro é o pior caso: a pessoa só troca se
as pessoas dela trocarem junto. "WhatsApp 2" descreve exatamente a briga direta que Thiel manda
evitar: você vira o quarto ou quinto mensageiro, não o único em um mercado pequeno. O Zoen precisa
ser valioso dentro de um grupo fechado (uma comunidade, um negócio e seus clientes) antes de ser
valioso como mensageiro geral.

**4. Escala antes da hora.** FoundationDB desde o primeiro dia, seções "a 1 bilhão de usuários" em
todos os ADRs, Firecracker, navegador em microVM, cadeia de egress: tudo isso antes do primeiro
usuário. O Playbook: pensar em "como fazer isso em escala massiva" é uma armadilha; morrem mais
startups debatendo isso do que por não terem pensado; regra prática: pensar só em 10x da escala
atual. Walker Williams (Aula 8): otimizar velocidade em vez de escalabilidade e código limpo, e só
se preocupar com a próxima ordem de grandeza (com 10 usuários, pensar em 100). Andreessen (Aula 9)
descreve a startup como uma cebola de riscos (equipe, produto, técnico, lançamento, aceitação do
mercado, receita, crescimento viral) que você descasca em ordem. O Zoen descascou com perfeição a
camada técnica, que era a menor, e ainda não tocou nas camadas que matam: aceitação de mercado e
crescimento.

**5. Monetização frágil e contraditória.** A pesquisa de custo concluiu: só anúncio não fecha o
cenário médio. O banner em comunidade rende cerca de US$ 0,01 a 0,02 por usuário por mês, contra
um custo de cerca de US$ 0,05. Quem fecha a conta é o lado empresa: bot pago, mensagens de empresa
e comissão. Ao mesmo tempo, a visão proíbe anúncio para humanos e propõe banner patrocinado em
comunidade, que é anúncio para humanos. E promete agente grátis para pequeno negócio, que é
justamente quem pagaria. O Playbook aceita unidade econômica ruim no começo, desde que haja uma
razão concreta para ela melhorar depois. Williams (Aula 8) recomenda não dar o produto de graça,
com exceções. Emmett Shear (Aula 16) diz que vender é o teste que resolve tudo: se a pessoa dá o
cartão, ela quer de verdade. Hoje não há esse teste.

**6. Dependência da Meta como canal.** Se os agentes dos pequenos negócios atendem pelo WhatsApp, a
Meta manda: desde 15/jan/2026 os termos do WhatsApp Business restringem assistentes de IA de uso
geral, e no Brasil a Meta cobra por mensagem de "AI Providers" desde 11/mar/2026
([Meta](https://developers.facebook.com/documentation/business-messaging/whatsapp/pricing/ai-providers)).
A Meta também tem o Meta AI dentro do WhatsApp. Se o agente vive só dentro do Zoen, o cliente da
confeiteira precisa instalar um mensageiro novo, e voltamos ao risco 3. Hoffman (Aula 13) diz que
distribuição é mais fundamental do que o próprio produto; hoje a distribuição do lado B2B pertence
a um concorrente.

**7. Regulação: ECA Digital e ANPD.** A medida contra o Discord mostra que a ANPD age rápido e que
criptografia de ponta a ponta foi lida como agravante para menores. Gamers e concurseiros incluem
menores. Sem verificação de idade e regras para menores, lançar no nicho gamer com vídeo e
compartilhamento de tela é repetir o caso Discord. "Rede social brasileira soberana" também atrai
atenção política, para o bem e para o mal.

**8. Concorrência com Meta, Telegram e OpenAI.** O Playbook diz que concorrente é "história de
fantasma": 99% das startups morrem por suicídio, não por assassinato. Isso vale para a operação:
não se deve deixar de construir por causa deles. Mas vale para o posicionamento: Telegram já tem
bots e mini apps, ChatGPT e Claude já rodam MCP Apps de Canva, Figma e Asana. "Loja de apps de IA
dentro do mensageiro" não é um segredo, no sentido de Thiel; é um mercado que três gigantes já
disputam. O segredo do Zoen, se existe, está mais perto de "o agente do pequeno negócio brasileiro"
ou "a comunidade que o Discord deixou órfã" do que de "loja".

**9. Fundador solo.** Altman (Aula 2) diz que 2 ou 3 fundadores é o ideal, que um não é ótimo e
que é melhor nenhum cofundador do que um ruim. Agentes de IA substituem bem o cofundador que
programa. Não substituem quem vende para barbeiros, quem convence um criador como o Alexor Mods,
quem discorda do Enzo e quem divide o peso. Hoje, o gargalo é justamente distribuição, não código.

**10. Viralidade antes de retenção.** A visão fala em coeficiente viral K, widgets e cortes
compartilháveis. Alex Schultz (Aula 6) é explícito: retenção é a coisa mais importante para
crescer. Ele diz que mesmo um K acima de 1 não adianta sem retenção, e que só se deve trabalhar a
viralidade depois de ter muita gente retida. PG (Aula 3) diz para traduzir "growth hacks"
mentalmente por "bullshit". Widget e corte de casal são táticas; sem uma curva de retenção que
achata, eles trazem gente que vai embora.

## 4. O que as aulas mandariam fazer nos próximos 30 e 90 dias

### Próximos 30 dias

1. **Congelar o escopo.** Nada novo de infraestrutura, loja, anúncio, Kotlin, widgets ou cortes. O
   que existe basta para os primeiros 100 usuários. A única obra permitida é o que esses usuários
   pedirem (Playbook: fazer coisas que não escalam e só pensar em 10x).
2. **Falar com 50 pessoas**, no método de Emmett Shear (Aula 16): perguntar o que a pessoa fez da
   última vez, não que recurso ela quer. Por exemplo, 20 pessoas da Mensa, 15 gamers ou moderadores
   de Discord, 15 donos de pequeno negócio. Registrar tudo em `docs/research/entrevistas/`.
3. **Escolher uma rede pequena e começar por ela.** Minha recomendação é a comunidade de amigos da
   Mensa, porque o Enzo tem acesso direto, é um grupo denso e já se organiza em grupo. Thiel e
   Facebook dão a barra: 60% da comunidade dentro em dez dias. O Enzo coloca cada pessoa à mão,
   como Ben Silbermann pedia em cafés que estranhos testassem o Pinterest (Aula 1 e Playbook).
4. **Definir um momento mágico.** Schultz (Aula 6): o Facebook focava em "10 amigos em 14 dias".
   Uma proposta para o Zoen: "nos primeiros 10 minutos, a pessoa vê pelo menos 5 conhecidos no
   Space e recebe uma resposta, de alguém ou do agente da comunidade, que resolve algo de verdade".
   Medir o tempo até esse momento.
5. **Uma métrica norte: mensagens enviadas por membro ativo por semana.** Schultz (Aula 6) diz que,
   para um app de mensagem, envios são provavelmente o número mais importante, e conta que o Jan
   Koum publicava esse número do WhatsApp.
6. **Atendimento pelo fundador.** Kevin Hale (Aula 7): todo mundo faz suporte, a resposta sai em
   minutos, e ignorar um usuário é uma das maiores causas de churn no começo.

### Até 90 dias

1. **Meta de retenção.** Schultz (Aula 6) diz que a curva de "percentual ativo por dias desde a
   entrada" tem que achatar, ficando paralela ao eixo, e não cair até zero. Proposta minha, não
   das aulas: pelo menos 40% dos membros ativos na semana 4, com a curva plana entre a semana 4 e a
   semana 8. Se não achatar, consertar o produto, não fazer crescimento.
2. **Segunda comunidade só depois que a primeira achatar.** Pode ser uma comunidade de criador
   gamer no modelo Alexor Mods, que é uma tática de "fazer o que não escala" com uma pessoa só, não
   uma parceria corporativa. O Playbook avisa que acordos com outras empresas e grande lançamento
   de imprensa "effectively never work".
3. **Teste de venda com 10 pequenos negócios**, configurando o agente à mão, como Adora Cheung
   (Aula 4) fazia antes de automatizar e como a DoorDash (Aula 8) entregava comida antes de ter
   sistema. Tentar cobrar (Shear: o cartão de crédito é o teste). Isso decide se o lado empresa é
   a cunha principal ou o segundo passo.
4. **Medir o K só depois da retenção**, com o funil de convite que Schultz descreve: convites por
   pessoa, cliques, cadastros e quem convida de novo.
5. **Tirar anúncio do roadmap** até haver cerca de 100 mil usuários ativos por mês. Testar desde já
   quem paga: negócio, criador ou comunidade.
6. **Procurar um cofundador** com força em comunidade, vendas ou distribuição.

## 5. Perguntas que o Enzo precisa responder

1. Quem são os primeiros 100 usuários, com nome? Onde eles conversam hoje?
2. Por que o Zoen é valioso para a primeira pessoa de um grupo, antes de os amigos entrarem?
3. Qual é o momento mágico, em uma frase?
4. O Zoen é um mensageiro com agentes ou uma plataforma de agentes com mensagens? Qual dos dois
   ganha se só um puder existir?
5. Se amanhã o WhatsApp der um agente grátis para todo pequeno negócio, o que sobra para o Zoen?
6. Qual é o segredo, no sentido de Thiel: o que você sabe que a Meta, o Telegram e a OpenAI não
   sabem ou não querem fazer?
7. Quem paga e quando? O bot do pequeno negócio é grátis ou é o produto pago?
8. Banner patrocinado em comunidade é ou não é "anúncio para humanos"?
9. Menores de 18 anos entram? Se sim, como fica o ECA Digital com criptografia de ponta a ponta?
10. Que número em 90 dias faria você mudar de nicho ou de tese?
11. Quem é o cofundador, e o que ele faz que você e os agentes não fazem?

## Nota sobre as fontes

- Nem todos os nomes pedidos deram aula no CS183B de 2014.
  - **Zuckerberg:** não deu aula. A regra "10 amigos em 14 dias" aparece aqui citada por Alex
    Schultz na Aula 6.
  - **Livingston e Seibel:** não deram aula no curso. "Do things that don't scale" aparece na
    Aula 8, com Stanley Tang, Walker Williams e Justin Kan, e no Playbook.
  - **Hoffman:** deu a Aula 13, "How to Be a Great Founder". "Blitzscaling" é de outro curso, o
    CS183C, de 2015, e não foi usado aqui.
  - **Pirate metrics (AARRR):** são de Dave McClure, não de Kevin Hale. Não foram usadas.
  - **Conway, Conrad e Andreessen:** a Aula 9 é sobre captação, não sobre vendas. Usei a "cebola
    de riscos" de Andreessen.
  - **Silbermann:** falou de cultura e contratação na Aula 11. A história dos cafés vem da Aula 1
    e do Playbook.
- Transcrições: [iqiancheng/how-to-start-a-startup](https://github.com/iqiancheng/how-to-start-a-startup)
  (Aulas 1 a 20; os vídeos e slides estão em [startupclass.samaltman.com](https://startupclass.samaltman.com/lists/about/)).
  - Aula 1: Altman, ideia e produto
  - Aula 2: Altman, equipe e execução
  - Aula 3: Paul Graham
  - Aula 4: Adora Cheung
  - Aula 5: Peter Thiel
  - Aula 6: Alex Schultz
  - Aula 7: Kevin Hale
  - Aula 8: Tang, Williams e Kan
  - Aula 9: Andreessen, Conway e Conrad
  - Aula 13: Reid Hoffman
  - Aula 16: Emmett Shear
- Sam Altman, [The Startup Playbook](https://playbook.samaltman.com/) e
  [How to Be Successful](https://blog.samaltman.com/how-to-be-successful).
- Meta, [preço para AI Providers no WhatsApp Business](https://developers.facebook.com/documentation/business-messaging/whatsapp/pricing/ai-providers)
  (atualizado em 1/set/2026).
- Fontes internas: `docs/product/vision.md`, `docs/cost-model.md`, `docs/decisions-log.md` e a
  pesquisa de custo por usuário (PR #18).
- A pesquisa de curvas de crescimento de redes ainda estava em andamento quando este texto foi
  escrito e não foi incorporada.
