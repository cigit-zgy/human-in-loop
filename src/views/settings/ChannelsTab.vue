<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { useSettingsContext } from "./context";

const { t } = useI18n();
const ctx = useSettingsContext();
const {
  persist,
  channelIssueText,
} = ctx;
const config = computed(() => ctx.config.value!);
</script>

<template>
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
