import { Redirect } from 'expo-router';

/** SessionGate owns onboarding; a complete local session proceeds to conversations. */
export default function SignInRoute() {
  return <Redirect href="/" />;
}
