# Repensado do zero: pessoas, comunidades e agentes

Este documento registra a visão e o plano original, com decisões que evoluíram. O nome confirmado do produto e do agente é Zoen. O [roadmap atual](roadmap-status.md) substitui o cronograma do protótipo como critério de conclusão; [páginas vivas](product/live-pages.md) detalha a referência atual de edição e colaboração. Nomes técnicos antigos seguem a [migração de nomenclatura](dev/naming.md).


> Um redesenho a partir dos 133 posters de tironi.xyz, agora com **todas as decisões tomadas**.
> Critério: **máximo de elegância e máximo de ambição.** O alvo final é jogar na liga de WhatsApp, iMessage, Slack e Ando, com o menor conjunto de ideias capaz de gerar tudo isso.
> Referências como **#063** apontam para `img/063-*.png`. Fatos externos foram checados na web em 07/10/2026; o que não pude confirmar está marcado com *(a verificar)*.
> É um documento de pensamento: nenhum código foi escrito.

---

Os capítulos abaixo preservam o texto integral, na ordem original. O índice mantém os títulos e a hierarquia das seções; os links abrem cada capítulo.

## 0. Resumo

[Leia o capítulo](repensado/00-resumo.md#0-resumo).

## Decisões (registro no estilo ADR)

[Leia o capítulo](repensado/decisoes.md#decisões-registro-no-estilo-adr).

## 1. Tese e momento mágico

[Leia o capítulo](repensado/01-tese.md#1-tese-e-momento-mágico).

### Tese, em uma frase

[Leia a seção](repensado/01-tese.md#tese-em-uma-frase).

### O que diferencia de cada referência

[Leia a seção](repensado/01-tese.md#o-que-diferencia-de-cada-referência).

### Momento mágico (primeiros 60 s)

[Leia a seção](repensado/01-tese.md#momento-mágico-primeiros-60-s).

## 2. O modelo mínimo

[Leia o capítulo](repensado/02-modelo.md#2-o-modelo-mínimo).

### Os 5 primitivos (+ 1 substrato)

[Leia a seção](repensado/02-modelo.md#os-5-primitivos--1-substrato).

### Diagrama

[Leia a seção](repensado/02-modelo.md#diagrama).

### Como os 11 capítulos colapsam

[Leia a seção](repensado/02-modelo.md#como-os-11-capítulos-colapsam).

## 3. Design da experiência

[Leia o capítulo](repensado/03-experiencia.md#3-design-da-experiência).

### Navegação: 4 abas (e por que não 5)

[Leia a seção](repensado/03-experiencia.md#navegação-4-abas-e-por-que-não-5).

### As 20 telas

[Leia a seção](repensado/03-experiencia.md#as-20-telas).

### Uma única representação de agente

[Leia a seção](repensado/03-experiencia.md#uma-única-representação-de-agente).

### Onboarding progressivo (a identidade só aparece quando há algo a proteger)

[Leia a seção](repensado/03-experiencia.md#onboarding-progressivo-a-identidade-só-aparece-quando-há-algo-a-proteger).

### Modelo de confiança (substitui ~30% das telas de revisar/confirmar)

[Leia a seção](repensado/03-experiencia.md#modelo-de-confiança-substitui-30-das-telas-de-revisarconfirmar).

### Como mostrar custo

[Leia a seção](repensado/03-experiencia.md#como-mostrar-custo).

### Onde mora a infraestrutura

[Leia a seção](repensado/03-experiencia.md#onde-mora-a-infraestrutura).

### Direção visual (alinhada ao PR 211)

[Leia a seção](repensado/03-experiencia.md#direção-visual-alinhada-ao-pr-211).

## 4. Como construir: arquitetura e tecnologias

[Leia o capítulo](repensado/04-arquitetura-tecnologias.md#4-como-construir-arquitetura-e-tecnologias).

### Princípio: os mesmos primitivos do pixel ao disco

[Leia a seção](repensado/04-arquitetura-tecnologias.md#princípio-os-mesmos-primitivos-do-pixel-ao-disco).

### Diagrama do sistema

[Leia a seção](repensado/04-arquitetura-tecnologias.md#diagrama-do-sistema).

### Paridade de recursos (WhatsApp / iMessage / Slack / Ando) mapeada nos primitivos

[Leia a seção](repensado/04-arquitetura-tecnologias.md#paridade-de-recursos-whatsapp--imessage--slack--ando-mapeada-nos-primitivos).

### Plano de tecnologia por área

[Leia a seção](repensado/04-arquitetura-tecnologias.md#plano-de-tecnologia-por-área).

### Tabela do stack (v1 → depois)

[Leia a seção](repensado/04-arquitetura-tecnologias.md#tabela-do-stack-v1--depois).

### Nostr/relays agora ou servidor convencional primeiro?

[Leia a seção](repensado/04-arquitetura-tecnologias.md#nostrrelays-agora-ou-servidor-convencional-primeiro).

### O que é v1 e o que vem depois, em uma frase por camada

[Leia a seção](repensado/04-arquitetura-tecnologias.md#o-que-é-v1-e-o-que-vem-depois-em-uma-frase-por-camada).

## 4A. Cliente Apple nativo (iOS + macOS)

[Leia o capítulo](repensado/04a-cliente-apple.md#4a-cliente-apple-nativo-ios--macos).

### Divisão de responsabilidades

[Leia a seção](repensado/04a-cliente-apple.md#divisão-de-responsabilidades).

### Estrutura do app

[Leia a seção](repensado/04a-cliente-apple.md#estrutura-do-app).

### Componentes de Liquid Glass (sistema primeiro, custom só onde faz sentido)

[Leia a seção](repensado/04a-cliente-apple.md#componentes-de-liquid-glass-sistema-primeiro-custom-só-onde-faz-sentido).

### Integrações de sistema (o fosso de qualidade)

[Leia a seção](repensado/04a-cliente-apple.md#integrações-de-sistema-o-fosso-de-qualidade).

### Especificidades do Mac

[Leia a seção](repensado/04a-cliente-apple.md#especificidades-do-mac).

### Web: só o visualizador de convite

[Leia a seção](repensado/04a-cliente-apple.md#web-só-o-visualizador-de-convite).

## 5. Arquitetura de escala e segurança

[Leia o capítulo](repensado/05-escala-seguranca.md#5-arquitetura-de-escala-e-segurança).

### 5.1 Escalar o chat

[Leia a seção](repensado/05-escala-seguranca.md#51-escalar-o-chat).

### 5.2 Segurança

[Leia a seção](repensado/05-escala-seguranca.md#52-segurança).

## 6. Comunidades (como construir)

[Leia o capítulo](repensado/06-comunidades.md#6-comunidades-como-construir).

### Três níveis de privacidade (visíveis no cabeçalho)

[Leia a seção](repensado/06-comunidades.md#três-níveis-de-privacidade-visíveis-no-cabeçalho).

### Papéis, convites e regras

[Leia a seção](repensado/06-comunidades.md#papéis-convites-e-regras).

### Moderação

[Leia a seção](repensado/06-comunidades.md#moderação).

### Descoberta (Explorar)

[Leia a seção](repensado/06-comunidades.md#descoberta-explorar).

### Feed

[Leia a seção](repensado/06-comunidades.md#feed).

### Tiers pagos e repasses

[Leia a seção](repensado/06-comunidades.md#tiers-pagos-e-repasses).

### Agentes nas comunidades

[Leia a seção](repensado/06-comunidades.md#agentes-nas-comunidades).

## 7. Protocolos: existe algum que resolva tudo?

[Leia o capítulo](repensado/07-protocolos.md#7-protocolos-existe-algum-que-resolva-tudo).

### Veredito para o nosso desenho em camadas

[Leia a seção](repensado/07-protocolos.md#veredito-para-o-nosso-desenho-em-camadas).

## 8. Signal como inspiração?

[Leia o capítulo](repensado/08-signal.md#8-signal-como-inspiração).

## 9. Roadmap por fases (atualizado para o cliente Apple nativo)

[Leia o capítulo](repensado/09-roadmap.md#9-roadmap-por-fases-atualizado-para-o-cliente-apple-nativo).

## 10. O que eu construiria primeiro (12 semanas)

[Leia o capítulo](repensado/10-primeiras-semanas.md#10-o-que-eu-construiria-primeiro-12-semanas).

### Marcos

[Leia a seção](repensado/10-primeiras-semanas.md#marcos).

### Workspace Rust (`zoen-native/`)

[Leia a seção](repensado/10-primeiras-semanas.md#workspace-rust-zoen-native).

### Módulos SwiftUI (`apple/`)

[Leia a seção](repensado/10-primeiras-semanas.md#módulos-swiftui-apple).

### As 5 primeiras telas

[Leia a seção](repensado/10-primeiras-semanas.md#as-5-primeiras-telas).

### "Demo que impressiona"

[Leia a seção](repensado/10-primeiras-semanas.md#demo-que-impressiona).

## 11. Correções das inconsistências

[Leia o capítulo](repensado/11-inconsistencias.md#11-correções-das-inconsistências).

## 12. Como isso se encaixa no Zoen e no signal-rust (decidido)

[Leia o capítulo](repensado/12-zoen-signal-rust.md#12-como-isso-se-encaixa-no-zoen-e-no-signal-rust-decidido).

## Apêndice: destino de cada uma das 133 telas

[Leia o capítulo](repensado/apendice-telas.md#apêndice-destino-de-cada-uma-das-133-telas).

