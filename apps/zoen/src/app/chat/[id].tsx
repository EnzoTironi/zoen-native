import { renderChatScreen } from '../../components/chat-screen';
import { useChatController } from '../../components/use-chat-controller';

export default function Chat() {
  return renderChatScreen(useChatController());
}
