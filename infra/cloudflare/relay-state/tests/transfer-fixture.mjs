// Test-only route to private service methods; production still has no HTTP route.
export { DirectoryState } from "./media-fixture.mjs";
import MediaFixture from "./media-fixture.mjs";

export default class TransferFixture extends MediaFixture {
  fetch(request) {
    const url = new URL(request.url);
    if (url.pathname.startsWith("/transfer-facade/")) {
      url.pathname = url.pathname.slice("/transfer-facade".length);
      return super.transfer(new Request(url, request));
    }
    if (url.pathname.startsWith("/v1/transfer/"))
      return this.env.DIRECTORY.getByName(
        url.searchParams.get("name") ?? "private-transfer-fixture",
      ).transfer(request);
    // Existing local bridge chooses its own cell; use the same name for controls.
    if (!url.searchParams.has("name")) url.searchParams.set("name", "private-transfer-fixture");
    return super.fetch(new Request(url, request));
  }
}
