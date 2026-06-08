# Draw.io XML Best Practices

## General Rules

### ID Management
- Use meaningful IDs: `start`, `process-1`, `decision-stock`, `edge-payment`
- All IDs must be globally unique within the file
- Recommended format: `{type}-{name}` or `{type}-{number}`
- IDs `0` and `1` are reserved for root mxCells

### Style Essentials
- Always include `whiteSpace=wrap;html=1;` so text wraps properly
- Always set `fontSize=14;` or larger for readability
- Use `fontStyle=1` for bold headers, `fontStyle=0` for normal text
- Use `strokeWidth=2;` for important borders
- Edges should use `edgeStyle=orthogonalEdgeStyle;rounded=1;` for clean right-angle routing
- Add `exitX`, `exitY`, `entryX`, `entryY` on edges for precise anchor points

### Text and Line Breaks
- For multi-line labels, use `&#xa;` inside `value`, for example `value="Scene&#xa;Layer"`
- Never use literal `\n` inside `value`; draw.io treats it as visible text rather than a rendered line break
- Apply the same rule to node labels, layer labels, edge labels, and swimlane/table text when a forced line break is needed

### Alignment and Space Usage
- Sibling items in the same row or column should use equal widths, equal heights, and equal gaps unless the content explicitly requires otherwise
- Outer padding should be close to the internal gap size; avoid one oversized blank gutter on the left, right, top, or bottom
- Containers should be content-driven: fit the actual number of rows and columns plus consistent padding, instead of leaving large unused blank blocks
- If the last row is not full, center it inside the container rather than leaving a large empty tail on one side
- For sidebars and cross-cutting panels, either:
  - size the panel to its content plus balanced top/bottom padding, or
  - if the panel must span the full stack height, distribute items so the top gap, inter-item gaps, and bottom gap are visually balanced

### Common Mistakes to Avoid

1. **Duplicate IDs** - every mxCell must have a unique id
2. **Wrong parent** - standalone nodes use `parent="1"`, container children use the container ID
3. **Missing `as="geometry"`** - every mxGeometry must have this attribute
4. **Overlapping nodes** - always calculate and verify bounding boxes
5. **Missing `whiteSpace=wrap;html=1;`** - text will overflow without this
6. **Edges referencing non-existent IDs** - double-check source/target
7. **Coordinates not grid-aligned** - use multiples of 10
8. **Too small font** - minimum fontSize=12, prefer 14
9. **No edge style** - without `edgeStyle=orthogonalEdgeStyle`, edges route randomly
10. **Page too small** - if content exceeds 1200x900, increase pageWidth/pageHeight
11. **Literal `\n` in `value`** - use `&#xa;` for real line breaks
12. **Unbalanced whitespace** - reduce one-sided blank areas by resizing items, gaps, or the containing frame
13. **Loose packing** - do not leave containers half-empty when a tighter, symmetric layout is possible

## Architecture Diagram Templates (Layered Block Style)

**This is the preferred style for architecture diagrams.** It uses a structured block layout
with no connecting arrows, organized into horizontal layers with a left label column and an
optional right-side cross-cutting concerns sidebar.

### Visual Structure

```
+------------------------------------------------------------------+
| [Background: gray #f5f5f5, no stroke]                            |
|                                                                   |
| [Layer    | [Layer Container (blue, opacity=60)         ] [Side  ]|
|  Label    |   [SubGroup A header]    [SubGroup B header] | panel ]|
|  (blue,   |     [item] [item]          [item] [item]    | (red, ]|
|  bold)]   |     [item] [item]          [item] [item]    | dashed|
|           |                                              | opac  ]|
| [Layer    | [Layer Container (blue, opacity=60)         ] | 30)  ]|
|  Label]   |   [SubGroup C]           [SubGroup D]       |       ]|
|           |     [item] [item]          [item] [item]    |       ]|
+------------------------------------------------------------------+
```

### Key Design Principles

1. **Background rectangle** - Gray `#f5f5f5` with `strokeColor=none` covering entire diagram area
2. **Left label column** - Bold blue cells (width=100) naming each layer (e.g., "Scene Layer", "Application Layer")
3. **Layer containers** - Blue `#dae8fc` with `opacity=60`, sits to the right of the label column
4. **Sub-groups within layers** - Blue `#dae8fc` containers with `verticalAlign=top;spacingTop=8;` for header text
5. **Leaf items** - White `#ffffff` with gray border `#999999`, compact size (90-140px wide, 35-60px tall)
6. **Cross-cutting sidebar (optional)** - Vertical panel on the right (e.g., red `#f8cecc` with `opacity=30;dashed=1;`) for cross-cutting concerns like security, monitoring
7. **No edges/arrows** - Pure block diagram; hierarchy is expressed through spatial nesting
8. **Compact packing** - Items tightly arranged in grid within sub-groups, minimal wasted space
9. **Balanced fill** - Right sidebar, sub-groups, and layer containers should feel full and evenly distributed, not sparse

### Layout Constants

```
MARGIN = 40                    // outer margin from canvas edge
LABEL_X = MARGIN + 20         // layer label x (60)
LABEL_W = 100                  // layer label width
CONTAINER_X = LABEL_X + LABEL_W + 20   // layer container x (180)
CONTAINER_W = 790              // layer container width
SIDEBAR_X = CONTAINER_X + CONTAINER_W + 20  // sidebar x (990)
SIDEBAR_W = 110                // sidebar width
LAYER_GAP = 20                 // vertical gap between layers
SUBGROUP_PAD = 20              // padding inside layer container to sub-groups
SUBGROUP_PAD_Y = 20            // vertical padding inside containers
SUBGROUP_HEADER_H = 40         // reserved header space inside subgroup
ITEM_GAP_H = 10               // horizontal gap between leaf items
ITEM_GAP_V = 10               // vertical gap between leaf item rows
ITEM_H = 35                   // standard leaf item height
SIDEBAR_PAD_X = 10            // sidebar horizontal inner padding
SIDEBAR_PAD_Y = 20            // sidebar vertical inner padding
SIDEBAR_TITLE_H = 30          // sidebar title height
SIDEBAR_TITLE_GAP = 10        // gap below sidebar title
PAGE_W = SIDEBAR_X + SIDEBAR_W + MARGIN   // total page width (~1160)
```

### Background Rectangle
```xml
<mxCell id="background" value=""
        style="rounded=0;whiteSpace=wrap;html=1;fillColor=#f5f5f5;strokeColor=none;"
        vertex="1" parent="1">
  <mxGeometry x="40" y="40" width="1080" height="700" as="geometry" />
</mxCell>
```

### Layer Label (left column)
```xml
<mxCell id="layer-scenario" value="Scene&#xa;Layer"
        style="rounded=1;whiteSpace=wrap;html=1;fillColor=#dae8fc;strokeColor=#6c8ebf;
               fontSize=18;fontStyle=1;verticalAlign=middle;align=center;"
        vertex="1" parent="1">
  <mxGeometry x="60" y="60" width="100" height="110" as="geometry" />
</mxCell>
```

Note: layer label height matches its container height.

### Layer Container (semi-transparent)
```xml
<mxCell id="scenario-container" value=""
        style="rounded=1;whiteSpace=wrap;html=1;fillColor=#dae8fc;strokeColor=#6c8ebf;opacity=60;"
        vertex="1" parent="1">
  <mxGeometry x="180" y="60" width="790" height="110" as="geometry" />
</mxCell>
```

### Leaf Item (white with gray border)
```xml
<mxCell id="scenario-office" value="Smart Office"
        style="rounded=1;whiteSpace=wrap;html=1;fillColor=#ffffff;strokeColor=#999999;fontSize=14;"
        vertex="1" parent="1">
  <mxGeometry x="200" y="85" width="140" height="60" as="geometry" />
</mxCell>
```

### Sub-Group Container (within a layer)
```xml
<mxCell id="app-open" value="Open Applications"
        style="rounded=1;whiteSpace=wrap;html=1;fillColor=#dae8fc;strokeColor=#6c8ebf;
               fontSize=16;verticalAlign=top;align=center;spacingTop=8;"
        vertex="1" parent="1">
  <mxGeometry x="200" y="210" width="330" height="130" as="geometry" />
</mxCell>
```

Items inside the sub-group:
```xml
<mxCell id="app-workspace" value="Workspace"
        style="rounded=1;whiteSpace=wrap;html=1;fillColor=#ffffff;strokeColor=#999999;fontSize=14;"
        vertex="1" parent="1">
  <mxGeometry x="212" y="250" width="95" height="35" as="geometry" />
</mxCell>
<mxCell id="app-management" value="Management"
        style="rounded=1;whiteSpace=wrap;html=1;fillColor=#ffffff;strokeColor=#999999;fontSize=14;"
        vertex="1" parent="1">
  <mxGeometry x="317" y="250" width="95" height="35" as="geometry" />
</mxCell>
```

### Cross-Cutting Sidebar (optional, e.g., Security)
```xml
<!-- Sidebar frame (semi-transparent, dashed) -->
<mxCell id="security-frame" value=""
        style="rounded=1;whiteSpace=wrap;html=1;fillColor=#f8cecc;strokeColor=#b85450;
               fontSize=16;verticalAlign=middle;align=center;opacity=30;dashed=1;"
        vertex="1" parent="1">
  <mxGeometry x="990" y="60" width="110" height="660" as="geometry" />
</mxCell>

<!-- Sidebar title -->
<mxCell id="security-title" value="Security"
        style="text;html=1;strokeColor=none;fillColor=none;align=center;verticalAlign=middle;
               whiteSpace=wrap;rounded=0;fontSize=16;fontStyle=1;fontColor=#b85450"
        vertex="1" parent="1">
  <mxGeometry x="1015" y="70" width="60" height="30" as="geometry" />
</mxCell>

<!-- Sidebar items (stacked vertically, evenly distributed) -->
<mxCell id="security-permission" value="Permissions"
        style="rounded=1;whiteSpace=wrap;html=1;fillColor=#ffffff;strokeColor=#b85450;fontSize=14;"
        vertex="1" parent="1">
  <mxGeometry x="1000" y="110" width="90" height="135" as="geometry" />
</mxCell>
<mxCell id="security-protection" value="Protection"
        style="rounded=1;whiteSpace=wrap;html=1;fillColor=#ffffff;strokeColor=#b85450;fontSize=14;"
        vertex="1" parent="1">
  <mxGeometry x="1000" y="265" width="90" height="135" as="geometry" />
</mxCell>
```

Do not leave a large empty tail at the bottom of the sidebar. Either shrink the sidebar frame to fit the title + items, or recompute vertical distribution so top/bottom margins and item gaps are balanced.

### Layered Block Layout Calculation

For a diagram with N layers:
```
// Layer positions
layer[0].y = MARGIN + 20                           // first layer y (60)
layer[i].y = layer[i-1].y + layer[i-1].height + LAYER_GAP

// Background covers everything
background.width = SIDEBAR_X + SIDEBAR_W - MARGIN  // 1080
background.height = layer[N-1].y + layer[N-1].height - MARGIN + 20

// Each layer: label + container at same y, same height
label[i].x = LABEL_X                               // 60
label[i].width = LABEL_W                            // 100
label[i].height = layer[i].height
container[i].x = CONTAINER_X                        // 180
container[i].width = CONTAINER_W                    // 790
container[i].height = layer[i].height

// Sub-groups inside container: split container width
// For 2 sub-groups side by side:
subgroup[0].x = container.x + 20
subgroup[0].width = (container.width - 60) / 2      // ~365
subgroup[1].x = subgroup[0].x + subgroup[0].width + 20
subgroup[1].width = subgroup[0].width

// Items inside sub-group: grid layout
// header takes ~40px from top (spacingTop=8 + fontSize=16)
item_start_y = subgroup.y + SUBGROUP_HEADER_H
item[row][col].x = subgroup.x + 12 + col * (item_width + ITEM_GAP_H)
item[row][col].y = item_start_y + row * (ITEM_H + ITEM_GAP_V)

// Dense-fill check for subgroup rows
row_inner_width = subgroup.width - 24
row_content_width = cols * item_width + (cols - 1) * ITEM_GAP_H
row_side_pad = (row_inner_width - row_content_width) / 2
// Keep row_side_pad close to ITEM_GAP_H; if it is much larger, widen items,
// reduce subgroup width, or rebalance columns/rows.

// For incomplete last rows, recenter the last row using the actual item count in that row.
last_row_content_width = last_row_cols * item_width + (last_row_cols - 1) * ITEM_GAP_H
last_row_start_x = subgroup.x + (subgroup.width - last_row_content_width) / 2

// Sidebar spans full height alongside all layers
sidebar.y = layer[0].y
sidebar.height = layer[N-1].y + layer[N-1].height - layer[0].y

// If the sidebar spans full height, distribute items evenly rather than pinning them to the top:
sidebar_content_top = sidebar.y + SIDEBAR_PAD_Y + SIDEBAR_TITLE_H + SIDEBAR_TITLE_GAP
usable_sidebar_h = sidebar.height - (SIDEBAR_PAD_Y * 2) - SIDEBAR_TITLE_H - SIDEBAR_TITLE_GAP
slot_gap = (usable_sidebar_h - item_count * item_h) / (item_count - 1)
// If slot_gap is too large, prefer shrinking sidebar.height to content-fit:
sidebar.height = SIDEBAR_PAD_Y * 2 + SIDEBAR_TITLE_H + SIDEBAR_TITLE_GAP +
                 item_count * item_h + (item_count - 1) * desired_gap
```
