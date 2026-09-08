import { createApp } from "vue";
import { listen } from "@tauri-apps/api/event";
import App from "./App.vue";
import "./styles/tokens.css";
import "./styles/base.css";
import "./styles/controls.css";
import { i18n, applyLanguage } from "./i18n";
import { mark as perfMarkFe } from "./lib/perf";
import { applyWindowMaterial } from "./lib/theme";
import type { WindowEffect } from "./lib/types";

function isWindowEffect(value: string | null): value is WindowEffect {
  return value === "glass" || value === "blur" || value === "solid";
}

function bootstrap() {
  perfMarkFe("fe.bootstrap");
  const initialEffect = new URLSearchParams(location.search).get("effect");
  if (
    document.documentElement.classList.contains("macos") &&
    isWindowEffect(initialEffect)
  ) {
    applyWindowMaterial(initialEffect);
  }
  void listen<WindowEffect>("window-effect-changed", (event) => {
    applyWindowMaterial(event.payload);
  });
  // 立即挂载，不阻塞首帧：先按系统语言（auto）兜底，精确语言由各视图从自己的 init 命令
  // （弹窗走 popup_init，零钥匙串）拿到后再 applyLanguage 校正。极少数情况下 init 返回前
  // 文本短暂为系统语言，肉眼基本无感。
  applyLanguage("auto");
  createApp(App).use(i18n).mount("#app");
  perfMarkFe("fe.mounted");
}

bootstrap();
