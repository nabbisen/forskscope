<script lang="ts">
  import Header from './includes/Header.svelte'
  import Toast from '../../components/common/Toast.svelte'
  import { onMount } from 'svelte'
  import { _setPathSeparator } from '../../stores/file.svelte'
  import {
    activeDiffFontFamily,
    activeTheme,
    activeUiFontFamily,
    diffFontSize,
    uiFontSizeScale,
  } from '../../stores/settings/theme.svelte'
  import AppHeader from './main/AppHeader.svelte'
  import AppBody from './main/AppBody.svelte'
  import AppFooter from './main/AppFooter.svelte'
  import './style.scss'

  onMount(async () => {
    await _setPathSeparator()
  })
</script>

<div
  id="app"
  class={`${$activeTheme} ${$activeUiFontFamily} ${$activeDiffFontFamily}`}
  style={`--diff-font-size: ${$diffFontSize}px; --ui-font-size: ${$uiFontSizeScale * $diffFontSize}px`}
>
  <Toast />
  <header>
    <Header />
  </header>
  <main>
    <div class="app-header">
      <AppHeader />
    </div>
    <div class="app-body">
      <AppBody />
    </div>
    <div class="app-footer">
      <AppFooter />
    </div>
  </main>
</div>
