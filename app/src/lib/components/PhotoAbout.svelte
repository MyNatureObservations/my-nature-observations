<script lang="ts">import * as m from "$lib/paraglide/messages.js";let {locale}:{locale:"sv"|"en"}=$props();</script>
<details class="photo-about"><summary>{m.photo_about({}, {locale})}</summary><p>{m.bird_photo_original_policy({}, {locale})}</p></details>
<style>.photo-about{font:11px/1.5 system-ui,sans-serif;color:var(--text-muted,#686d63);margin-top:6px}.photo-about summary{cursor:pointer;min-height:28px;align-content:center}.photo-about p{margin:4px 0;max-width:54ch}summary:focus-visible{outline:2px solid #788e7b;outline-offset:3px}</style>
