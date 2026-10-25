<script setup lang="ts">
import { Circle, Path, Svg } from '@inkoxide/vue';
</script>

<template>
  <Svg :width="28" :height="28" view-box="0 0 32 32">
    <Circle :cx="16" :cy="16" :r="16" fill="#4f46e5" />
    <Path
      d="M10 21 L16 9 L22 21 M12.5 16.5 H19.5"
      fill="none"
      stroke="white"
      :stroke-width="2.4"
      stroke-linecap="round"
    />
  </Svg>
</template>
