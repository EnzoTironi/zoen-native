/** Native encrypted backups are not browser sessions. Never collect a recovery key here. */
export function RecoveryHelp() {
  return <details className="zw-recovery">
    <summary>Recuperar minha conta</summary>
    <p>Perdeu todos os aparelhos? Abra o aplicativo Zoen em um celular ou Mac sem conta conectada. Na tela inicial, escolha Recuperar minha conta e use seu @, o endereço do servidor e a chave de 64 números que você guardou.</p>
    <p>Depois de recuperar no aplicativo, volte aqui para conectar este navegador em Aparelhos.</p>
    <p>Este navegador não restaura cópias da conta. Não digite nem envie sua chave aqui. Sem a chave ou outro aparelho conectado, o Zoen não pode recuperar suas conversas.</p>
    <p>Já usava uma cópia com senha? Ela não foi apagada. Mantenha um aparelho conectado; a recuperação por senha não está disponível nesta tela.</p>
  </details>;
}
