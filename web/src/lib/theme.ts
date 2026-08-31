// Theme + rule constants — faithful port of `src/ui/theme.py` and
// `src/models/puzzle.py` (RULE_NAMES) / `src/ui/constraint_panel.py` (descriptions).

export const MODE_COLORS: Record<string, string> = {
  select: '#5B9BD5',
  boundary: '#E74C3C',
  block: '#7F8C8D',
  number: '#27AE60',
  symbol: '#8E44AD',
  compass: '#2980B9',
  watchtower: '#D35400',
}

export const REGION_COLORS = [
  '#4E79A7', '#F28E2B', '#59A14F', '#76B7B2', '#499894',
  '#E15759', '#B07AA1', '#FF9DA7', '#9C755F', '#BAB0AC',
  '#86BCB6', '#D37295', '#8CD17D', '#74C476', '#AF7AA1',
  '#79706E', '#D4A6C8', '#A0CBE8', '#FFBE7D', '#CAB2D6',
]

// P-number coloured circles (matches the archive viewer).
export const P_COLORS = ['#c84030', '#3060c0', '#cca020', '#30a040', '#8030c0']

// Fence diamond edges per F-value: (NW, NE, SW, SE).
export const FENCE_EDGES: Record<string, [number, number, number, number]> = {
  F0: [0, 0, 0, 0],
  F1: [0, 0, 0, 1],
  F2: [0, 1, 1, 0],
  F3: [0, 1, 1, 1],
  F4: [1, 1, 1, 1],
  F7: [1, 1, 0, 0],
}

// Light theme colors (matches the default PyQt ColorTheme).
export const colors = {
  grid_bg: '#FFFFFF',
  cell_bg_null: '#F0F0F0',
  cell_border: '#E2E8F0',
  cell_blocked_bg: '#2C3E50',
  cell_blocked_border: '#1A252F',
  cell_blocked_x: '#5D6D7E',
  boundary_edge: '#B8860B',
  boundary_highlight: '#FFD700',
  grid_line: '#E2E8F0',
  edge_constr_bg: '#FFF8E1',
  edge_constr_border: '#F59E0B',
  edge_constr_text: '#D97706',
  watchtower_bg: '#EDE9FE',
  watchtower_border: '#7C3AED',
  watchtower_text: '#7C3AED',
  symbol_text: '#DC2626',
  number_text: '#1E293B',
  compass_text: '#2563EB',
  compass_line: '#BFDBFE',
  selection_border: '#3B82F6',
  selection_vertex_fill: '#DBEAFE',
  hover_cell: '#60A5FA',
  hover_vertex: '#60A5FA',
  overlay_bg: 'rgba(255,255,255,0.92)',
  overlay_border: '#D0D5DD',
  overlay_text: '#1E293B',
  overlay_header: '#64748B',
  shape_mini_pen: '#3B82F6',
  shape_mini_fill: '#DBEAFE',
  shape_editor_active_bg: '#DBEAFE',
  shape_editor_active_border: '#3B82F6',
  shape_editor_empty_bg: '#F8FAFC',
  shape_editor_empty_border: '#CBD5E1',
  shape_editor_area_text: '#64748B',
  preview_bg: '#FFFFFF',
  preview_blocked_bg: '#2C3E50',
  preview_cell_normal: '#F0F0F0',
  preview_cell_border: '#D0D5DD',
  preview_boundary: '#B8860B',
  preview_summary_text: '#64748B',
}

/** Dark board palette — the puzzle sheet flips with the app theme. */
export const colorsDark: BoardPalette = {
  grid_bg: '#0F131B',
  cell_bg_null: '#151A24',
  cell_border: '#222A37',
  cell_blocked_bg: '#05070B',
  cell_blocked_border: '#000000',
  cell_blocked_x: '#3B4757',
  boundary_edge: '#F5C451',
  boundary_highlight: '#FFF3C4',
  grid_line: '#202836',
  edge_constr_bg: '#2A2113',
  edge_constr_border: '#F59E0B',
  edge_constr_text: '#FBBF24',
  watchtower_bg: '#241C3D',
  watchtower_border: '#A78BFA',
  watchtower_text: '#C4B5FD',
  symbol_text: '#F87171',
  number_text: '#E2E8F0',
  compass_text: '#7DD3FC',
  compass_line: '#1E3A5F',
  selection_border: '#6C7CFF',
  selection_vertex_fill: '#1E2540',
  hover_cell: '#8B9BFF',
  hover_vertex: '#8B9BFF',
  overlay_bg: 'rgba(15,19,27,0.92)',
  overlay_border: '#2A3140',
  overlay_text: '#E2E8F0',
  overlay_header: '#94A3B8',
  shape_mini_pen: '#6C7CFF',
  shape_mini_fill: '#232A4A',
  shape_editor_active_bg: '#232A4A',
  shape_editor_active_border: '#6C7CFF',
  shape_editor_empty_bg: '#151A24',
  shape_editor_empty_border: '#2A3140',
  shape_editor_area_text: '#94A3B8',
  preview_bg: '#0F131B',
  preview_blocked_bg: '#05070B',
  preview_cell_normal: '#151A24',
  preview_cell_border: '#2A3140',
  preview_boundary: '#F5C451',
  preview_summary_text: '#94A3B8',
}

export const boardPalettes = { light: colors, dark: colorsDark }

export type BoardPalette = Record<keyof typeof colors, string>
export type BoardThemeName = 'light' | 'dark'

/** sRGB channel blend — keeps region fills legible on both sheet colours. */
export function mix(a: string, b: string, t: number): string {
  const pa = hexRgb(a)
  const pb = hexRgb(b)
  const ch = (i: number) => Math.round(pa[i] + (pb[i] - pa[i]) * t)
  return `#${[ch(0), ch(1), ch(2)].map((v) => v.toString(16).padStart(2, '0')).join('')}`
}

function hexRgb(hex: string): [number, number, number] {
  const s = hex.replace('#', '')
  const full = s.length === 3 ? s.split('').map((c) => c + c).join('') : s
  return [parseInt(full.slice(0, 2), 16), parseInt(full.slice(2, 4), 16), parseInt(full.slice(4, 6), 16)]
}

/** Soft fill + saturated outline for a solved region on the given sheet. */
export function regionFill(color: string, dark: boolean): string {
  return dark ? mix(color, '#0F131B', 0.62) : mix(color, '#FFFFFF', 0.45)
}

export function regionStroke(color: string, dark: boolean): string {
  return dark ? mix(color, '#FFFFFF', 0.2) : mix(color, '#0F172A', 0.12)
}

export const RULE_NAMES: Record<string, string> = {
  shape_pool: '形状池',
  rose_window: '玫瑰窗',
  heterogeneous: '异生',
  homogeneous: '双生',
  precise: '精确',
  puzzle_piece: '拼块',
  mixed: '混合',
  area: '面积',
  same: '相同',
  range: '范围',
  fence: '围栏',
  different: '相异',
  solitary: '独居',
  block: '方块',
  non_block: '非方块',
  differentiation: '差异化',
  brick: '砖纹',
  ring: '环纹',
  inequality: '不等号',
  difference: '差值',
  watchtower: '望塔',
  compass: '罗盘',
}

export const RULE_CATEGORIES: [string, string[]][] = [
  ['形状类', ['shape_pool', 'same', 'different', 'mixed', 'block', 'non_block']],
  ['面积类', ['precise', 'range', 'area', 'differentiation']],
  ['约束边', ['heterogeneous', 'homogeneous', 'inequality', 'difference']],
  ['标记类', ['puzzle_piece', 'fence', 'solitary', 'watchtower', 'compass', 'rose_window']],
  ['结构类', ['brick', 'ring']],
]

export const RULE_DESCRIPTIONS: Record<string, string> = {
  shape_pool: '区域形状必须来自形状池',
  rose_window: 'N种符号各M个，M个区域各含全部N种',
  heterogeneous: '标记边两侧区域形状不同',
  homogeneous: '标记边两侧区域形状相同',
  precise: '所有区域面积=指定值',
  puzzle_piece: '标记格的区域形状=标记图案',
  mixed: '相邻区域形状互不相同',
  area: '标记格的区域面积=数字',
  same: '所有区域形状相同',
  range: '区域面积在[min,max]内',
  fence: '边界分布匹配标记图案',
  different: '所有区域形状互不相同',
  solitary: '每区域仅含一个符号',
  block: '所有区域为矩形',
  non_block: '所有区域非矩形',
  differentiation: '相邻区域面积不等',
  brick: '禁止四边同交于一点',
  ring: '禁止三边同交于一点',
  inequality: '不等号指向面积更小侧（> 左大 · < 右大 · ^ 上大 · v 下大）',
  difference: '边数字=两侧面积差',
  watchtower: '顶点数字=相邻区域数',
  compass: '四方向同区域格数',
}
