import type { Dialogs } from '../components/dialog-types';
import type { TrustLevelDto } from '../core/generated/TrustLevelDto';
import { t } from '../i18n/index.ts';

export function agentTrustLevels() {
  return [
    { value: 'Listen', title: t('agents.listen'), detail: t('agents.followsTheConversation') },
    {
      value: 'Suggest',
      title: t('agents.suggest'),
      detail: t('agents.proposesChangesAndAsksForYourApproval'),
    },
    {
      value: 'Act',
      title: t('agents.act'),
      detail: t('agents.performsAllowedActionsWithAnOptionToUndo'),
    },
    {
      value: 'Autonomous',
      title: t('agents.autonomous'),
      detail: t('agents.actsWithinThePermissionsAndBudgetYouSet'),
    },
  ] satisfies { value: TrustLevelDto; title: string; detail: string }[];
}
/** Shared profile/editor consent copy. The caller must recheck its captured scope
 * after the dialog; confirmation never grants authority on its own. */
export async function confirmAgentTrust(
  dialogs: Dialogs,
  name: string,
  chat: string,
  current: TrustLevelDto,
  desired: TrustLevelDto,
  explicit = false,
) {
  const levels = agentTrustLevels();
  const level = levels.find((candidate) => candidate.value === desired);
  if (!level) return false;
  return (
    (!explicit &&
      levels.findIndex((candidate) => candidate.value === desired) <=
        levels.findIndex((candidate) => candidate.value === current)) ||
    dialogs.confirm({
      title: t('agents.allowAutonomy', { level: level.title }),
      message: t('agents.autonomyConfirmation', { name, chat, detail: level.detail }),
      confirmLabel: t('agents.updateAutonomy'),
    })
  );
}
