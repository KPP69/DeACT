<script lang="ts">
  import Overview from "./lib/pages/Overview.svelte";
  import Trading from "./lib/pages/Trading.svelte";
  import CopyConfig from "./lib/pages/CopyConfig.svelte";
  import Signals from "./lib/pages/Signals.svelte";
  import Accounts from "./lib/pages/Accounts.svelte";
  import Settings from "./lib/pages/Settings.svelte";

  const pages = [
    { id: "overview", label: "总览" },
    { id: "trading", label: "交易中心" },
    { id: "copy", label: "跟单配置" },
    { id: "signals", label: "信号中心" },
    { id: "accounts", label: "账户配置" },
    { id: "settings", label: "系统设置" },
  ] as const;

  type PageId = (typeof pages)[number]["id"];

  let current = $state<PageId>("overview");
</script>

<div class="layout">
  <aside class="sidebar">
    <div class="brand">DeACT</div>
    <nav class="nav">
      {#each pages as page}
        <button
          type="button"
          class:active={current === page.id}
          onclick={() => (current = page.id)}
        >
          {page.label}
        </button>
      {/each}
    </nav>
  </aside>

  <main class="content">
    {#if current === "overview"}
      <Overview />
    {:else if current === "trading"}
      <Trading />
    {:else if current === "copy"}
      <CopyConfig />
    {:else if current === "signals"}
      <Signals />
    {:else if current === "accounts"}
      <Accounts />
    {:else if current === "settings"}
      <Settings />
    {/if}
  </main>
</div>
