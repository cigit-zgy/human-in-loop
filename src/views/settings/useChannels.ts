import { onBeforeUnmount, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import {
  channelHealth,
  detectCancel,
  feishuDetectPrepare,
  feishuDetectWait,
  feishuTest,
  openPath,
} from "../../lib/ipc";
import type { ChannelIssue } from "../../lib/types";
import type { SettingsCore } from "./context";

export function useChannels(core: SettingsCore) {
  const { t, locale } = useI18n();
  const { config, activeTab, persist } = core;
  const channelIssues = ref<ChannelIssue[]>([]);
  let channelHealthTimer: number | undefined;

  async function refreshChannelHealth() {
    try {
      channelIssues.value = (await channelHealth()).filter((issue) =>
        issue.channel === "feishu" || issue.channel === "imessage",
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

  function channelIssueText(id: "feishu" | "imessage"): string | null {
    const issue = channelIssues.value.find((candidate) => candidate.channel === id);
    return issue
      ? t("settings.channels.issueBanner", { time: issueAgo(issue.atMs), msg: issue.message })
      : null;
  }

  function openChannelGuide(_id: "feishu") {
    const suffix = String(locale.value).startsWith("zh") ? ".md" : ".en.md";
    void openPath(
      `https://github.com/Naituw/AskHuman/blob/77e2e576347f94ef203bc2426b73a18749cb4e92/docs/wiki/feishu-setup${suffix}`,
    );
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

  const detectCancelled = ref(false);
  const feishuTesting = ref(false);
  const feishuDetecting = ref(false);
  const feishuDetectCode = ref<string | null>(null);
  const feishuMessage = ref<string | null>(null);
  const feishuError = ref(false);

  async function cancelDetect() {
    detectCancelled.value = true;
    try {
      await detectCancel();
    } catch {
      // The bounded backend wait still times out and cleans up.
    }
  }

  async function runFeishuTest() {
    if (!config.value) return;
    feishuTesting.value = true;
    feishuMessage.value = null;
    const feishu = config.value.channels.feishu;
    try {
      feishuMessage.value = await feishuTest({
        appId: feishu.appId,
        appSecret: feishu.appSecret,
        openId: feishu.openId,
        baseUrl: feishu.baseUrl,
      });
      feishuError.value = false;
    } catch (error) {
      feishuMessage.value = String(error);
      feishuError.value = true;
    } finally {
      feishuTesting.value = false;
    }
  }

  async function runFeishuDetect() {
    if (!config.value) return;
    const feishu = config.value.channels.feishu;
    feishuDetecting.value = true;
    detectCancelled.value = false;
    feishuMessage.value = null;
    feishuDetectCode.value = null;
    try {
      const code = await feishuDetectPrepare({
        appId: feishu.appId,
        appSecret: feishu.appSecret,
        baseUrl: feishu.baseUrl,
      });
      feishuDetectCode.value = code;
      feishu.openId = await feishuDetectWait({
        appId: feishu.appId,
        appSecret: feishu.appSecret,
        baseUrl: feishu.baseUrl,
        code,
      });
      await persist();
      feishuError.value = false;
      feishuMessage.value = t("settings.channels.feishuDetected", { openId: feishu.openId });
    } catch (error) {
      if (!detectCancelled.value) {
        feishuMessage.value = String(error);
        feishuError.value = true;
      }
    } finally {
      feishuDetecting.value = false;
      feishuDetectCode.value = null;
      detectCancelled.value = false;
    }
  }

  return {
    channelIssueText,
    openChannelGuide,
    cancelDetect,
    feishuTesting,
    feishuDetecting,
    feishuDetectCode,
    feishuMessage,
    feishuError,
    runFeishuTest,
    runFeishuDetect,
  };
}
