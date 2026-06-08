---
name: bruce-drawio
description: |
  **Use this skill** when the user wants to create any diagram: flowchart, architecture, UML (sequence/class), ER, mindmap, network topology, or any visual diagram.

  Trigger words: "draw", "diagram", "flowchart", "architecture", "UML", "sequence diagram", "mindmap", "ER diagram", "network topology", "visualize", "draw.io", "drawio".

  Workflow: understand requirements -> generate drawio XML directly -> self-review -> CLI export PNG/SVG/PDF.
---

# Draw.io Diagram Generator

## Workflow

```
1. Understand requirements  -> determine diagram type, elements, relationships
2. Generate XML directly    -> write drawio XML (read only the relevant section from references/best-practices.md)
3. Self-review (DO NOT SKIP) -> re-read XML and fix any issues found
4. Save .drawio file        -> write to user's working directory
5. CLI export               -> call draw.io desktop CLI to export image
6. Deliver to user          -> show image + provide editable .drawio file
```

## Step 1: Understand Requirements

Determine:
- **Diagram type**: flowchart / architecture / uml-sequence / uml-class / er / mindmap / network
- **Key elements**: nodes, components, participants, entities
- **Relationships**: connections, dependencies, flow direction
- **Output format**: PNG (default) / SVG / PDF
- **Language**: match the user's language for labels

## Step 2: Generate XML

**You MUST write the XML directly.** Do not call any script to generate it.

### Base XML Structure

```xml
<?xml version="1.0" encoding="UTF-8"?>
<mxfile host="app.diagrams.net" agent="drawio-skill" version="21.0.0" type="device">
  <diagram name="DiagramName" id="diagram-1">
    <mxGraphModel dx="1422" dy="762"
                   grid="1" gridSize="10"
                   guides="1" tooltips="1" connect="1"
                   arrows="1" fold="1"
                   page="1" pageScale="1"
                   pageWidth="1600" pageHeight="1200"
                   math="0" shadow="0">
      <root>
        <mxCell id="0" />
        <mxCell id="1" parent="0" />

        <!-- nodes and edges here -->

      </root>
    </mxGraphModel>
  </diagram>
</mxfile>
```

### Node Template

```xml
<mxCell id="node-1" value="Label"
        style="rounded=1;whiteSpace=wrap;html=1;fillColor=#dae8fc;strokeColor=#6c8ebf;fontSize=14;"
        vertex="1" parent="1">
  <mxGeometry x="100" y="100" width="160" height="60" as="geometry" />
</mxCell>
```

### Text Content Rules

- For multi-line labels, encode line breaks as `&#xa;` inside `value`, for example `value="API&#xa;Gateway"`
- Do not write literal `\n` inside `value`; draw.io will render it as backslash + n text
- Keep `html=1` on nodes, but still use `&#xa;` as the default line-break form for predictable output

### Edge Template

```xml
<mxCell id="edge-1" value=""
        style="edgeStyle=orthogonalEdgeStyle;rounded=1;orthogonalLoop=1;jettySize=auto;html=1;exitX=0.5;exitY=1;exitDx=0;exitDy=0;entryX=0.5;entryY=0;entryDx=0;entryDy=0;"
        edge="1" parent="1" source="node-1" target="node-2">
  <mxGeometry relative="1" as="geometry" />
</mxCell>
```

## Step 3: Layout Rules (CRITICAL for beautiful output)

### General Principles

1. **Grid alignment**: all x, y coordinates must be multiples of 10 (snap to grid)
2. **Generous spacing**: minimum 80px gap between node edges (not centers)
3. **Center alignment**: nodes in the same column share the same x; nodes in the same row share the same y
4. **Consistent sizing**: same-type nodes use identical width and height
5. **Page margins**: keep at least 60px from the canvas edge (pageWidth/pageHeight)
6. **Use `whiteSpace=wrap;html=1;`** on all nodes so long text wraps instead of overflowing
7. **Balanced gutters**: outer padding around a row/column should visually match the internal gaps; avoid one oversized blank side
8. **Symmetry first**: centered groups should have roughly equal left/right and top/bottom whitespace
9. **Dense fill**: containers, sub-groups, and sidebars should fit content plus consistent padding; do not leave large dead zones just because the canvas is large

### Layout by Diagram Type

| Type | Direction | Primary axis | Spacing (between edges) | Alignment |
|------|-----------|-------------|------------------------|-----------|
| Flowchart | Top-to-bottom | Y increases | 100px vertical | Center x |
| Architecture | Layered block (preferred) | Y increases | 20px between layers | Left label + container rows |
| UML Sequence | Left-to-right participants | X increases | 200px horizontal | Top-aligned |
| UML Class | Grid / top-to-bottom | Y increases | 100px vertical, 80px horizontal | Left-aligned columns |
| ER Diagram | Spread / grid | Both axes | 120px both | Grid-aligned |
| Mindmap | Center-outward radial | Both axes | 150px from center per level | Radial symmetric |
| Network | Hierarchical layers | Y increases | 100px vertical, 120px horizontal | Center each layer |

### Anti-Overlap Checklist

Before finalizing coordinates, verify:
- No two nodes' bounding boxes overlap (check x, y, width, height)
- Edge labels don't overlap with nodes
- Decision branches (Yes/No) go in clearly different directions
- For flowcharts with branches: main path goes down, alternate path goes right (or left)
- For wide diagrams: increase `pageWidth` in mxGraphModel; for tall ones increase `pageHeight`

### Calculating Coordinates

Use this formula to center N items horizontally in a row:

```
total_width = N * node_width + (N - 1) * gap
start_x = (pageWidth - total_width) / 2
item[i].x = start_x + i * (node_width + gap)
```

For vertical centering in a column, apply the same logic to Y axis.

For rows inside a fixed-width container, also check fill density:

```
inner_width = container_width - 2 * side_pad
gap = (inner_width - N * item_width) / (N - 1)
```

If `gap` is much larger than the item width, or side padding is much larger than `gap`, adjust one of:
- increase item width moderately
- increase item count per row only if still readable
- reduce container width
- split into multiple balanced rows

For incomplete last rows, center the remaining items instead of left-aligning them and leaving a large blank tail.

### Standard Sizes

| Element | Width | Height |
|---------|-------|--------|
| Standard node | 160 | 60 |
| Decision (rhombus) | 160 | 80 |
| Database (cylinder) | 140 | 80 |
| Actor (UML) | 40 | 60 |
| Start/End (rounded) | 160 | 60 |
| Mindmap center | 180 | 80 |
| Mindmap branch | 140 | 50 |
| Mindmap leaf | 120 | 40 |
| ER table header | 200 | varies |
| Group/container | auto | auto |

## Step 4: Self-Review (DO NOT SKIP)

After generating XML, re-read your output and check each item below. If any check fails, fix the XML before proceeding. This step catches the most common rendering bugs — skipping it results in broken diagrams.

**Structural checks:**
- [ ] All `id` values are unique across the entire file
- [ ] Every `mxCell` with `vertex="1"` has correct `parent` (usually `"1"`, but container children use the container ID)
- [ ] Every edge's `source` and `target` reference existing node IDs
- [ ] XML is well-formed: all tags closed, all attribute values quoted
- [ ] `mxGeometry` always has `as="geometry"` attribute

**Layout checks** (these are the most common failures — actually verify the numbers):
- [ ] No two non-container nodes overlap: for each pair, confirm their bounding boxes (x, y, x+width, y+height) don't intersect
- [ ] All coordinates (x, y) are multiples of 10 — scan every `mxGeometry` element
- [ ] Page dimensions (pageWidth, pageHeight) are large enough for all content with margins
- [ ] Sibling items in the same row/column use equal sizes and equal gaps unless there is a clear reason not to
- [ ] Left/right padding and top/bottom padding inside each container are visually balanced; no obvious one-sided blank area
- [ ] Containers, sub-groups, and sidebars are sized to content plus padding; if a blank region is larger than a normal item gap or roughly a full item row, tighten the layout
- [ ] Incomplete last rows are centered or otherwise balanced; they are not stuck to one side with a large empty remainder

**Style checks:**
- [ ] All nodes include `whiteSpace=wrap;html=1;` in style
- [ ] Every multi-line label uses `&#xa;` inside `value`, never literal `\n`
- [ ] `fontSize=14` or larger for readability
- [ ] Edges use `edgeStyle=orthogonalEdgeStyle` for clean routing (except mindmaps which use `curved=1`)
- [ ] Decision nodes use `rhombus` shape; database nodes use `shape=cylinder3`

## Step 5: CLI Export (Cross-Platform)

### 5a. Detect draw.io

Run these commands **in order**, stop at the first one that succeeds:

```bash
# 1. Try PATH first (works if user installed globally)
which draw.io 2>/dev/null || which drawio 2>/dev/null
```

If that fails, check platform-specific default paths:

**macOS:**
```bash
ls /Applications/draw.io.app/Contents/MacOS/draw.io 2>/dev/null
```

**Windows (bash/MSYS2):**
```bash
# Check common install locations
ls "/c/Program Files/draw.io/draw.io.exe" 2>/dev/null || \
ls "$LOCALAPPDATA/Programs/draw.io/draw.io.exe" 2>/dev/null
```

**Linux:**
```bash
ls /usr/bin/drawio 2>/dev/null || ls /snap/bin/drawio 2>/dev/null
```

### 5b. If not found, guide installation

Tell the user draw.io is not installed and suggest:

| Platform | Install Command |
|----------|----------------|
| macOS | `brew install --cask drawio` |
| Windows | `winget install JGraph.Draw` |
| Linux | `snap install drawio` |
| All | Download from https://github.com/jgraph/drawio-desktop/releases |

Do NOT auto-install without user confirmation.

### 5c. Export

Use the detected path (stored as `$DRAWIO`) to export:

```bash
"$DRAWIO" -x -f png --scale 2 -o output.png diagram.drawio
```

### Export flags

| Flag | Purpose |
|------|---------|
| `-x` | Export mode (no GUI) |
| `-f png/svg/pdf` | Output format |
| `-o path` | Output file path |
| `--scale 2` | 2x resolution for crisp PNG |
| `--border 20` | Add border padding (px) |
| `--width 1600` | Constrain output width |
| `-p 0` | Export specific page (0-indexed) |
| `--crop` | Crop to diagram content |

## Step 6: Deliver to User

After export:
- Show the exported image
- Tell the user the .drawio file location (can be edited at https://app.diagrams.net)
- Mention the export format used

## File Naming

- Lowercase + hyphens: `ecommerce-order-flow.drawio`
- No Chinese characters, spaces, or special characters in filenames
- Output image uses same base name: `ecommerce-order-flow.png`

## Architecture Diagram: Layered Block Style

For architecture diagrams, use the **Layered Block Style** — see `references/best-practices.md` for full templates and layout constants. This is the preferred style: structured block layout with no arrows, horizontal layers, left label column, and optional cross-cutting sidebar.

## Modifying Existing Diagrams

When the user is not satisfied with the result and asks for modifications:
1. **Read the existing .drawio file** first to understand current structure
2. **Edit based on the existing XML** — do not regenerate from scratch
3. Apply the user's requested changes while preserving the overall layout and style
4. Run the same self-review and export steps

## Reference

`references/best-practices.md` contains:
- **General Rules** — ID management, style essentials, common mistakes
- **Architecture Diagram Templates (Layered Block Style)** — the preferred architecture style with full XML templates and layout constants

For other diagram types (flowchart, UML, ER, mindmap, network, etc.), generate appropriate draw.io XML directly based on your knowledge. Read the "General Rules" section for basic formatting guidance.
