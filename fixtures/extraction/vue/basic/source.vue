<template>
  <section class="worker">
    <HeaderBar />
    <h1>{{ title }}</h1>
  </section>
</template>

<script setup lang="ts">
const title = format("Worker");

function format(value: string): string {
  return value.trim();
}
</script>

<style scoped>
.worker {
  color: #0f766e;
}
</style>
