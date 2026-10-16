"""Guard the Compose-to-UniFFI flag wiring; core voice_notes tests cover behavior.

Run: python3 -m unittest discover -s android/tests -p 'test_voice_broadcast_wiring.py'
"""
from pathlib import Path
import unittest


SOURCE = Path(__file__).resolve().parents[1] / "app/src/main/java/xyz/tironi/zoen"


class VoiceBroadcastWiringTest(unittest.TestCase):
    def source(self, name):
        return (SOURCE / name).read_text()

    def test_thread_checkbox_reaches_composer_and_resets_only_after_send(self):
        source = self.source("ui/ChatScreen.kt")
        self.assertIn("threadRoot = root, alsoInChat = alsoInChat, onSent = { replyTo = root; alsoInChat = false }", source)

    def test_hold_release_reads_latest_checkbox_for_both_release_paths(self):
        source = self.source("media/VoiceComposer.kt")
        self.assertIn("val currentAlsoInChat by rememberUpdatedState(alsoInChat)", source)
        self.assertEqual(2, source.count("session.release(model.repository, spaceId, currentReply, thread, currentAlsoInChat)"))

    def test_review_send_preserves_checkbox(self):
        self.assertIn("VoiceReview(session, model, spaceId, reply, thread, alsoInChat)", self.source("media/VoiceComposer.kt"))
        self.assertIn("session.sendNow(model.repository, space, reply, thread, alsoInChat, onSent)", self.source("media/VoiceReview.kt"))

    def test_session_forwards_flag_from_release_and_review(self):
        source = self.source("media/VoiceSession.kt")
        self.assertEqual(2, source.count("send(repository, space, reply, thread, alsoInChat, onSent)"))
        self.assertIn("VoiceTransport.send(repository, sentClip, editor.keptTranscript, space, reply, thread, alsoInChat)", source)

    def test_transport_passes_flag_to_uniffi_and_defaults_to_thread_only(self):
        source = self.source("media/VoiceTransport.kt")
        self.assertIn("alsoInChat: Boolean = false", source)
        self.assertIn("clip.detail, transcript, reply, thread, alsoInChat)", source)


if __name__ == "__main__":
    unittest.main()
