<script lang="ts">
 import BirdModuleSetting from '../../src/lib/components/BirdModuleSetting.svelte';
 import {birdModule} from '../../src/lib/bird-module.svelte';
</script>
<BirdModuleSetting />
<p id="revision">{birdModule.revision}</p>
