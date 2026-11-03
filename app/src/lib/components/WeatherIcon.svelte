<script lang="ts">
  import type { WeatherIconName } from "$lib/weather";
  let { name, size = 20 }: { name: WeatherIconName; size?: number } = $props();
</script>

<!-- Local stroke icons follow MNO's existing inline SVG style; no icon dependency. -->
<svg aria-hidden="true" focusable="false" data-weather-icon={name} width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
  {#if name === 'sun'}
    <circle cx="12" cy="12" r="4" /><path d="M12 2v2m0 16v2M2 12h2m16 0h2M5 5l1.4 1.4m11.2 11.2L19 19M5 19l1.4-1.4M17.6 6.4 19 5" />
  {:else if name === 'sun-cloud'}
    <circle cx="8" cy="8" r="3" /><path d="M8 1v2M1 8h2m.1-4.9 1.4 1.4M13 3l-1.4 1.4M7 20h11a4 4 0 0 0 .3-8A5.5 5.5 0 0 0 8 12a4 4 0 0 0-1 8Z" />
  {:else if name === 'cloud' || name === 'rain' || name === 'thunder'}
    <path d="M6 16h12a4 4 0 0 0 .2-8A6 6 0 0 0 7 7a4.5 4.5 0 0 0-1 9Z" />
    {#if name === 'rain'}<path d="m7 19-1 3m7-3-1 3m7-3-1 3" />{/if}
    {#if name === 'thunder'}<path d="m13 16-3 4h4l-2 3" />{/if}
  {:else if name === 'snow'}
    <path d="M12 2v20M3.3 7l17.4 10M3.3 17 20.7 7M9 4l3 3 3-3M9 20l3-3 3 3M4 10l4-1-1-4m10 14-1-4 4-1M4 14l4 1-1 4M17 5l-1 4 4 1" />
  {:else if name === 'fog'}
    <path d="M6 11h12a3.5 3.5 0 0 0 .1-7A5.5 5.5 0 0 0 8 4a3.5 3.5 0 0 0-2 7ZM3 15h15m-12 4h15M2 19h1m18-4h1" />
  {:else if name === 'temperature'}
    <path d="M9 14V5a3 3 0 0 1 6 0v9a5 5 0 1 1-6 0Z" /><path d="M12 8v10" /><circle cx="12" cy="18" r="1.5" />
  {:else if name === 'wind'}
    <path d="M3 7h12a3 3 0 1 0-3-3M2 12h17a2 2 0 1 0-2-2M4 17h10a3 3 0 1 1-3 3" />
  {:else if name === 'drop' || name === 'humidity'}
    <path d="M12 2S5 10 5 15a7 7 0 0 0 14 0c0-5-7-13-7-13Z" />
    {#if name === 'humidity'}<path d="m9 17 6-6" /><circle cx="9" cy="12" r=".6" /><circle cx="15" cy="17" r=".6" />{/if}
  {:else if name === 'sunrise' || name === 'sunset'}
    <path d="M2 18h20M5 14a7 7 0 0 1 14 0M2 13h1m18 0h1M5 6l1.5 1.5M19 6l-1.5 1.5M12 3v10" />
    {#if name === 'sunrise'}<path d="m9 6 3-3 3 3" />{:else}<path d="m9 10 3 3 3-3" />{/if}
  {:else}
    <circle cx="12" cy="12" r="8" /><path d="M9.5 9a2.5 2.5 0 0 1 5 .5c0 2-2.5 2-2.5 4m0 3h.01" />
  {/if}
</svg>

<style>svg { flex-shrink: 0; vertical-align: middle; }</style>
