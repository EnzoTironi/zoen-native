[Índice](../repensado.md) · [Anterior](02-modelo.md) · [Próximo](04-arquitetura-tecnologias.md)

## 3. Design da experiência

### Navegação: 4 abas (e por que não 5)

```
┌──────────────────────────────────────────────────────┐
│  (  Conversas   Atividade   Explorar   Você  )   (⌕) │  ← cápsula de vidro + botão de busca/nova separado
└──────────────────────────────────────────────────────┘
```

1. **Conversas:** todos os seus Espaços numa lista só. DMs com pessoas, com agentes e grupos aparecem misturados por recência. Comunidades aparecem como **pilhas** que abrem seus canais (Espaços-filhos). Filtros em pílula: Todas · Pessoas · Agentes · Comunidades (#011 melhorado).
2. **Atividade:** *o que precisa de você e o que está sendo feito por você.* Topo: pedidos de agentes em lote. Meio: tarefas em andamento, com progresso ao vivo (#051). Embaixo: menções e respostas. É aqui que o modelo de confiança vive (#024 promovido a aba).
3. **Explorar:** Rede (Seguindo/Para você), comunidades e agentes públicos. É o motor de crescimento e a vitrine dos criadores (#091 + #115 numa tela só).
4. **Você:** seu perfil público e seus rascunhos, além de **Agentes**, **Itens** (todos os arquivos e docs, de todos os Espaços), **Memória** (o que os agentes sabem de você), **Acessos** (assinaturas, compras e o que você concedeu), **Segurança** e **Ajustes**. Uma única tela de ajustes.
5. **Botão de vidro à direita** (o padrão de "search tab" do iOS 26): busca global. Puxado para cima, vira **Nova** (conversa, grupo, agente, Item).

**Por que "Agentes" não é aba:** uma aba de agentes trata o agente como ferramenta. Na tese, agente é **membro**: ele aparece onde as pessoas aparecem (Conversas, Atividade, perfis). Configurar agentes é raro e mora em Você. Além disso, o botão central elevado dos posters é um padrão Android/Instagram que briga com a tab bar em cápsula do Liquid Glass.

**Por que Arquivos não é aba:** arquivo é Item, e todo Item vive num Espaço. A visão "todos os meus itens" é um filtro (Você › Itens ou Busca), não um destino diário.

### As 20 telas
| # | Tela | Absorve (exemplos) |
|---|---|---|
| 1 | Conversas | #011, #098, #122 |
| 2 | **Espaço** (conversa) | #015, #036, #043, #049, #117, #041/#042 (pílula de privacidade), #045–#048 (cartões) |
| 3 | Info do Espaço | #016, #017, #100, #102, #101 (seção Acesso) |
| 4 | Atividade | #024, #040, #087, #118 (sugestões) |
| 5 | Busca | #025, #067 |
| 6 | Explorar | #091, #115 |
| 7 | **Perfil universal** (pessoa, agente ou Espaço público) | #116, #027, #092, #097, #029/#030 (seções do agente) |
| 8 | Criar agente (conversando) | #028, #031 |
| 9 | **Item** (documento, arquivo, tarefa) | #064, #052, #071, #084, #085 |
| 10 | Versões | #073, #033, #034, #074 |
| 11 | Comparar e juntar | #076, #077, #078 |
| 12 | Conceder (compartilhar) | #072, #086, #038 |
| 13 | Ao vivo | #056, #060, #063 |
| 14 | Ofertas (para criadores) | #103, #101 |
| 15 | Você (hub) | *nova* |
| 16 | Você › Memória | #082, #083, #090 |
| 17 | Você › Segurança | #130, #004, #125 |
| 18 | Você › Ajustes | #132, #133, #128 (Avançado) |
| 19 | Entrar | #005, #006, #007 |
| 20 | Conectar aparelho | #008, #009, #010 |

### Uma única representação de agente
- **Pessoa:** avatar **circular** com foto.
- **Agente:** avatar em **squircle** (quadrado arredondado, como ícone de app) com um glifo monocromático sobre um gradiente da cor escolhida, mais um **mini-avatar circular do dono** no canto inferior direito. O "de quem é" fica visível sem texto.
- **Espaço/comunidade:** squircle com foto de capa e sem mini-avatar.
- **Nome:** "Financeiro" no título e "Financeiro · Enzo" como subtítulo quando o agente não é seu.
- **Morrem:** o selo "AG" de letras, o cubo 3D, os ícones que mudam de cor entre telas e a palavra "Agente" como badge de texto. A forma diz que é agente; o mini-avatar diz de quem é.
- **Estados no próprio avatar:** um anel animado quando está trabalhando, um ponto âmbar quando espera você e um anel parcial como medidor quando está perto do limite de orçamento.

### Onboarding progressivo (a identidade só aparece quando há algo a proteger)
| Momento | O que acontece | O que o usuário vê |
|---|---|---|
| Abrir o app | Identidade gerada no aparelho (chave no Secure Enclave/Keystore) | Nada. Só a conversa com o agente. |
| Primeiro Item criado | — | Nada. Valor primeiro. |
| Convidar alguém ou instalar em outro aparelho | Pede um nome e (opcional) uma foto | "Como a Marina vai te ver?" (uma linha) |
| ~3 Itens ou 1º grupo | **Passkey**: a chave raiz é cifrada com o PRF da passkey e sincroniza pelo iCloud Keychain/Google | Cartão "Proteja o que é seu", 1 toque, Face ID |
| Pagar, criar comunidade ou publicar | Verificação de contato (e-mail ou telefone) para recuperação e descoberta | Folha curta |
| Avançado (opcional) | Código de recuperação, assinador externo, exportar chave | Você › Segurança |

As palavras "chave privada", "assinador" e "backup criptografado" **nunca aparecem no fluxo principal**. A soberania está lá por baixo; a UI fala de "seus aparelhos" e "proteger".

### Modelo de confiança (substitui ~30% das telas de revisar/confirmar)

**1. Nível de confiança por agente e por Espaço** (um controle só, no toque do membro):
| Nível | O agente… | Padrão para |
|---|---|---|
| **Ouvir** | Lê só quando chamado e responde com texto | Agentes de terceiros, comunidades |
| **Sugerir** | Prepara rascunhos e propostas; nada sai sem você | Agentes novos |
| **Agir** | Faz o que é reversível dentro do Espaço e do orçamento, sempre com desfazer | Seus agentes nos seus Espaços |
| **Autônomo** | Também age fora (agenda, e-mail, pagamentos pequenos) dentro dos limites | Escolha explícita |

**2. Três leis simples:**
- **Reversível → faz e mostra "Desfazer".** Como tudo é versionado, quase tudo é reversível: editar, mover, criar, renomear, restaurar.
- **Irreversível ou externo → pede.** Enviar para fora, pagar, apagar de verdade, publicar para uma audiência maior.
- **Linhas vermelhas fixas:** dinheiro acima do teto, nova audiência pública e dados de terceiros sempre pedem, em qualquer nível.

**3. Pedidos em lote:** pedidos não interrompem; acumulam em Atividade como *"Financeiro quer 3 coisas"*, com Aprovar todos / Revisar. Cada pedido é um cartão (#045 simplificado) com **o que muda, para quem e quanto custa**. A aprovação fica presa ao hash do conteúdo, então editar invalida sozinho (fim do #121 como tela).

**4. A confiança cresce com o histórico:** depois de N aprovações sem edição de um mesmo tipo de ação, o app sugere "Deixar o Financeiro fazer isso sozinho?". Confiança se conquista, não se configura.

### Como mostrar custo
- **Nunca por mensagem.** Custo por mensagem gera ansiedade e convida a comparar com o ChatGPT "ilimitado".
- **Orçamento mensal em R$** (nada de tokens nem de "créditos"), por agente e por Espaço. O medidor é o anel no avatar e uma linha no perfil (#030 simplificado).
- **Custo da tarefa no recibo** (#048), só quando você abre o detalhe.
- Alerta aos 80% e parada no teto, com "Aumentar limite" (exemplo: "+R$ 20 este mês").
- **Regra de quem paga, numa frase:** *quem é dono do agente paga o agente.* Nas comunidades, o criador decide quanto do orçamento dos agentes dele cada tier inclui (ver §6).

### Onde mora a infraestrutura
Em Você › Ajustes › **Avançado**, fechado por padrão: provedor de modelo, onde os agentes rodam, relay/host do Espaço, armazenamento e exportar/mover Espaço. Para quase todos os usuários, isso é "Automático". A única infra que aparece na superfície é um selo de privacidade no cabeçalho do Espaço: 🔒 *Ponta a ponta*, ou 🌐 *Público*.

### Direção visual (alinhada ao PR 211)
- **Pele do PR 211:** Zoen Night (cinzas azulados), Mona Sans com o itálico do Instrument Serif só em momentos editoriais, e um azul de ação.
- **Liquid Glass só na camada de navegação e controle:** a tab bar em cápsula que encolhe ao rolar, o cabeçalho que vira vidro, o composer, as folhas e os cartões de pedido flutuantes. O conteúdo (mensagens, Itens) **não é vidro**: fica sólido e legível.
- **Large titles** nas raízes e compactos ao rolar, alvos de 44 pt, Dynamic Type, e "Reduzir transparência" troca vidro por sólido.
- **Movimento com significado:** o cartão do Item "nasce" da mensagem do agente, o desfazer desliza do composer e o anel do avatar respira enquanto o agente trabalha.
- **Mudanças em relação aos posters:** sai o preto puro e entra o Night; sai a tab bar opaca com FAB e entra a cápsula de vidro; botões primários viram pílulas; o modo claro passa a ser cidadão de primeira classe.

---

