import assert from 'node:assert/strict';

export function messageById(page, id) {
  return page.locator(`.zm-main [data-testid=message][id=${JSON.stringify(id)}]`);
}

export async function waitForDeletedMessage(page, id, timeout) {
  const text = messageById(page, id).locator('.zm-deleted .zm-text');
  await text.waitFor({ timeout });
  assert.equal(await text.textContent(), 'Mensagem apagada');
}

export async function deleteForEveryone(sender, receiver, text, capture) {
  const wrong = sender.locator('.zm-main [data-testid=message]').filter({
    has: sender.locator('.zm-text', { hasText: text }),
  });
  await wrong.waitFor({ timeout: 30000 });
  const id = await wrong.getAttribute('id');
  assert.ok(id, 'The delete target must have its stable message ID.');
  await messageById(receiver, id).locator('.zm-text', { hasText: text }).waitFor({ timeout: 30000 });
  let confirmation = null;
  const confirm = async (dialog) => {
    confirmation = { type: dialog.type(), message: dialog.message() };
    if (confirmation.type === 'confirm' && confirmation.message === 'Apagar esta mensagem para todos?') {
      await dialog.accept();
    } else {
      await dialog.dismiss();
    }
  };
  sender.once('dialog', confirm);
  try {
    await wrong.hover();
    await wrong.getByRole('button', { name: 'Mais ações' }).click();
    await wrong.getByRole('menuitem', { name: 'Apagar para todos' }).click();
    const deadline = Date.now() + 30000;
    await capture('delete-clicked', { messageId: id, confirmation });
    assert.deepEqual(confirmation, { type: 'confirm', message: 'Apagar esta mensagem para todos?' });
    // Both projections must delete this exact message, within the existing budget.
    const results = await Promise.allSettled([sender, receiver].map((page) =>
      waitForDeletedMessage(page, id, Math.max(1, deadline - Date.now()))));
    const failures = results.flatMap((result, index) => result.status === 'rejected'
      ? [new Error(`${index === 0 ? 'Sender' : 'Receiver'} did not delete ${id}`, { cause: result.reason })] : []);
    if (failures.length) throw new AggregateError(failures, 'Delete did not reach both message projections.');
    await capture('delete-confirmed', { messageId: id, confirmation });
  } catch (error) {
    await capture('delete-failed', {
      messageId: id, confirmation, error: error.stack ?? String(error),
      causes: error instanceof AggregateError ? error.errors.map((cause) => ({
        error: cause.message, cause: String(cause.cause),
      })) : [],
    }).catch((captureError) => { console.error('Delete evidence could not be saved:', captureError); });
    throw error;
  } finally {
    sender.off('dialog', confirm);
  }
}
