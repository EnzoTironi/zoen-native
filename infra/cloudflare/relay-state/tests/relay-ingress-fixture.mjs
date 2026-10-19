// Test-only inspection of fresh owned cells; production never imports this file.
import { DirectoryState as ProductionDirectory } from "../src/directory-state.mjs";
export { default, RelayIngress, RelayDispatch } from "../src/directory-index.mjs";

export class DirectoryState extends ProductionDirectory {
  ingressInventory() {
    return JSON.stringify({
      connections: this.ctx.storage.sql
        .exec("SELECT count(*) AS n FROM directory_connections")
        .toArray()[0].n,
      challenges: this.ctx.storage.sql
        .exec("SELECT count(*) AS n FROM directory_challenges")
        .toArray()[0].n,
      sessions: this.ctx.storage.sql
        .exec("SELECT count(*) AS n FROM directory_sessions")
        .toArray()[0].n,
    });
  }
}
