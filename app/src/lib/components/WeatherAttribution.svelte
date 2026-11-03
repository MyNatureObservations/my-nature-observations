<script lang="ts">
  import type { WeatherPart } from "$lib/weather";
  import { weatherLabels } from "$lib/weather-labels";
  let { part, locale }: { part: WeatherPart | null | undefined; locale: "sv" | "en" } = $props();
</script>

{#if part?.source?.provider === "open-meteo"}
  <span class="attribution">{weatherLabels.history_source({}, { locale })}:
    <a href="https://open-meteo.com/" target="_blank" rel="noopener noreferrer">Open-Meteo</a>
    · <a href="https://creativecommons.org/licenses/by/4.0/" target="_blank" rel="noopener noreferrer">CC BY 4.0</a>
  </span>
{/if}

<style>
  .attribution { display: block; color: var(--text-muted, #686d63); font: 11px/1.4 system-ui, sans-serif; overflow-wrap: anywhere; }
  a { color: inherit; }
  a:focus-visible { outline: 2px solid #435b47; outline-offset: 3px; }
</style>
