import assert from "node:assert/strict";

/** Keep the real app/assets online while denying only this browser's relay.
 * No authentication replies, core state or encrypted frames are fabricated. */
export async function acceptOfflineMessageJourney({
  page,
  context,
  relay,
  space,
  author,
  tag,
  retainedMessages,
  peerHome,
  cli,
  expectMessage,
  until,
  screenshot,
  step,
}) {
  const socketUrl = new URL("/v1/sync", relay);
  socketUrl.protocol = "ws:";
  let unavailable = true;
  let blockedConnections = 0;
  const relayHttp = (url) => url.origin === relay.origin;
  const httpFault = (route) => (unavailable ? route.abort("connectionfailed") : route.continue());
  await context.route(relayHttp, httpFault);
  await context.routeWebSocket(
    (url) => url.href === socketUrl.href,
    (socket) => {
      if (!unavailable) {
        socket.connectToServer();
        return;
      }
      blockedConnections++;
      return socket.close({ code: 4000, reason: "Acceptance relay unavailable" });
    },
  );
  const text = `Expo acceptance offline retained message ${tag}`;
  try {
    await page.reload({ waitUntil: "domcontentloaded" });
    await until(() => blockedConnections > 0, "cold startup did not encounter the relay fault");
    for (const message of retainedMessages)
      await expectMessage(message.author, message.text, 30_000, message.id);
    assert.equal(await page.getByRole("alert").count(), 0, "relay loss must not close the account");

    await page.getByTestId("message-input").fill(text);
    await page.getByTestId("send-message").click();
    const messageId = await expectMessage(author, text);
    for (const message of retainedMessages)
      await expectMessage(message.author, message.text, 30_000, message.id);
    const bubble = () => page.getByTestId(`message-bubble-${messageId}`);
    await bubble().getByTestId("message-receipt-sending").waitFor();
    const before = await cli(peerHome, "read", space);
    assert.ok(!before.includes(text), "an offline browser message must not reach the real peer");

    const blockedBeforeReload = blockedConnections;
    await page.reload({ waitUntil: "domcontentloaded" });
    await until(
      () => blockedConnections > blockedBeforeReload,
      "reopened account did not encounter the still-unavailable relay",
    );
    await expectMessage(author, text, 30_000, messageId);
    for (const message of retainedMessages)
      await expectMessage(message.author, message.text, 30_000, message.id);
    await bubble().getByTestId("message-receipt-sending").waitFor();
    assert.equal(await page.getByRole("alert").count(), 0);
    await screenshot("04a-cold-relay-offline-retained-message.png");
    step("relay-offline-cold-start-retains-history-and-durable-outbox", {
      retainedMessageIds: retainedMessages.map((message) => message.id),
      queuedMessageId: messageId,
      peerDidNotReceiveWhileBlocked: true,
      blockedConnections,
    });

    unavailable = false;
    await bubble()
      .getByTestId(/^message-receipt-(sent|delivered|read)$/)
      .waitFor({ timeout: 60_000 });
    const sync = await cli(peerHome, "sync");
    assert.match(sync, /connection=online synced=true pending=0(?:\s|$)/);
    const delivered = await cli(peerHome, "read", space);
    assert.equal(
      delivered.split(text).length - 1,
      1,
      "peer must retain exactly one visible message",
    );

    await page.reload({ waitUntil: "domcontentloaded" });
    await expectMessage(author, text, 30_000, messageId);
    for (const message of retainedMessages)
      await expectMessage(message.author, message.text, 30_000, message.id);
    await bubble()
      .getByTestId(/^message-receipt-(sent|delivered|read)$/)
      .waitFor();
    await cli(peerHome, "sync");
    const reopened = await cli(peerHome, "read", space);
    assert.equal(
      reopened.split(text).length - 1,
      1,
      "cold reopen must not duplicate the peer entry",
    );
    step("relay-reconnect-and-cold-reopen-retain-one-confirmed-message", {
      messageId,
      exactPeerOccurrences: 1,
    });
    return { author, text, id: messageId };
  } finally {
    unavailable = false;
    await context.unroute(relayHttp, httpFault);
  }
}
