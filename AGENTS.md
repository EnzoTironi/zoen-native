# Interface

Evite rótulos e descrições secundárias desnecessárias. Deixe o layout fazer a explicação.

Mantenha os rótulos necessários para decisões claras e acessibilidade; não confunda simplificação visual com remover informações essenciais.

# Engenharia

Busque a solução mais elegante e ambiciosa que entregue a visão completa do produto. Prefira uma estrutura simples e reutilizável, uma única autoridade de estado e jornadas concluídas de ponta a ponta. Resolva a causa dos problemas.

Aplique nesta ordem:

1. **Questione os requisitos.** Relacione cada requisito ao problema, ao pedido de origem e a um responsável nominal no issue. Enzo é o responsável pela direção de produto; identifique quem propôs cada requisito técnico derivado. Valide a necessidade e o resultado esperado.
2. **Elimine partes e etapas dispensáveis.** Remova código, estado, dependências, caminhos legados, relatórios e passagens de aprovação que não contribuam para a entrega ou para uma proteção demonstrável.
3. **Simplifique a solução restante.** Reutilize os contratos e componentes adequados, consolide responsabilidades e preserve as garantias de autorização, integridade e recuperação. Mantenha o escopo e os critérios aprovados do produto.
4. **Acelere o ciclo.** Trabalhe em frentes independentes e lotes coerentes, com um responsável por fonte, dispositivo e serviço. Execute decisões rotineiras dentro da autorização existente; dimensione os checks ao impacto real.
5. **Automatize o fluxo simplificado.** Automatize o trabalho repetitivo depois de comprovar o caminho. Prefira ferramentas mantidas e mecanismos existentes.

Use essa ordem para tomar decisões e executar mudanças. Mantenha um registro conciso por entrega e evite criar novos checklists, relatórios ou gates para demonstrar que o processo foi seguido.

Execute o driver local de QA sempre por `python3 scripts/qa-device.py <verb> ...`; `fill` e `type` são bloqueados no driver padrão. A alternativa privada exige a fonte exata qualificada e a configuração isolada documentada.
Siga [o guia do driver iPhone](docs/dev/iphone-qa-driver.md); não contorne a guarda pelo driver bruto ou outra interface.

# Rust e tratamento de erros

Use o compilador e o Clippy como proteção de projeto. Preserve Result/Option,
exaustividade, ownership e limites de tipos; corrija a causa dos diagnósticos.
Não adicione allow/expect de lint, unsafe, casts sem verificação ou valores padrão
para contornar uma falha de compilação, um lint ou um erro operacional.

Não use unwrap()/expect() em código de produção. Propague erros recuperáveis com
contexto e recuse estados inválidos explicitamente. Não troque panic por fallback
silencioso, perda de dados, autorização implícita ou resultado de sucesso fictício.
Quando uma invariável puder ser representada por tipos ou um construtor validado,
represente-a assim. Exceções reais precisam de justificativa específica e revisão.
Asserções de testes são distintas; não enfraqueça testes para eliminar diagnósticos.

Todos os crates mantidos herdam workspace.lints. O CI executa Clippy com todos os
targets e -D warnings, incluindo pedantic, perf e as regras explícitas de pânico.
Avalie regras de nursery/restriction individualmente; não habilite grupos
contraditórios em bloco nem suprima os avisos em massa. Checks estáticos não
substituem testes, profiling ou provas de recuperação e autorização.
