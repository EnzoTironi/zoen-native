// Acceptance copy is authored independently of production translation code.
// Never import application catalogs or translate the synthetic user content here.
export const expoWebLocales = Object.freeze(["en-US", "pt-BR"]);
const expectations = Object.freeze({
  "en-US": Object.freeze({
    developmentOptions: "Developer options",
    connectOtherDevice: "Connect with another device",
    showPairingCode: "Show code",
    pairingQr: "QR code to connect this device",
    pairingCode: "This device’s connection code",
    verificationPrefix: "Verification code:",
    eraseDialog: "Sign out of this device?",
    cancel: "Cancel",
    confirmErase: "Sign out and erase data",
    importMarkdown: "Import Markdown file",
    importAndSharePage: "Import and share page",
    editBlock: (text) => `Edit block: ${text}`,
    publishedPage: "Shared in chat",
    savedDraft: "Draft saved on this device",
    sharePageChanges: "Share changes",
    blockContent: "Block content in Markdown",
    saveDraft: "Save draft",
    exportMarkdown: "Export Markdown",
  }),
  "pt-BR": Object.freeze({
    developmentOptions: "Opções de desenvolvimento",
    connectOtherDevice: "Conectar com outro aparelho",
    showPairingCode: "Mostrar código",
    pairingQr: "QR code para conectar este dispositivo",
    pairingCode: "Código de conexão deste dispositivo",
    verificationPrefix: "Código de verificação:",
    eraseDialog: "Sair deste aparelho?",
    cancel: "Cancelar",
    confirmErase: "Sair e apagar dados",
    importMarkdown: "Importar arquivo Markdown",
    importAndSharePage: "Importar e compartilhar página",
    editBlock: (text) => `Editar bloco: ${text}`,
    publishedPage: "Compartilhada na conversa",
    savedDraft: "Rascunho salvo neste aparelho",
    sharePageChanges: "Compartilhar alterações",
    blockContent: "Conteúdo do bloco em Markdown",
    saveDraft: "Guardar rascunho",
    exportMarkdown: "Exportar Markdown",
  }),
});

export function expoWebExpectations(locale) {
  if (!expoWebLocales.includes(locale))
    throw new Error("ZOEN_EXPO_TEST_LOCALE must explicitly select en-US or pt-BR.");
  return expectations[locale];
}

export function pairingScreenshotMasks(page) {
  // Mask both languages even if a localization regression shows the other copy.
  return expoWebLocales.flatMap((locale) => {
    const ui = expoWebExpectations(locale);
    return [
      page.getByLabel(ui.pairingQr, { exact: true }),
      page.getByLabel(ui.pairingCode, { exact: true }),
      page.locator(`[aria-label^=${JSON.stringify(ui.verificationPrefix)}]`),
    ];
  });
}
