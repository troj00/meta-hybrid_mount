<script lang="ts">
  import './Skeleton.css';

  interface Props {
    width?: string;
    height?: string;
    style?: string;
    borderRadius?: string;
  }

  let { width = '100%', height = '20px', style = '', borderRadius = '4px' }: Props = $props();
</script>

<div class="skeleton" style="width: {width}; height: {height}; border-radius: {borderRadius}; {style}"></div>