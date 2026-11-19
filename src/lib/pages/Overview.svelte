<script lang="ts">
  import { onMount } from "svelte";
  import { fetchAppStatus, type AppStatus } from "../api";

  let status = $state<AppStatus | null>(null);
  let error = $state<string | null>(null);

  onMount(async () => {
    try {
      status = await fetchAppStatus();
    } catch (err) {
      error = `无法读取后端状态：${String(err)}`;
    }
  });
</script>

<h1>总览</h1>

{#if error}
  <div class="error-box">{error}</div>
{:else if status === null}
  <p class="muted">正在读取后端状态…</p>
{:else}
  <div class="panel">
    <h2>运行状态</h2>
    <table class="status">
      <tbody>
        <tr>
          <th>应用版本</th>
          <td>{status.app_version}</td>
        </tr>
        <tr>
          <th>数据库</th>
          <td class={status.database_ready ? "ok" : "bad"}>
            {status.database_ready ? "就绪" : "异常"}
            （schema 版本 {status.schema_version}）
          </td>
        </tr>
        <tr>
          <th>后台任务</th>
          <td class={status.runtime_ready ? "ok" : "bad"}>
            {status.runtime_ready ? "运行中" : "未就绪"}
          </td>
        </tr>
        <tr>
          <th>配置版本</th>
          <td>{status.config_version}</td>
        </tr>
        <tr>
          <th>窗口尺寸</th>
          <td>{status.window_width} × {status.window_height}</td>
        </tr>
        <tr>
          <th>日志级别</th>
          <td>{status.log_level}</td>
        </tr>
      </tbody>
    </table>
  </div>

  <div class="panel">
    <h2>便携数据目录</h2>
    <table class="status">
      <tbody>
        <tr>
          <th>数据目录</th>
          <td>{status.data_dir}</td>
        </tr>
        <tr>
          <th>配置文件</th>
          <td>{status.config_path}</td>
        </tr>
        <tr>
          <th>数据库文件</th>
          <td>{status.database_path}</td>
        </tr>
        <tr>
          <th>WebView 数据目录</th>
          <td>{status.webview_dir}</td>
        </tr>
      </tbody>
    </table>
  </div>
{/if}
