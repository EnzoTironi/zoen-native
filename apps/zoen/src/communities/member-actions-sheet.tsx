import { BottomSheet, Host, RNHostView } from "@expo/ui";
import { ScrollView, Text, View } from "react-native";
import { router } from "expo-router";
import { t } from "../i18n";
import { useTheme } from "../theme";
import { IconButton, Loading, NativeButton } from "../components/primitives";
import { useDialogs } from "../components/dialogs";
import { SettingsRows } from "../components/settings-rows";
import { ProfileAvatar } from "../profile/profile-avatar";
import type { MemberActionsSession, MemberActionsState } from "./member-actions";

export function MemberActionsSheet({
  session,
  state,
}: {
  session: MemberActionsSession;
  state: MemberActionsState;
}) {
  const { presentation: p, scheme } = useTheme();
  const { confirm } = useDialogs();
  const context = state.context;
  const member = context?.member;
  const ticket = session.ticket();
  return (
    <Host
      style={{ position: "absolute" }}
      pointerEvents="none"
      colorScheme={scheme}
      seedColor={p.list.accent}
    >
      <BottomSheet
        testID="member-actions-sheet"
        isPresented={Boolean(state.selected)}
        onDismiss={() => session.close(ticket)}
        snapPoints={["half", "full"]}
        containerColor={p.list.background}
      >
        <RNHostView>
          <ScrollView
            contentInsetAdjustmentBehavior="automatic"
            contentContainerStyle={{ gap: 20, paddingVertical: 16, paddingBottom: 32 }}
          >
            <View style={{ flexDirection: "row", alignItems: "center", gap: 12 }}>
              {member ? (
                <ProfileAvatar
                  person={member}
                  name={member.name}
                  color={member.tint_hex}
                  size={40}
                />
              ) : null}
              <Text
                accessibilityRole="header"
                style={{ flex: 1, color: p.list.text, fontSize: 22, fontWeight: "600" }}
              >
                {member?.name ?? t("community.member")}
              </Text>
              <IconButton
                name={{ ios: "xmark", android: "close", web: "close" }}
                label={t("common.close")}
                onPress={() => session.close(ticket)}
              />
            </View>
            {state.phase === "loading" ? <Loading /> : null}
            {state.phase === "failed" ? (
              <View style={{ gap: 12 }}>
                <Text accessibilityRole="alert" selectable style={{ color: p.destructive }}>
                  {t("community.readFailed")}
                </Text>
                <NativeButton
                  label={t("chat.tryAgain2")}
                  onPress={() => {
                    void session.refresh(ticket);
                  }}
                />
              </View>
            ) : null}
            {state.phase === "ready" && member && context ? (
              <>
                {context.role === "owner" || context.role === "admin" ? (
                  <Text style={{ color: p.list.secondary }}>
                    {t(context.role === "owner" ? "community.owner" : "community.admin")}
                  </Text>
                ) : null}
                <SettingsRows
                  testID="member-actions"
                  rows={[
                    {
                      id: "member-open-profile",
                      title: t("community.profile"),
                      icon: { ios: "person.crop.circle", android: "person", web: "person" },
                      onPress: () => {
                        if (!session.canOpenProfile(ticket)) return;
                        session.close(ticket);
                        router.push({ pathname: "/person/[id]", params: { id: member.id } });
                      },
                    },
                  ]}
                />
                {context.can_remove ||
                context.state === "Removing" ||
                context.state === "Updating" ? (
                  <NativeButton
                    testID="member-remove"
                    destructive
                    disabled={state.busy || !context.can_remove}
                    label={t(
                      context.state === "Removing"
                        ? "community.removing"
                        : context.state === "Updating"
                          ? "community.updating"
                          : "community.remove",
                    )}
                    onPress={() => {
                      void session.remove(
                        (review) =>
                          confirm({
                            title: t("community.removeNamed", {
                              name: review.member?.name ?? t("community.member"),
                            }),
                            message: t("community.removeFrom", { title: review.space_title }),
                            confirmLabel: t("community.remove"),
                            destructive: true,
                          }),
                        ticket,
                      );
                    }}
                  />
                ) : null}
                {state.actionFailed ? (
                  <Text accessibilityRole="alert" selectable style={{ color: p.destructive }}>
                    {t("community.actionFailed")}
                  </Text>
                ) : null}
              </>
            ) : null}
          </ScrollView>
        </RNHostView>
      </BottomSheet>
    </Host>
  );
}
