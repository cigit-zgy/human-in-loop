import { onBeforeUnmount, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { channelHealth } from "../../lib/ipc";
import type { ChannelIssue } from "../../lib/types";
import type { SettingsCore } from "./context";

export function useChannels(core: SettingsCore) {
  const { t } = useI18n();
  const { activeTab } = core;
  const channelIssues = ref<ChannelIssue[]>([]);
  let channelHealthTimer: number | undefined;

  async function refreshChannelHealth() {
    try {
      channelIssues.value = (await channelHealth()).filter(
        (issue) => issue.channel === "imessage",
      );
    } catch {
      channelIssues.value = [];
    }
  }

  function issueAgo(atMs: number): string {
    const diff = Math.max(0, Math.floor((Date.now() - atMs) / 1000));
    if (diff < 60) return t("agents.time.justNow");
    const min = Math.floor(diff / 60);
    if (min < 60) return t("agents.time.minutesAgo", { n: min });
    const hr = Math.floor(min / 60);
    if (hr < 24) return t("agents.time.hoursAgo", { n: hr });
    return t("agents.time.daysAgo", { n: Math.floor(hr / 24) });
  }

  function channelIssueText(id: "imessage"): string | null {
    const issue = channelIssues.value.find((candidate) => candidate.channel === id);
    return issue
      ? t("settings.channels.issueBanner", { time: issueAgo(issue.atMs), msg: issue.message })
      : null;
  }

  watch(
    activeTab,
    (tab) => {
      if (channelHealthTimer) window.clearInterval(channelHealthTimer);
      channelHealthTimer = undefined;
      if (tab === "channel") {
        void refreshChannelHealth();
        channelHealthTimer = window.setInterval(() => void refreshChannelHealth(), 10_000);
      }
    },
    { immediate: true },
  );
  onBeforeUnmount(() => {
    if (channelHealthTimer) window.clearInterval(channelHealthTimer);
  });

  return {
    channelIssueText,
  };
}
