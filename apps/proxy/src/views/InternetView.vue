<script setup lang="ts">
import FriendsCard from "@ttv-tts/ui/components/FriendsCard.vue";
import { friends } from "../api";
import { run, store, toast } from "../store";
</script>

<template>
  <FriendsCard :api="friends" :busy="store.busy" :run="run" :toast="toast" />
</template>
