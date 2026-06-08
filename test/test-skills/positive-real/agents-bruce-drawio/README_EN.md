English | [中文](README.md)

# Draw.io Diagram Generator Skill

Cross-platform draw.io (diagrams.net) diagram generation skill for AI coding agents (Claude Code, OpenClaw, etc.). The agent directly generates drawio XML, self-reviews it, then exports via CLI.

## Installation

Send the following prompt to your AI coding agent:

```
Help me install this skill: https://github.com/bruc3van/bruce-drawio
```

The agent will clone the repository and configure the skill automatically.

## Supported Diagram Types

| Type             | Description                                    | Trigger                        |
| ---------------- | ---------------------------------------------- | ------------------------------ |
| Flowchart        | Business process, approval flows, algorithms   | "draw a flowchart"             |
| Architecture     | System architecture, microservices, deployment | "draw an architecture diagram" |
| UML Sequence     | Interaction timelines between components       | "draw a sequence diagram"      |
| UML Class        | Class relationships, inheritance               | "draw a class diagram"         |
| ER Diagram       | Database design, entity relationships          | "draw an ER diagram"           |
| Mindmap          | Brainstorming, knowledge organization          | "draw a mindmap"               |
| Network Topology | Network architecture, device connectivity      | "draw a network topology"      |

## Platform Support

| Platform | Install Command                | Package Manager     |
| -------- | ------------------------------ | ------------------- |
| macOS    | `brew install --cask drawio` | Homebrew            |
| Windows  | `winget install JGraph.Draw` | winget / Chocolatey |
| Linux    | `snap install drawio`        | snap / manual       |

All platforms also support manual download from [draw.io releases](https://github.com/jgraph/drawio-desktop/releases).

## How It Works

1. User describes the diagram they want
2. Agent determines diagram type and elements
3. Agent generates complete drawio XML directly
4. Self-review checklist verifies correctness and layout
5. Saves `.drawio` file
6. CLI exports to PNG/SVG/PDF
7. Delivers image and editable source file

## Export Formats

| Format | Flag       | Use Case           |
| ------ | ---------- | ------------------ |
| PNG    | `-f png` | Default, universal |
| SVG    | `-f svg` | Scalable vector    |
| PDF    | `-f pdf` | Print / document   |

Use `--scale 2` for high-DPI PNG output.

## Project Structure

```
bruce-drawio/
  SKILL.md                      # Main skill document (workflow + rules)
  skill.json                    # Skill metadata
  references/
    best-practices.md           # XML templates, styles, layout rules
    examples.md                 # Complete working XML examples
  evals/
    evals.json                  # Test cases
```

## Architecture Diagram Style

Architecture diagrams use a **layered block layout** style by default:

- Gray background plate
- Left label column for each layer (e.g., "Scene Layer", "Application Layer")
- Blue semi-transparent layer containers with sub-groups
- White leaf nodes with gray borders
- Optional right-side cross-cutting sidebar (e.g., security, monitoring)
- Pure block diagram with no arrows; hierarchy expressed through spatial nesting

## Dependency Check

No separate script needed. The agent follows instructions in SKILL.md Step 5 to detect draw.io via shell commands (`which drawio`, checking default paths). If not found, it guides the user to install.

## Usage Examples

After installation, simply describe the diagram you want in natural language:

- "Draw an e-commerce order flowchart"
- "Draw a microservice architecture diagram"
- "Draw a user registration sequence diagram"
- "Draw a blog system ER diagram"
- "Draw an AI Agent mindmap"
