// 设置页共享上下文：SettingsView 在 setup 里调用 createSettingsContext() 组装各域
// 组合式函数（provide），各 tab 子组件用 useSettingsContext() 注入取用。类型由
// createSettingsContext 的返回值推导，无须手工维护巨型 interface。
import { inject, provide, ref, type InjectionKey, type Ref } from "vue";
import { saveSettings } from "../../lib/ipc";
import type { AppConfig } from "../../lib/types";
import { useGeneralSettings } from "./useGeneralSettings";
import { useAboutUpdates } from "./useAboutUpdates";
import { useIntegration } from "./useIntegration";
import { useAgentTasks } from "./useAgentTasks";
import { useChannels } from "./useChannels";
import { useSettingsSearch } from "./useSearch";
import { isMac, isWindows, supportsAgentTasks } from "../../lib/platform";

export { isMac, isWindows, supportsAgentTasks };

export type Tab = "general" | "integration" | "channel" | "advanced" | "experimental";
export const TABS: readonly Tab[] = ["general", "integration", "channel", "advanced", "experimental"];

function parseInitialTab(): Tab {
  // 初始定位 tab 经窗口 URL 传入（如托盘「渠道异常」行 → ?tab=channel），无监听时序问题。
  // 支持 `tab#elementId` 锚点后缀（跨窗口定位，spec gui-agent-task-launch G5）：此处只取 tab 段，
  // 锚点滚动由 SettingsView 挂载完成后处理。
  const tab = (new URLSearchParams(window.location.search).get("tab") ?? "").split("#")[0];
  return TABS.includes(tab as Tab) ? (tab as Tab) : "general";
}

export interface SettingsCore {
  config: Ref<AppConfig | null>;
  activeTab: Ref<Tab>;
  persist: () => Promise<void>;
}

function createCore() {
  const config = ref<AppConfig | null>(null);
  const activeTab = ref<Tab>(parseInitialTab());

  async function persist() {
    if (!config.value) return;
    await saveSettings(config.value);
  }

  return {
    config,
    activeTab,
    persist,
  };
}

export function createSettingsContext() {
  const core = createCore();
  const general = useGeneralSettings(core);
  const updates = useAboutUpdates();
  const tasks = useAgentTasks(core);
  const integration = useIntegration(core, tasks);
  const channels = useChannels(core);
  const search = useSettingsSearch({
    config: core.config,
    activeTab: core.activeTab,
  });

  const ctx = {
    isMac,
    isWindows,
    supportsAgentTasks,
    ...core,
    ...general,
    ...updates,
    ...integration,
    ...tasks,
    ...channels,
    ...search,
  };
  provide(SettingsCtxKey, ctx);
  return ctx;
}

export type SettingsContext = ReturnType<typeof createSettingsContext>;

const SettingsCtxKey: InjectionKey<SettingsContext> = Symbol("settings-ctx");

export function useSettingsContext(): SettingsContext {
  const ctx = inject(SettingsCtxKey);
  if (!ctx) throw new Error("settings context not provided");
  return ctx;
}
