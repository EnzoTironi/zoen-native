import { SkyScene } from '../../components/sky-background';
import { Loading } from '../../components/primitives';
import { useClient } from '../../core/provider';
import { homeOwnerFor } from '../../home/access';
import { WorkspaceBrowser } from '../../documents/workspace-view';

export default function Files() {
  const state = useClient();
  const owner = homeOwnerFor(state);
  // Replace the read/filter owner before another device/account can render old rows.
  const scope = owner ? JSON.stringify([state.store.sessionEpoch, owner]) : null;
  return <SkyScene tabScene>{scope ? <WorkspaceBrowser key={scope} /> : <Loading />}</SkyScene>;
}
