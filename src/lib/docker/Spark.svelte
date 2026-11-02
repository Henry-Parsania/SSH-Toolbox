<script lang="ts">
  // A small line graph of recent readings, oldest on the left.
  interface Props {
    values: number[];
    /** The top of the graph. By default the highest reading, so the line always fills the height. */
    max?: number;
    width?: number;
    height?: number;
    label?: string;
  }

  let { values, max, width = 64, height = 16, label = "" }: Props = $props();

  const top = $derived(Math.max(max ?? 0, ...values, 0.0001));
  const points = $derived(
    values
      .map((v, i) => {
        const x = values.length < 2 ? width : (i / (values.length - 1)) * width;
        const y = height - 1 - (Math.min(v, top) / top) * (height - 2);
        return `${x.toFixed(1)},${y.toFixed(1)}`;
      })
      .join(" "),
  );
</script>

<svg class="spark" {width} {height} viewBox="0 0 {width} {height}" role="img" aria-label={label}>
  {#if values.length >= 2}
    <polygon points="0,{height} {points} {width},{height}" class="fill" />
    <polyline {points} class="line" />
  {/if}
</svg>

<style>
  .spark {
    flex: none;
    vertical-align: middle;
  }
  .line {
    fill: none;
    stroke: var(--accent);
    stroke-width: 1.4;
    stroke-linejoin: round;
  }
  .fill {
    fill: var(--accent);
    opacity: 0.15;
  }
</style>
