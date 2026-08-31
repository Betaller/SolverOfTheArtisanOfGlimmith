<script setup lang="ts">
import AppModal from './AppModal.vue'

defineEmits<{ (e: 'close'): void }>()

const GROUPS: { title: string; items: [string[], string][] }[] = [
  {
    title: '通用',
    items: [
      [['Ctrl', 'N'], '新建盘面'],
      [['Ctrl', 'Z'], '撤销'],
      [['Ctrl', 'Shift', 'Z'], '重做'],
      [['Ctrl', 'R'], '重置盘面'],
      [['F5'], '求解'],
      [['?'], '打开/关闭本帮助'],
    ],
  },
  {
    title: '工具切换',
    items: [
      [['V'], '选择'],
      [['B'], '边框绘制'],
      [['X'], '障碍格'],
      [['N'], '数字标注'],
      [['S'], '符号标注'],
      [['C'], '罗盘标注'],
      [['W'], '望塔标注'],
    ],
  },
  {
    title: '盘面操作',
    items: [
      [['方向键'], '移动选中格/顶点'],
      [['0' ,'9'], '数字模式下直接输入线索'],
      [['Delete'], '清除选中格内容'],
      [['Esc'], '取消选中'],
      [['右键'], '打开上下文菜单'],
      [['滚轮'], '缩放盘面'],
      [['+', '−'], '放大 / 缩小'],
      [['F'], '适应窗口'],
    ],
  },
]
</script>

<template>
  <AppModal title="快捷键" subtitle="在盘面获得焦点时按键生效" @close="$emit('close')">
    <template #icon>
      <span class="prop-hero__icon"><span style="display: grid; place-items: center; color: var(--brand)">⌘</span></span>
    </template>

    <div v-for="g in GROUPS" :key="g.title" class="shortcut-group">
      <div class="shortcut-group__title">{{ g.title }}</div>
      <div class="shortcut-grid">
        <div v-for="([keys, label], i) in g.items" :key="i" class="shortcut-row">
          <span>{{ label }}</span>
          <span class="shortcut-row__keys">
            <kbd v-for="k in keys" :key="k" class="kbd">{{ k }}</kbd>
          </span>
        </div>
      </div>
    </div>

    <template #footer>
      <button class="btn btn--primary" @click="$emit('close')">知道了</button>
    </template>
  </AppModal>
</template>
