[Research index](analise-critica-zoen.md).

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
