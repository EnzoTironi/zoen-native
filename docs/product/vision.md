# Visão do produto — anotações

> Anotações de produto (Enzo + Grok Bot, 8–9 out 2026). Guiam decisões; não são promessas de escopo.
>
> Regra: registramos tudo para revisar depois, inclusive ideias que podem ser descartadas.

## Tese

Zoen é o "WhatsApp 2": um mensageiro privado onde, além de conversar, as pessoas usam e criam apps e agentes na hora. Comunicação, comunidade, trabalho e comércio no mesmo lugar, com privacidade e segurança de verdade.

## Democratização

- Todo mundo pode ter o próprio agente, de graça. Pequenos negócios (confeiteiras, barbeiros, lojinhas de bairro) saem da "corrente do WhatsApp" de atender tudo na mão: o agente responde, agenda, vende e cobra por eles.
- Para quem só tem celular, Zoen é o computador no bolso: criar, vender e trabalhar sem precisar de um PC.
- O Brasil é o mercado de entrada (WhatsApp com ~93% de penetração).
- Responsabilidade: agente só lê conversas onde é membro declarado; ações sensíveis passam por aprovação; o sandbox nunca vê senhas.

## Plataformas

- iOS e macOS nativos hoje (SwiftUI).
- Android em Kotlin/Jetpack Compose: o núcleo em Rust gera bindings Kotlin pelo mesmo UniFFI, então a lógica (criptografia, sync, arquivos, regras) vem pronta; o trabalho é a interface e as partes de plataforma (push, Keystore, passkeys, câmera).
- Web com framework web de verdade, núcleo Rust via WebAssembly. Não é landing page: é a porta de entrada instantânea pelos links de convite. Onde precisar de algo nativo, convite para instalar o app.

## Crescimento

### Viralidade
- Estudar os apps mais virais da App Store e do Google Play e reaproveitar formatos e estratégias de marketing já validados.
- O gancho tem que ser o que o agente entrega de verdade, nunca promessa vazia.
- Convites para Spaces e conversas abrem direto na web: a pessoa entra e já usa.

### B2C: crescimento viral sem pagar anúncio
- Tese: o B2C traz crescimento viral orgânico, sem custo de anúncio, e complementa o B2B dos pequenos negócios. As pessoas chegam pelo uso pessoal e levam o Zoen para o trabalho e para as lojas onde compram (e vice-versa).
- Widgets nativos na tela inicial e na tela de bloqueio do iPhone, aproveitando que o app é nativo. Exemplo de referência: o widget do burrinho, algo fofo que as pessoas querem mostrar e compartilhar. Os widgets seguem o estilo desenhado à mão e animado do Zoen.
- Momentos de app de casal: cortes curtos e compartilháveis (lembranças, conquistas, momentos juntos) gerados a partir do uso, prontos para stories e vídeos curtos, reforçando o efeito viral.
- Tudo que é compartilhável respeita a privacidade: nada sai de uma conversa sem a pessoa escolher compartilhar.

### Comunidades de entrada
- **Mensa:** Enzo tem amigos na Mensa e quer criar a comunidade deles no Zoen. É uma comunidade pronta, engajada e com muitos membros, um bom primeiro grupo.
- **Nicho gamer:** atacar o público gamer brasileiro como alternativa ao Discord. Preparar as comunidades para criadores com influência: ferramentas de criador, bots treinados no conteúdo e monetização.
  - **Fato validado:** no Brasil, o Discord está com Go Live, chamadas de vídeo e compartilhamento de tela suspensos por medida preventiva da ANPD com base no ECA Digital (Lei 15.211/2025). A medida foi expedida em 12/ago/2026 e está em vigor desde meados de agosto; em 24/set/2026 a ANPD negou o pedido de retomada, e o recurso foi ao Conselho Diretor. Voz e texto continuam funcionando. Fontes: [ANPD](https://www.gov.br/anpd/pt-br/assuntos/noticias/em-medida-preventiva-anpd-determina-que-discord-suspenda-transmissoes-ao-vivo-no-brasil), [Discord](https://support.discord.com/hc/pt-br/articles/42704051358359), [O Globo](https://oglobo.globo.com/brasil/noticia/2026/09/25/agencia-nega-pedido-do-discord-e-lives-seguem-suspensas-no-brasil-plataforma-nao-apresentou-provas-do-bloqueio.ghtml).
  - **Timing:** o concorrente acabou de abrir um vazio no nicho gamer brasileiro, e é a hora de entrar.
  - **Cuidado:** a mesma regra vale para nós. A ANPD citou que a criptografia de ponta a ponta do Discord tornou o produto "menos protetivo" para menores. Se o Zoen oferecer vídeo ao vivo ou compartilhamento de tela no Brasil, precisa nascer em conformidade com o ECA Digital: verificação de idade, padrões seguros para menores, controles de quem pode transmitir para quem e denúncia, sem quebrar a criptografia. Sem isso, corremos o mesmo risco de suspensão (multa de até R$ 50 milhões).
- **Estratégia Alexor Mods (evento concreto):**
  1. Escolher a comunidade de um influenciador gamer, como o Alexor Mods.
  2. Montar a comunidade dele no Zoen e treinar os bots no conteúdo dele.
  3. Chegar com tudo pronto, ainda desativado para os usuários.
  4. Oferecer o lançamento em parceria com ele.
  - Generalizável: um playbook repetível de "comunidade pronta para o criador".
  - Cuidado: usar só conteúdo público dele e pedir autorização antes de abrir para qualquer pessoa.

### SEO no estilo n8n
- Cada caso de uso, tipo de negócio e integração vira uma página indexável (ex.: "agente para barbearia", "agente que agenda pelo WhatsApp", "Canva + Zoen").
- As páginas mostram o mini app/agente funcionando e levam direto para usar.

### Parcerias win-win com apps grandes
- Apps que já rodam dentro de mensageiros são parceiros naturais: Zoen traz distribuição e comunidades; eles trazem um caso de uso pronto.
- Exemplos:
  - **Ditto** (Popcorn AI Tech, região de São Francisco): namoro com IA via iMessage, sem app; um match e um encontro planejado toda quarta. Mais de 160 mil estudantes cadastrados e mais de 80 mil encontros (cofundador Allen Wang à CBS, ago/2026).
  - **YouMatch** (Bulgária): mini app no Telegram, ~10–12 mil usuários, marca encontros a cada duas semanas com análise de personalidade.

## Store e MCP Apps

- MCP Apps é o padrão aberto lançado em 26/jan/2026 (com OpenAI e MCP-UI) para ferramentas devolverem telas interativas dentro da conversa. Já roda no Claude, ChatGPT, Goose e VS Code.
- Implementar o padrão faz Zoen herdar de uma vez os apps desse ecossistema.
- Prioridades para a Store: Canva, Figma, Hex, Amplitude, Asana, monday.com, Box (também Slack, Clay e Salesforce).
- Encaixe:
  - A tela HTML do app abre isolada dentro do card de mini app, com a animação de virar.
  - Telas simples (formulários, listas, confirmações) são desenhadas nativamente; HTML é reserva.
  - Ações que mudam algo passam pelos cards de aprovação de arrastar.
  - Aviso claro de privacidade: um app de terceiro só vê o conteúdo se alguém do grupo autorizar, porque ele fica fora da nossa criptografia.
- Os nossos próprios mini apps usam o mesmo padrão, então funcionam também fora do Zoen.

## Comércio sem intermediário

- Lojas publicam catálogos como MCP Apps: cardápio, produtos, agenda.
- O cliente pede direto na conversa, sem intermediário tipo iFood e sem taxa sobre o pedido.
- O agente da loja confirma, cobra e acompanha.

## Publicidade

- Anúncio direto para humanos é proibido.
- Publicidade para bots: o anúncio precisa convencer o agente do usuário, que filtra notícias, spam e ofertas pelo que importa para a pessoa (como o filtro de spam do e-mail). Isso inverte o incentivo dos apps sociais.
- Dentro das comunidades: um bannerzinho discreto com selo de "patrocinado", um canal novo e barato para lojas locais alcançarem o bairro.
  - Em aberto: como isso convive com a regra de não anunciar para humanos (proposta: só em comunidades que optarem, sempre identificado, sem uso de conteúdo das conversas para segmentar).
- Conta por usuário (custo de IA e infraestrutura contra anúncio, comissão e bots para empresas): [research/unit-economics.md](../research/unit-economics.md). Resumo: só anúncio não cobre o cenário médio (US$ 0,047 de custo contra ~US$ 0,02 de anúncio por usuário por mês); quem fecha a conta é cobrar a empresa (Bot Pro, mensagens de empresa, comissão), nunca o consumidor.

## Trabalho

- A ontologia transforma conversa em trabalho organizado: pedidos, agenda, clientes, cobranças.
- Empresas conectam suas bases de dados; agentes e apps operam os mesmos dados com permissões e auditoria.
