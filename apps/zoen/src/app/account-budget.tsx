import { useLocalSearchParams } from 'expo-router';
import { AccountBudgetScreen } from '../agents/account-budget-screen';
export default function AccountBudget() {
  const { agent } = useLocalSearchParams<{ agent?: string }>();
  return <AccountBudgetScreen agentId={agent} />;
}
