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
