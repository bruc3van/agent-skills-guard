# Draw.io Complete XML Examples

These are ready-to-use, tested XML examples demonstrating proper structure, layout, and styling.

For multi-line text in any `value` attribute, use `&#xa;` instead of literal `\n`.

## Example 1: Simple Flowchart (Order Process)

```xml
<?xml version="1.0" encoding="UTF-8"?>
<mxfile host="app.diagrams.net" agent="drawio-skill" version="21.0.0" type="device">
  <diagram name="Order Flow" id="order-flow">
    <mxGraphModel dx="1422" dy="762" grid="1" gridSize="10" guides="1" tooltips="1"
                   connect="1" arrows="1" fold="1" page="1" pageScale="1"
                   pageWidth="1000" pageHeight="800" math="0" shadow="0">
      <root>
        <mxCell id="0" />
        <mxCell id="1" parent="0" />

        <!-- Start -->
        <mxCell id="start" value="User Places Order"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#d5e8d4;strokeColor=#82b366;fontSize=14;fontStyle=1;arcSize=50;"
                vertex="1" parent="1">
          <mxGeometry x="340" y="40" width="180" height="60" as="geometry" />
        </mxCell>

        <!-- Process: Create Order -->
        <mxCell id="process-create" value="Create Order Record"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#dae8fc;strokeColor=#6c8ebf;fontSize=14;"
                vertex="1" parent="1">
          <mxGeometry x="350" y="160" width="160" height="60" as="geometry" />
        </mxCell>

        <!-- Decision: Stock Check -->
        <mxCell id="decision-stock" value="In Stock?"
                style="rhombus;whiteSpace=wrap;html=1;fillColor=#fff2cc;strokeColor=#d6b656;fontSize=14;"
                vertex="1" parent="1">
          <mxGeometry x="350" y="280" width="160" height="80" as="geometry" />
        </mxCell>

        <!-- Process: Payment -->
        <mxCell id="process-pay" value="Process Payment"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#dae8fc;strokeColor=#6c8ebf;fontSize=14;"
                vertex="1" parent="1">
          <mxGeometry x="350" y="420" width="160" height="60" as="geometry" />
        </mxCell>

        <!-- Error: Out of Stock -->
        <mxCell id="error-stock" value="Notify: Out of Stock"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#f8cecc;strokeColor=#b85450;fontSize=14;"
                vertex="1" parent="1">
          <mxGeometry x="600" y="290" width="160" height="60" as="geometry" />
        </mxCell>

        <!-- Process: Ship -->
        <mxCell id="process-ship" value="Ship Order"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#dae8fc;strokeColor=#6c8ebf;fontSize=14;"
                vertex="1" parent="1">
          <mxGeometry x="350" y="540" width="160" height="60" as="geometry" />
        </mxCell>

        <!-- End -->
        <mxCell id="end" value="Order Complete"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#d5e8d4;strokeColor=#82b366;fontSize=14;fontStyle=1;arcSize=50;"
                vertex="1" parent="1">
          <mxGeometry x="350" y="660" width="160" height="60" as="geometry" />
        </mxCell>

        <!-- Edges -->
        <mxCell id="e1" style="edgeStyle=orthogonalEdgeStyle;rounded=1;orthogonalLoop=1;jettySize=auto;html=1;exitX=0.5;exitY=1;exitDx=0;exitDy=0;entryX=0.5;entryY=0;entryDx=0;entryDy=0;"
                edge="1" parent="1" source="start" target="process-create">
          <mxGeometry relative="1" as="geometry" />
        </mxCell>
        <mxCell id="e2" style="edgeStyle=orthogonalEdgeStyle;rounded=1;orthogonalLoop=1;jettySize=auto;html=1;exitX=0.5;exitY=1;exitDx=0;exitDy=0;entryX=0.5;entryY=0;entryDx=0;entryDy=0;"
                edge="1" parent="1" source="process-create" target="decision-stock">
          <mxGeometry relative="1" as="geometry" />
        </mxCell>
        <mxCell id="e3" value="Yes"
                style="edgeStyle=orthogonalEdgeStyle;rounded=1;orthogonalLoop=1;jettySize=auto;html=1;exitX=0.5;exitY=1;exitDx=0;exitDy=0;entryX=0.5;entryY=0;entryDx=0;entryDy=0;fontSize=12;fontStyle=1;"
                edge="1" parent="1" source="decision-stock" target="process-pay">
          <mxGeometry relative="1" as="geometry" />
        </mxCell>
        <mxCell id="e4" value="No"
                style="edgeStyle=orthogonalEdgeStyle;rounded=1;orthogonalLoop=1;jettySize=auto;html=1;exitX=1;exitY=0.5;exitDx=0;exitDy=0;entryX=0;entryY=0.5;entryDx=0;entryDy=0;fontSize=12;fontStyle=1;"
                edge="1" parent="1" source="decision-stock" target="error-stock">
          <mxGeometry relative="1" as="geometry" />
        </mxCell>
        <mxCell id="e5" style="edgeStyle=orthogonalEdgeStyle;rounded=1;orthogonalLoop=1;jettySize=auto;html=1;exitX=0.5;exitY=1;exitDx=0;exitDy=0;entryX=0.5;entryY=0;entryDx=0;entryDy=0;"
                edge="1" parent="1" source="process-pay" target="process-ship">
          <mxGeometry relative="1" as="geometry" />
        </mxCell>
        <mxCell id="e6" style="edgeStyle=orthogonalEdgeStyle;rounded=1;orthogonalLoop=1;jettySize=auto;html=1;exitX=0.5;exitY=1;exitDx=0;exitDy=0;entryX=0.5;entryY=0;entryDx=0;entryDy=0;"
                edge="1" parent="1" source="process-ship" target="end">
          <mxGeometry relative="1" as="geometry" />
        </mxCell>
      </root>
    </mxGraphModel>
  </diagram>
</mxfile>
```

## Example 2: Layered Block Architecture (Preferred Style)

This is the **default architecture diagram style**: structured block layout with left label column,
layer containers, sub-groups, and optional cross-cutting sidebar. No connecting arrows.

```xml
<?xml version="1.0" encoding="UTF-8"?>
<mxfile host="app.diagrams.net" agent="drawio-skill" version="21.0.0" type="device">
  <diagram name="Platform Architecture" id="arch-1">
    <mxGraphModel dx="1428" dy="842" grid="1" gridSize="10" guides="1" tooltips="1"
                   connect="1" arrows="1" fold="1" page="1" pageScale="1"
                   pageWidth="1169" pageHeight="827" math="0" shadow="0">
      <root>
        <mxCell id="0" />
        <mxCell id="1" parent="0" />

        <!-- Background -->
        <mxCell id="background" value=""
                style="rounded=0;whiteSpace=wrap;html=1;fillColor=#f5f5f5;strokeColor=none;"
                vertex="1" parent="1">
          <mxGeometry x="40" y="40" width="1080" height="700" as="geometry" />
        </mxCell>

        <!-- Cross-cutting sidebar (Security) -->
        <mxCell id="security-frame" value=""
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#f8cecc;strokeColor=#b85450;
                       fontSize=16;verticalAlign=middle;align=center;opacity=30;dashed=1;"
                vertex="1" parent="1">
          <mxGeometry x="990" y="60" width="110" height="660" as="geometry" />
        </mxCell>
        <mxCell id="security-title" value="Security"
                style="text;html=1;strokeColor=none;fillColor=none;align=center;verticalAlign=middle;
                       whiteSpace=wrap;rounded=0;fontSize=16;fontStyle=1;fontColor=#b85450"
                vertex="1" parent="1">
          <mxGeometry x="1015" y="70" width="60" height="30" as="geometry" />
        </mxCell>
        <mxCell id="security-auth" value="Auth"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#ffffff;strokeColor=#b85450;fontSize=14;"
                vertex="1" parent="1">
          <mxGeometry x="1000" y="110" width="90" height="135" as="geometry" />
        </mxCell>
        <mxCell id="security-protection" value="Protection"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#ffffff;strokeColor=#b85450;fontSize=14;"
                vertex="1" parent="1">
          <mxGeometry x="1000" y="265" width="90" height="135" as="geometry" />
        </mxCell>
        <mxCell id="security-isolation" value="Isolation"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#ffffff;strokeColor=#b85450;fontSize=14;"
                vertex="1" parent="1">
          <mxGeometry x="1000" y="420" width="90" height="135" as="geometry" />
        </mxCell>
        <mxCell id="security-audit" value="Audit"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#ffffff;strokeColor=#b85450;fontSize=14;"
                vertex="1" parent="1">
          <mxGeometry x="1000" y="575" width="90" height="135" as="geometry" />
        </mxCell>

        <!-- Layer 1: Scene Layer -->
        <mxCell id="layer-scene" value="Scene&#xa;Layer"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#dae8fc;strokeColor=#6c8ebf;
                       fontSize=18;fontStyle=1;verticalAlign=middle;align=center;"
                vertex="1" parent="1">
          <mxGeometry x="60" y="60" width="100" height="110" as="geometry" />
        </mxCell>
        <mxCell id="scene-container" value=""
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#dae8fc;strokeColor=#6c8ebf;opacity=60;"
                vertex="1" parent="1">
          <mxGeometry x="180" y="60" width="790" height="110" as="geometry" />
        </mxCell>
        <mxCell id="scene-1" value="Smart Office"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#ffffff;strokeColor=#999999;fontSize=14;"
                vertex="1" parent="1">
          <mxGeometry x="200" y="85" width="140" height="60" as="geometry" />
        </mxCell>
        <mxCell id="scene-2" value="Knowledge Base"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#ffffff;strokeColor=#999999;fontSize=14;"
                vertex="1" parent="1">
          <mxGeometry x="360" y="85" width="140" height="60" as="geometry" />
        </mxCell>
        <mxCell id="scene-3" value="Industry Q&amp;A"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#ffffff;strokeColor=#999999;fontSize=14;"
                vertex="1" parent="1">
          <mxGeometry x="520" y="85" width="140" height="60" as="geometry" />
        </mxCell>
        <mxCell id="scene-4" value="Customer Service"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#ffffff;strokeColor=#999999;fontSize=14;"
                vertex="1" parent="1">
          <mxGeometry x="680" y="85" width="140" height="60" as="geometry" />
        </mxCell>
        <mxCell id="scene-more" value="..."
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#ffffff;strokeColor=#999999;fontSize=16;"
                vertex="1" parent="1">
          <mxGeometry x="840" y="85" width="110" height="60" as="geometry" />
        </mxCell>

        <!-- Layer 2: Application Layer -->
        <mxCell id="layer-app" value="Application&#xa;Layer"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#dae8fc;strokeColor=#6c8ebf;
                       fontSize=18;fontStyle=1;verticalAlign=middle;align=center;"
                vertex="1" parent="1">
          <mxGeometry x="60" y="190" width="100" height="170" as="geometry" />
        </mxCell>
        <mxCell id="app-container" value=""
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#dae8fc;strokeColor=#6c8ebf;opacity=60;"
                vertex="1" parent="1">
          <mxGeometry x="180" y="190" width="790" height="170" as="geometry" />
        </mxCell>
        <!-- Sub-group: Open Applications -->
        <mxCell id="app-open" value="Open Applications"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#dae8fc;strokeColor=#6c8ebf;
                       fontSize=16;verticalAlign=top;align=center;spacingTop=8;"
                vertex="1" parent="1">
          <mxGeometry x="200" y="210" width="330" height="130" as="geometry" />
        </mxCell>
        <mxCell id="app-workspace" value="Workspace"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#ffffff;strokeColor=#999999;fontSize=14;"
                vertex="1" parent="1">
          <mxGeometry x="212" y="250" width="95" height="35" as="geometry" />
        </mxCell>
        <mxCell id="app-mgmt" value="Admin Panel"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#ffffff;strokeColor=#999999;fontSize=14;"
                vertex="1" parent="1">
          <mxGeometry x="317" y="250" width="95" height="35" as="geometry" />
        </mxCell>
        <mxCell id="app-channel" value="Channels"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#ffffff;strokeColor=#999999;fontSize=14;"
                vertex="1" parent="1">
          <mxGeometry x="422" y="250" width="95" height="35" as="geometry" />
        </mxCell>
        <mxCell id="app-app-mgmt" value="App Mgmt"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#ffffff;strokeColor=#999999;fontSize=14;"
                vertex="1" parent="1">
          <mxGeometry x="212" y="295" width="95" height="35" as="geometry" />
        </mxCell>
        <mxCell id="app-kb" value="KB Mgmt"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#ffffff;strokeColor=#999999;fontSize=14;"
                vertex="1" parent="1">
          <mxGeometry x="317" y="295" width="95" height="35" as="geometry" />
        </mxCell>
        <mxCell id="app-api" value="Open API"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#ffffff;strokeColor=#999999;fontSize=14;"
                vertex="1" parent="1">
          <mxGeometry x="422" y="295" width="95" height="35" as="geometry" />
        </mxCell>
        <!-- Sub-group: Smart Tools -->
        <mxCell id="app-tools" value="Smart Tools"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#dae8fc;strokeColor=#6c8ebf;
                       fontSize=16;verticalAlign=top;align=center;spacingTop=8;"
                vertex="1" parent="1">
          <mxGeometry x="550" y="210" width="400" height="130" as="geometry" />
        </mxCell>
        <mxCell id="tool-doc" value="Doc Processing"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#ffffff;strokeColor=#999999;fontSize=14;"
                vertex="1" parent="1">
          <mxGeometry x="570" y="250" width="110" height="35" as="geometry" />
        </mxCell>
        <mxCell id="tool-writing" value="Smart Writing"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#ffffff;strokeColor=#999999;fontSize=14;"
                vertex="1" parent="1">
          <mxGeometry x="700" y="250" width="110" height="35" as="geometry" />
        </mxCell>
        <mxCell id="tool-office" value="Office Assist"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#ffffff;strokeColor=#999999;fontSize=14;"
                vertex="1" parent="1">
          <mxGeometry x="830" y="250" width="100" height="35" as="geometry" />
        </mxCell>
        <mxCell id="tool-legal" value="Legal Expert"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#ffffff;strokeColor=#999999;fontSize=14;"
                vertex="1" parent="1">
          <mxGeometry x="640" y="295" width="100" height="35" as="geometry" />
        </mxCell>
        <mxCell id="tool-security" value="Security Expert"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#ffffff;strokeColor=#999999;fontSize=14;"
                vertex="1" parent="1">
          <mxGeometry x="760" y="295" width="100" height="35" as="geometry" />
        </mxCell>

        <!-- Layer 3: Support Layer -->
        <mxCell id="layer-support" value="Support&#xa;Layer"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#dae8fc;strokeColor=#6c8ebf;
                       fontSize=18;fontStyle=1;verticalAlign=middle;align=center;"
                vertex="1" parent="1">
          <mxGeometry x="60" y="380" width="100" height="170" as="geometry" />
        </mxCell>
        <mxCell id="support-container" value=""
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#dae8fc;strokeColor=#6c8ebf;opacity=60;"
                vertex="1" parent="1">
          <mxGeometry x="180" y="380" width="790" height="170" as="geometry" />
        </mxCell>
        <mxCell id="llm-group" value="LLM Integration"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#dae8fc;strokeColor=#6c8ebf;
                       fontSize=16;verticalAlign=top;align=center;spacingTop=8;"
                vertex="1" parent="1">
          <mxGeometry x="200" y="400" width="330" height="130" as="geometry" />
        </mxCell>
        <mxCell id="llm-models" value="Model Access"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#ffffff;strokeColor=#999999;fontSize=14;"
                vertex="1" parent="1">
          <mxGeometry x="220" y="440" width="140" height="35" as="geometry" />
        </mxCell>
        <mxCell id="llm-context" value="Context Mgmt"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#ffffff;strokeColor=#999999;fontSize=14;"
                vertex="1" parent="1">
          <mxGeometry x="370" y="440" width="140" height="35" as="geometry" />
        </mxCell>
        <mxCell id="llm-adapter" value="Unified Adapter"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#ffffff;strokeColor=#999999;fontSize=14;"
                vertex="1" parent="1">
          <mxGeometry x="220" y="485" width="140" height="35" as="geometry" />
        </mxCell>
        <mxCell id="llm-prompt" value="Prompt Mgmt"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#ffffff;strokeColor=#999999;fontSize=14;"
                vertex="1" parent="1">
          <mxGeometry x="370" y="485" width="140" height="35" as="geometry" />
        </mxCell>
        <mxCell id="rag-group" value="RAG Enhancement"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#dae8fc;strokeColor=#6c8ebf;
                       fontSize=16;verticalAlign=top;align=center;spacingTop=8;"
                vertex="1" parent="1">
          <mxGeometry x="550" y="400" width="400" height="130" as="geometry" />
        </mxCell>
        <mxCell id="rag-kb" value="Multi-KB"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#ffffff;strokeColor=#999999;fontSize=14;"
                vertex="1" parent="1">
          <mxGeometry x="570" y="440" width="115" height="35" as="geometry" />
        </mxCell>
        <mxCell id="rag-ocr" value="OCR"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#ffffff;strokeColor=#999999;fontSize=14;"
                vertex="1" parent="1">
          <mxGeometry x="695" y="440" width="115" height="35" as="geometry" />
        </mxCell>
        <mxCell id="rag-vector" value="Vectorization"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#ffffff;strokeColor=#999999;fontSize=14;"
                vertex="1" parent="1">
          <mxGeometry x="820" y="440" width="115" height="35" as="geometry" />
        </mxCell>
        <mxCell id="rag-search" value="Hybrid Search"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#ffffff;strokeColor=#999999;fontSize=14;"
                vertex="1" parent="1">
          <mxGeometry x="570" y="485" width="115" height="35" as="geometry" />
        </mxCell>
        <mxCell id="rag-rerank" value="Re-ranking"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#ffffff;strokeColor=#999999;fontSize=14;"
                vertex="1" parent="1">
          <mxGeometry x="695" y="485" width="115" height="35" as="geometry" />
        </mxCell>
        <mxCell id="rag-source" value="Source Tracing"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#ffffff;strokeColor=#999999;fontSize=14;"
                vertex="1" parent="1">
          <mxGeometry x="820" y="485" width="115" height="35" as="geometry" />
        </mxCell>

        <!-- Layer 4: Infrastructure Layer -->
        <mxCell id="layer-infra" value="Infra&#xa;Layer"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#dae8fc;strokeColor=#6c8ebf;
                       fontSize=18;fontStyle=1;verticalAlign=middle;align=center;"
                vertex="1" parent="1">
          <mxGeometry x="60" y="570" width="100" height="150" as="geometry" />
        </mxCell>
        <mxCell id="infra-container" value=""
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#dae8fc;strokeColor=#6c8ebf;opacity=60;"
                vertex="1" parent="1">
          <mxGeometry x="180" y="570" width="790" height="150" as="geometry" />
        </mxCell>
        <mxCell id="infra-compute" value="Compute"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#dae8fc;strokeColor=#6c8ebf;
                       fontSize=16;verticalAlign=top;align=center;spacingTop=8;"
                vertex="1" parent="1">
          <mxGeometry x="200" y="590" width="230" height="110" as="geometry" />
        </mxCell>
        <mxCell id="compute-gpu" value="GPU/CPU Pool"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#ffffff;strokeColor=#999999;fontSize=14;"
                vertex="1" parent="1">
          <mxGeometry x="220" y="640" width="190" height="35" as="geometry" />
        </mxCell>
        <mxCell id="infra-storage" value="Storage"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#dae8fc;strokeColor=#6c8ebf;
                       fontSize=16;verticalAlign=top;align=center;spacingTop=8;"
                vertex="1" parent="1">
          <mxGeometry x="450" y="590" width="250" height="110" as="geometry" />
        </mxCell>
        <mxCell id="storage-file" value="File Store"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#ffffff;strokeColor=#999999;fontSize=14;"
                vertex="1" parent="1">
          <mxGeometry x="462" y="640" width="70" height="35" as="geometry" />
        </mxCell>
        <mxCell id="storage-rdb" value="RDBMS"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#ffffff;strokeColor=#999999;fontSize=14;"
                vertex="1" parent="1">
          <mxGeometry x="542" y="640" width="70" height="35" as="geometry" />
        </mxCell>
        <mxCell id="storage-vector" value="Vector DB"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#ffffff;strokeColor=#999999;fontSize=14;"
                vertex="1" parent="1">
          <mxGeometry x="622" y="640" width="70" height="35" as="geometry" />
        </mxCell>
        <mxCell id="infra-deploy" value="Deployment"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#dae8fc;strokeColor=#6c8ebf;
                       fontSize=16;verticalAlign=top;align=center;spacingTop=8;"
                vertex="1" parent="1">
          <mxGeometry x="720" y="590" width="230" height="110" as="geometry" />
        </mxCell>
        <mxCell id="deploy-cross" value="Cross-platform"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#ffffff;strokeColor=#999999;fontSize=14;"
                vertex="1" parent="1">
          <mxGeometry x="740" y="640" width="90" height="35" as="geometry" />
        </mxCell>
        <mxCell id="deploy-integration" value="Integration"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#ffffff;strokeColor=#999999;fontSize=14;"
                vertex="1" parent="1">
          <mxGeometry x="840" y="640" width="90" height="35" as="geometry" />
        </mxCell>
      </root>
    </mxGraphModel>
  </diagram>
</mxfile>
```

## Example 3: Mindmap (AI Agent)

```xml
<?xml version="1.0" encoding="UTF-8"?>
<mxfile host="app.diagrams.net" agent="drawio-skill" version="21.0.0" type="device">
  <diagram name="AI Agent Mindmap" id="mindmap-1">
    <mxGraphModel dx="1422" dy="762" grid="1" gridSize="10" guides="1" tooltips="1"
                   connect="1" arrows="1" fold="1" page="1" pageScale="1"
                   pageWidth="1400" pageHeight="900" math="0" shadow="0">
      <root>
        <mxCell id="0" />
        <mxCell id="1" parent="0" />

        <!-- Center -->
        <mxCell id="center" value="AI Agent"
                style="ellipse;whiteSpace=wrap;html=1;fillColor=#1ba1e2;strokeColor=#006EAF;fontSize=20;fontStyle=1;fontColor=#ffffff;shadow=1;"
                vertex="1" parent="1">
          <mxGeometry x="580" y="360" width="180" height="80" as="geometry" />
        </mxCell>

        <!-- Branch 1: Capabilities (right) -->
        <mxCell id="b1" value="Core Capabilities"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#dae8fc;strokeColor=#6c8ebf;fontSize=14;fontStyle=1;arcSize=50;"
                vertex="1" parent="1">
          <mxGeometry x="880" y="200" width="160" height="50" as="geometry" />
        </mxCell>
        <mxCell id="b1-l1" value="Perception"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#dae8fc;strokeColor=#6c8ebf;fontSize=12;arcSize=50;"
                vertex="1" parent="1">
          <mxGeometry x="1100" y="150" width="120" height="40" as="geometry" />
        </mxCell>
        <mxCell id="b1-l2" value="Reasoning"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#dae8fc;strokeColor=#6c8ebf;fontSize=12;arcSize=50;"
                vertex="1" parent="1">
          <mxGeometry x="1100" y="200" width="120" height="40" as="geometry" />
        </mxCell>
        <mxCell id="b1-l3" value="Action"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#dae8fc;strokeColor=#6c8ebf;fontSize=12;arcSize=50;"
                vertex="1" parent="1">
          <mxGeometry x="1100" y="250" width="120" height="40" as="geometry" />
        </mxCell>

        <!-- Branch 2: Applications (bottom-right) -->
        <mxCell id="b2" value="Applications"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#d5e8d4;strokeColor=#82b366;fontSize=14;fontStyle=1;arcSize=50;"
                vertex="1" parent="1">
          <mxGeometry x="880" y="500" width="160" height="50" as="geometry" />
        </mxCell>
        <mxCell id="b2-l1" value="Customer Service"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#d5e8d4;strokeColor=#82b366;fontSize=12;arcSize=50;"
                vertex="1" parent="1">
          <mxGeometry x="1100" y="460" width="130" height="40" as="geometry" />
        </mxCell>
        <mxCell id="b2-l2" value="Programming"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#d5e8d4;strokeColor=#82b366;fontSize=12;arcSize=50;"
                vertex="1" parent="1">
          <mxGeometry x="1100" y="510" width="130" height="40" as="geometry" />
        </mxCell>
        <mxCell id="b2-l3" value="Research"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#d5e8d4;strokeColor=#82b366;fontSize=12;arcSize=50;"
                vertex="1" parent="1">
          <mxGeometry x="1100" y="560" width="130" height="40" as="geometry" />
        </mxCell>

        <!-- Branch 3: Tech Stack (left) -->
        <mxCell id="b3" value="Tech Stack"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#fff2cc;strokeColor=#d6b656;fontSize=14;fontStyle=1;arcSize=50;"
                vertex="1" parent="1">
          <mxGeometry x="280" y="200" width="160" height="50" as="geometry" />
        </mxCell>
        <mxCell id="b3-l1" value="LLM"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#fff2cc;strokeColor=#d6b656;fontSize=12;arcSize=50;"
                vertex="1" parent="1">
          <mxGeometry x="120" y="150" width="120" height="40" as="geometry" />
        </mxCell>
        <mxCell id="b3-l2" value="RAG"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#fff2cc;strokeColor=#d6b656;fontSize=12;arcSize=50;"
                vertex="1" parent="1">
          <mxGeometry x="120" y="200" width="120" height="40" as="geometry" />
        </mxCell>
        <mxCell id="b3-l3" value="Tool Calling"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#fff2cc;strokeColor=#d6b656;fontSize=12;arcSize=50;"
                vertex="1" parent="1">
          <mxGeometry x="120" y="250" width="120" height="40" as="geometry" />
        </mxCell>

        <!-- Branch 4: Challenges (bottom-left) -->
        <mxCell id="b4" value="Challenges"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#e1d5e7;strokeColor=#9673a6;fontSize=14;fontStyle=1;arcSize=50;"
                vertex="1" parent="1">
          <mxGeometry x="280" y="500" width="160" height="50" as="geometry" />
        </mxCell>
        <mxCell id="b4-l1" value="Hallucination"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#e1d5e7;strokeColor=#9673a6;fontSize=12;arcSize=50;"
                vertex="1" parent="1">
          <mxGeometry x="120" y="460" width="120" height="40" as="geometry" />
        </mxCell>
        <mxCell id="b4-l2" value="Safety"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#e1d5e7;strokeColor=#9673a6;fontSize=12;arcSize=50;"
                vertex="1" parent="1">
          <mxGeometry x="120" y="510" width="120" height="40" as="geometry" />
        </mxCell>
        <mxCell id="b4-l3" value="Cost"
                style="rounded=1;whiteSpace=wrap;html=1;fillColor=#e1d5e7;strokeColor=#9673a6;fontSize=12;arcSize=50;"
                vertex="1" parent="1">
          <mxGeometry x="120" y="560" width="120" height="40" as="geometry" />
        </mxCell>

        <!-- Curved edges from center to branches -->
        <mxCell id="ec1" style="rounded=1;curved=1;html=1;endArrow=none;strokeWidth=3;strokeColor=#6c8ebf;"
                edge="1" parent="1" source="center" target="b1">
          <mxGeometry relative="1" as="geometry" />
        </mxCell>
        <mxCell id="ec2" style="rounded=1;curved=1;html=1;endArrow=none;strokeWidth=3;strokeColor=#82b366;"
                edge="1" parent="1" source="center" target="b2">
          <mxGeometry relative="1" as="geometry" />
        </mxCell>
        <mxCell id="ec3" style="rounded=1;curved=1;html=1;endArrow=none;strokeWidth=3;strokeColor=#d6b656;"
                edge="1" parent="1" source="center" target="b3">
          <mxGeometry relative="1" as="geometry" />
        </mxCell>
        <mxCell id="ec4" style="rounded=1;curved=1;html=1;endArrow=none;strokeWidth=3;strokeColor=#9673a6;"
                edge="1" parent="1" source="center" target="b4">
          <mxGeometry relative="1" as="geometry" />
        </mxCell>

        <!-- Branch to leaf edges -->
        <mxCell id="el1-1" style="rounded=1;curved=1;html=1;endArrow=none;strokeWidth=2;strokeColor=#6c8ebf;"
                edge="1" parent="1" source="b1" target="b1-l1"><mxGeometry relative="1" as="geometry" /></mxCell>
        <mxCell id="el1-2" style="rounded=1;curved=1;html=1;endArrow=none;strokeWidth=2;strokeColor=#6c8ebf;"
                edge="1" parent="1" source="b1" target="b1-l2"><mxGeometry relative="1" as="geometry" /></mxCell>
        <mxCell id="el1-3" style="rounded=1;curved=1;html=1;endArrow=none;strokeWidth=2;strokeColor=#6c8ebf;"
                edge="1" parent="1" source="b1" target="b1-l3"><mxGeometry relative="1" as="geometry" /></mxCell>
        <mxCell id="el2-1" style="rounded=1;curved=1;html=1;endArrow=none;strokeWidth=2;strokeColor=#82b366;"
                edge="1" parent="1" source="b2" target="b2-l1"><mxGeometry relative="1" as="geometry" /></mxCell>
        <mxCell id="el2-2" style="rounded=1;curved=1;html=1;endArrow=none;strokeWidth=2;strokeColor=#82b366;"
                edge="1" parent="1" source="b2" target="b2-l2"><mxGeometry relative="1" as="geometry" /></mxCell>
        <mxCell id="el2-3" style="rounded=1;curved=1;html=1;endArrow=none;strokeWidth=2;strokeColor=#82b366;"
                edge="1" parent="1" source="b2" target="b2-l3"><mxGeometry relative="1" as="geometry" /></mxCell>
        <mxCell id="el3-1" style="rounded=1;curved=1;html=1;endArrow=none;strokeWidth=2;strokeColor=#d6b656;"
                edge="1" parent="1" source="b3" target="b3-l1"><mxGeometry relative="1" as="geometry" /></mxCell>
        <mxCell id="el3-2" style="rounded=1;curved=1;html=1;endArrow=none;strokeWidth=2;strokeColor=#d6b656;"
                edge="1" parent="1" source="b3" target="b3-l2"><mxGeometry relative="1" as="geometry" /></mxCell>
        <mxCell id="el3-3" style="rounded=1;curved=1;html=1;endArrow=none;strokeWidth=2;strokeColor=#d6b656;"
                edge="1" parent="1" source="b3" target="b3-l3"><mxGeometry relative="1" as="geometry" /></mxCell>
        <mxCell id="el4-1" style="rounded=1;curved=1;html=1;endArrow=none;strokeWidth=2;strokeColor=#9673a6;"
                edge="1" parent="1" source="b4" target="b4-l1"><mxGeometry relative="1" as="geometry" /></mxCell>
        <mxCell id="el4-2" style="rounded=1;curved=1;html=1;endArrow=none;strokeWidth=2;strokeColor=#9673a6;"
                edge="1" parent="1" source="b4" target="b4-l2"><mxGeometry relative="1" as="geometry" /></mxCell>
        <mxCell id="el4-3" style="rounded=1;curved=1;html=1;endArrow=none;strokeWidth=2;strokeColor=#9673a6;"
                edge="1" parent="1" source="b4" target="b4-l3"><mxGeometry relative="1" as="geometry" /></mxCell>
      </root>
    </mxGraphModel>
  </diagram>
</mxfile>
```

## Example 4: ER Diagram (Blog System)

```xml
<?xml version="1.0" encoding="UTF-8"?>
<mxfile host="app.diagrams.net" agent="drawio-skill" version="21.0.0" type="device">
  <diagram name="Blog ER" id="er-1">
    <mxGraphModel dx="1422" dy="762" grid="1" gridSize="10" guides="1" tooltips="1"
                   connect="1" arrows="1" fold="1" page="1" pageScale="1"
                   pageWidth="1200" pageHeight="700" math="0" shadow="0">
      <root>
        <mxCell id="0" />
        <mxCell id="1" parent="0" />

        <!-- Users table -->
        <mxCell id="tbl-users" value="users"
                style="swimlane;fontStyle=1;childLayout=stackLayout;horizontal=1;startSize=30;horizontalStack=0;resizeParent=1;resizeParentMax=0;collapsible=0;marginBottom=0;fillColor=#dae8fc;strokeColor=#6c8ebf;fontSize=14;html=1;whiteSpace=wrap;"
                vertex="1" parent="1">
          <mxGeometry x="100" y="120" width="220" height="188" as="geometry" />
        </mxCell>
        <mxCell id="tbl-users-id" value="PK  id: INT"
                style="text;strokeColor=none;fillColor=none;align=left;verticalAlign=middle;spacingLeft=4;spacingRight=4;overflow=hidden;points=[[0,0.5],[1,0.5]];portConstraint=eastwest;fontStyle=1;fontSize=12;html=1;whiteSpace=wrap;"
                vertex="1" parent="tbl-users">
          <mxGeometry y="30" width="220" height="30" as="geometry" />
        </mxCell>
        <mxCell id="tbl-users-div" value=""
                style="line;strokeWidth=1;fillColor=none;align=left;verticalAlign=middle;spacingTop=-1;spacingLeft=3;spacingRight=3;rotatable=0;labelPosition=left;points=[];portConstraint=eastwest;strokeColor=#6c8ebf;"
                vertex="1" parent="tbl-users">
          <mxGeometry y="60" width="220" height="8" as="geometry" />
        </mxCell>
        <mxCell id="tbl-users-name" value="username: VARCHAR(50)"
                style="text;strokeColor=none;fillColor=none;align=left;verticalAlign=middle;spacingLeft=4;spacingRight=4;overflow=hidden;points=[[0,0.5],[1,0.5]];portConstraint=eastwest;fontSize=12;html=1;whiteSpace=wrap;"
                vertex="1" parent="tbl-users">
          <mxGeometry y="68" width="220" height="30" as="geometry" />
        </mxCell>
        <mxCell id="tbl-users-email" value="email: VARCHAR(100)"
                style="text;strokeColor=none;fillColor=none;align=left;verticalAlign=middle;spacingLeft=4;spacingRight=4;overflow=hidden;points=[[0,0.5],[1,0.5]];portConstraint=eastwest;fontSize=12;html=1;whiteSpace=wrap;"
                vertex="1" parent="tbl-users">
          <mxGeometry y="98" width="220" height="30" as="geometry" />
        </mxCell>
        <mxCell id="tbl-users-created" value="created_at: DATETIME"
                style="text;strokeColor=none;fillColor=none;align=left;verticalAlign=middle;spacingLeft=4;spacingRight=4;overflow=hidden;points=[[0,0.5],[1,0.5]];portConstraint=eastwest;fontSize=12;html=1;whiteSpace=wrap;"
                vertex="1" parent="tbl-users">
          <mxGeometry y="128" width="220" height="30" as="geometry" />
        </mxCell>
        <mxCell id="tbl-users-pwd" value="password_hash: VARCHAR(255)"
                style="text;strokeColor=none;fillColor=none;align=left;verticalAlign=middle;spacingLeft=4;spacingRight=4;overflow=hidden;points=[[0,0.5],[1,0.5]];portConstraint=eastwest;fontSize=12;html=1;whiteSpace=wrap;"
                vertex="1" parent="tbl-users">
          <mxGeometry y="158" width="220" height="30" as="geometry" />
        </mxCell>

        <!-- Articles table -->
        <mxCell id="tbl-articles" value="articles"
                style="swimlane;fontStyle=1;childLayout=stackLayout;horizontal=1;startSize=30;horizontalStack=0;resizeParent=1;resizeParentMax=0;collapsible=0;marginBottom=0;fillColor=#d5e8d4;strokeColor=#82b366;fontSize=14;html=1;whiteSpace=wrap;"
                vertex="1" parent="1">
          <mxGeometry x="460" y="120" width="220" height="188" as="geometry" />
        </mxCell>
        <mxCell id="tbl-articles-id" value="PK  id: INT"
                style="text;strokeColor=none;fillColor=none;align=left;verticalAlign=middle;spacingLeft=4;spacingRight=4;overflow=hidden;points=[[0,0.5],[1,0.5]];portConstraint=eastwest;fontStyle=1;fontSize=12;html=1;whiteSpace=wrap;"
                vertex="1" parent="tbl-articles">
          <mxGeometry y="30" width="220" height="30" as="geometry" />
        </mxCell>
        <mxCell id="tbl-articles-div" value=""
                style="line;strokeWidth=1;fillColor=none;align=left;verticalAlign=middle;spacingTop=-1;spacingLeft=3;spacingRight=3;rotatable=0;labelPosition=left;points=[];portConstraint=eastwest;strokeColor=#82b366;"
                vertex="1" parent="tbl-articles">
          <mxGeometry y="60" width="220" height="8" as="geometry" />
        </mxCell>
        <mxCell id="tbl-articles-title" value="title: VARCHAR(200)"
                style="text;strokeColor=none;fillColor=none;align=left;verticalAlign=middle;spacingLeft=4;spacingRight=4;overflow=hidden;points=[[0,0.5],[1,0.5]];portConstraint=eastwest;fontSize=12;html=1;whiteSpace=wrap;"
                vertex="1" parent="tbl-articles">
          <mxGeometry y="68" width="220" height="30" as="geometry" />
        </mxCell>
        <mxCell id="tbl-articles-content" value="content: TEXT"
                style="text;strokeColor=none;fillColor=none;align=left;verticalAlign=middle;spacingLeft=4;spacingRight=4;overflow=hidden;points=[[0,0.5],[1,0.5]];portConstraint=eastwest;fontSize=12;html=1;whiteSpace=wrap;"
                vertex="1" parent="tbl-articles">
          <mxGeometry y="98" width="220" height="30" as="geometry" />
        </mxCell>
        <mxCell id="tbl-articles-author" value="FK  author_id: INT"
                style="text;strokeColor=none;fillColor=none;align=left;verticalAlign=middle;spacingLeft=4;spacingRight=4;overflow=hidden;points=[[0,0.5],[1,0.5]];portConstraint=eastwest;fontStyle=2;fontSize=12;html=1;whiteSpace=wrap;"
                vertex="1" parent="tbl-articles">
          <mxGeometry y="128" width="220" height="30" as="geometry" />
        </mxCell>
        <mxCell id="tbl-articles-created" value="created_at: DATETIME"
                style="text;strokeColor=none;fillColor=none;align=left;verticalAlign=middle;spacingLeft=4;spacingRight=4;overflow=hidden;points=[[0,0.5],[1,0.5]];portConstraint=eastwest;fontSize=12;html=1;whiteSpace=wrap;"
                vertex="1" parent="tbl-articles">
          <mxGeometry y="158" width="220" height="30" as="geometry" />
        </mxCell>

        <!-- Comments table -->
        <mxCell id="tbl-comments" value="comments"
                style="swimlane;fontStyle=1;childLayout=stackLayout;horizontal=1;startSize=30;horizontalStack=0;resizeParent=1;resizeParentMax=0;collapsible=0;marginBottom=0;fillColor=#fff2cc;strokeColor=#d6b656;fontSize=14;html=1;whiteSpace=wrap;"
                vertex="1" parent="1">
          <mxGeometry x="820" y="120" width="220" height="188" as="geometry" />
        </mxCell>
        <mxCell id="tbl-comments-id" value="PK  id: INT"
                style="text;strokeColor=none;fillColor=none;align=left;verticalAlign=middle;spacingLeft=4;spacingRight=4;overflow=hidden;points=[[0,0.5],[1,0.5]];portConstraint=eastwest;fontStyle=1;fontSize=12;html=1;whiteSpace=wrap;"
                vertex="1" parent="tbl-comments">
          <mxGeometry y="30" width="220" height="30" as="geometry" />
        </mxCell>
        <mxCell id="tbl-comments-div" value=""
                style="line;strokeWidth=1;fillColor=none;align=left;verticalAlign=middle;spacingTop=-1;spacingLeft=3;spacingRight=3;rotatable=0;labelPosition=left;points=[];portConstraint=eastwest;strokeColor=#d6b656;"
                vertex="1" parent="tbl-comments">
          <mxGeometry y="60" width="220" height="8" as="geometry" />
        </mxCell>
        <mxCell id="tbl-comments-content" value="content: TEXT"
                style="text;strokeColor=none;fillColor=none;align=left;verticalAlign=middle;spacingLeft=4;spacingRight=4;overflow=hidden;points=[[0,0.5],[1,0.5]];portConstraint=eastwest;fontSize=12;html=1;whiteSpace=wrap;"
                vertex="1" parent="tbl-comments">
          <mxGeometry y="68" width="220" height="30" as="geometry" />
        </mxCell>
        <mxCell id="tbl-comments-article" value="FK  article_id: INT"
                style="text;strokeColor=none;fillColor=none;align=left;verticalAlign=middle;spacingLeft=4;spacingRight=4;overflow=hidden;points=[[0,0.5],[1,0.5]];portConstraint=eastwest;fontStyle=2;fontSize=12;html=1;whiteSpace=wrap;"
                vertex="1" parent="tbl-comments">
          <mxGeometry y="98" width="220" height="30" as="geometry" />
        </mxCell>
        <mxCell id="tbl-comments-user" value="FK  user_id: INT"
                style="text;strokeColor=none;fillColor=none;align=left;verticalAlign=middle;spacingLeft=4;spacingRight=4;overflow=hidden;points=[[0,0.5],[1,0.5]];portConstraint=eastwest;fontStyle=2;fontSize=12;html=1;whiteSpace=wrap;"
                vertex="1" parent="tbl-comments">
          <mxGeometry y="128" width="220" height="30" as="geometry" />
        </mxCell>
        <mxCell id="tbl-comments-created" value="created_at: DATETIME"
                style="text;strokeColor=none;fillColor=none;align=left;verticalAlign=middle;spacingLeft=4;spacingRight=4;overflow=hidden;points=[[0,0.5],[1,0.5]];portConstraint=eastwest;fontSize=12;html=1;whiteSpace=wrap;"
                vertex="1" parent="tbl-comments">
          <mxGeometry y="158" width="220" height="30" as="geometry" />
        </mxCell>

        <!-- Relationships -->
        <mxCell id="rel-user-article" value="1:N"
                style="edgeStyle=orthogonalEdgeStyle;rounded=1;orthogonalLoop=1;jettySize=auto;html=1;endArrow=ERmany;endFill=0;startArrow=ERmandOne;startFill=0;strokeColor=#666666;fontSize=12;"
                edge="1" parent="1" source="tbl-users" target="tbl-articles">
          <mxGeometry relative="1" as="geometry" />
        </mxCell>
        <mxCell id="rel-article-comment" value="1:N"
                style="edgeStyle=orthogonalEdgeStyle;rounded=1;orthogonalLoop=1;jettySize=auto;html=1;endArrow=ERmany;endFill=0;startArrow=ERmandOne;startFill=0;strokeColor=#666666;fontSize=12;"
                edge="1" parent="1" source="tbl-articles" target="tbl-comments">
          <mxGeometry relative="1" as="geometry" />
        </mxCell>
        <mxCell id="rel-user-comment" value="1:N"
                style="edgeStyle=orthogonalEdgeStyle;rounded=1;orthogonalLoop=1;jettySize=auto;html=1;endArrow=ERmany;endFill=0;startArrow=ERmandOne;startFill=0;strokeColor=#666666;fontSize=12;exitX=0.5;exitY=1;exitDx=0;exitDy=0;entryX=0.5;entryY=1;entryDx=0;entryDy=0;"
                edge="1" parent="1" source="tbl-users" target="tbl-comments">
          <mxGeometry relative="1" as="geometry">
            <Array as="points">
              <mxPoint x="210" y="400" />
              <mxPoint x="930" y="400" />
            </Array>
          </mxGeometry>
        </mxCell>
      </root>
    </mxGraphModel>
  </diagram>
</mxfile>
```

## Color Palette Reference

| Purpose | Fill | Stroke |
|---------|------|--------|
| Start/End/Success (green) | #d5e8d4 | #82b366 |
| Process/Service (blue) | #dae8fc | #6c8ebf |
| Decision/Warning (yellow) | #fff2cc | #d6b656 |
| Error/Client (red) | #f8cecc | #b85450 |
| Subprocess/Storage (purple) | #e1d5e7 | #9673a6 |
| Neutral/Default (gray) | #f5f5f5 | #666666 |
| Accent/Center (dark blue) | #1ba1e2 | #006EAF |
