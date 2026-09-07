<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { useSettingsContext } from "./context";

const { t } = useI18n();
const ctx = useSettingsContext();
const {
  persist,
  secretsPresent,
  SECRET_PLACEHOLDER,
  clearSecret,
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
} = ctx;
const config = computed(() => ctx.config.value!);
</script>

<template>
  <div class="card channel-card channel-card-feishu">
    <div class="row">
      <p class="card-title">{{ t("settings.channels.feishuTitle") }}</p>
      <a class="link guide-link" href="#" @click.prevent="openChannelGuide('feishu')">
        {{ t("settings.channels.setupGuide") }} ↗
      </a>
      <span class="spacer"></span>
      <label class="switch">
        <input type="checkbox" v-model="config.channels.feishu.enabled" @change="persist" />
        <span class="track"></span>
      </label>
    </div>
    <p v-if="channelIssueText('feishu')" class="card-desc warn">
      {{ channelIssueText("feishu") }}
    </p>

    <template v-if="config.channels.feishu.enabled">
      <hr class="divider" />
      <div class="field">
        <label>{{ t("settings.channels.appId") }}</label>
        <input class="input" v-model="config.channels.feishu.appId" @change="persist" />
      </div>
      <div class="field">
        <label>{{ t("settings.channels.appSecret") }}</label>
        <div class="row">
          <input
            class="input"
            style="flex: 1"
            type="password"
            :placeholder="secretsPresent.feishuSecret ? SECRET_PLACEHOLDER : ''"
            v-model="config.channels.feishu.appSecret"
            @change="persist"
          />
          <button
            v-if="secretsPresent.feishuSecret"
            class="btn"
            type="button"
            @click="clearSecret('feishu')"
          >
            {{ t("settings.channels.clearSecret") }}
          </button>
        </div>
      </div>
      <div class="field">
        <label>{{ t("settings.channels.openId") }}</label>
        <div class="row">
          <input class="input" style="flex: 1" v-model="config.channels.feishu.openId" @change="persist" />
          <button class="btn" type="button" :disabled="feishuDetecting" @click="runFeishuDetect">
            {{ feishuDetecting ? t("settings.channels.detecting") : t("settings.channels.autoDetect") }}
          </button>
          <button v-if="feishuDetecting" class="btn" type="button" @click="cancelDetect">
            {{ t("settings.channels.detectCancel") }}
          </button>
        </div>
      </div>
      <i18n-t
        v-if="feishuDetectCode"
        keypath="settings.channels.feishuDetectHint"
        tag="p"
        class="result ok"
      >
        <template #code><b>{{ feishuDetectCode }}</b></template>
      </i18n-t>
      <div class="field">
        <label>{{ t("settings.channels.feishuBaseUrl") }}</label>
        <input
          class="input"
          v-model="config.channels.feishu.baseUrl"
          :placeholder="t('settings.channels.feishuBaseUrlPlaceholder')"
          @change="persist"
        />
      </div>
      <button class="btn" type="button" :disabled="feishuTesting" @click="runFeishuTest">
        {{ feishuTesting ? t("settings.channels.testing") : t("settings.channels.testConnection") }}
      </button>
      <p v-if="feishuMessage" class="result" :class="feishuError ? 'err' : 'ok'">
        {{ feishuMessage }}
      </p>
    </template>
  </div>

  <div class="card channel-card channel-card-imessage">
    <div class="row">
      <p class="card-title">{{ t("settings.channels.imessageTitle") }}</p>
      <span class="spacer"></span>
      <label class="switch">
        <input type="checkbox" v-model="config.channels.imessage.enabled" @change="persist" />
        <span class="track"></span>
      </label>
    </div>
    <p class="card-desc">{{ t("settings.channels.imessageDescription") }}</p>
    <p v-if="channelIssueText('imessage')" class="card-desc warn">
      {{ channelIssueText("imessage") }}
    </p>

    <template v-if="config.channels.imessage.enabled">
      <hr class="divider" />
      <div class="field">
        <label>{{ t("settings.channels.imessageRecipient") }}</label>
        <input
          class="input"
          v-model="config.channels.imessage.recipient"
          :placeholder="t('settings.channels.imessageRecipientPlaceholder')"
          @change="persist"
        />
      </div>
      <div class="field">
        <label>{{ t("settings.channels.imessageIdentityMode") }}</label>
        <select class="input" v-model="config.channels.imessage.identityMode" @change="persist">
          <option value="distinct_peer">{{ t("settings.channels.imessageDistinctPeer") }}</option>
          <option value="same_account">{{ t("settings.channels.imessageSameAccount") }}</option>
        </select>
      </div>
      <div v-if="config.channels.imessage.chatId !== null" class="field">
        <label>{{ t("settings.channels.imessageChatId") }}</label>
        <input class="input" type="number" v-model.number="config.channels.imessage.chatId" readonly />
      </div>
      <div v-if="config.channels.imessage.chatGuid" class="field">
        <label>{{ t("settings.channels.imessageChatGuid") }}</label>
        <input class="input" v-model="config.channels.imessage.chatGuid" readonly />
      </div>
      <p class="card-desc">{{ t("settings.channels.imessageSetupHint") }}</p>
    </template>
  </div>
</template>
