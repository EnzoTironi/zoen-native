# Interface

Evite rótulos e descrições secundárias desnecessárias. Deixe o layout fazer a explicação.

Mantenha os rótulos necessários para decisões claras e acessibilidade; não confunda simplificação visual com remover informações essenciais.


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
